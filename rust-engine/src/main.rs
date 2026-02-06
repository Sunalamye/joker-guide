// 這些 API 是為未來擴展保留的公開介面
#![allow(dead_code)]

use std::env;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use dashmap::DashMap;

use rand::Rng;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};

use joker_env::proto::joker_env_server::{JokerEnv, JokerEnvServer};
use joker_env::proto::{
    Action, EnvInfo, GetSpecRequest, GetSpecResponse, Observation, ResetRequest, ResetResponse,
    StepBatchRequest, StepBatchResponse, StepRequest, StepResponse, TensorSpec,
    StreamRequest, StreamResponse, stream_request, stream_response,
};

// 遊戲核心模組
mod game;
mod service;

// 從 game 模組導入常量和類型
use game::{
    score_hand, BlindType, BossBlind, Card, Consumable, DeckType, Edition, Enhancement, GameEnd,
    JokerId, JokerSlot, PackContents, PackItem, PlanetId, Seal, SpectralId, Stage, Tag, TagId,
    TarotId, joker_def::JokerState, ACTION_MASK_SIZE, ACTION_TYPE_BUY_JOKER, ACTION_TYPE_BUY_PACK,
    ACTION_TYPE_BUY_VOUCHER, ACTION_TYPE_CASH_OUT, ACTION_TYPE_DISCARD, ACTION_TYPE_NEXT_ROUND,
    ACTION_TYPE_PLAY, ACTION_TYPE_REROLL, ACTION_TYPE_SELECT, ACTION_TYPE_SELECT_BLIND,
    ACTION_TYPE_SELL_JOKER, ACTION_TYPE_SKIP_BLIND, ACTION_TYPE_USE_CONSUMABLE, DISCARDS_PER_BLIND,
    HAND_SIZE, MAX_SELECTED, MAX_STEPS, OBS_SIZE, PLAYS_PER_BLIND,
    trigger_joker_slot_events, GameEvent, TriggerContext,
    calculate_shop_quality,
};

// 從 service 模組導入
use service::{
    action_mask_from_state, build_selected_hand, calculate_play_score, observation_from_state,
    EnvState,
};

// ============================================================================
// v6.4: 手牌潛力計算
// ============================================================================

/// 計算手牌的潛力指標（用於棄牌獎勵）
/// 返回 (flush_potential, straight_potential, pairs_potential)
fn calculate_hand_potential(hand: &[Card]) -> (f32, f32, f32) {
    if hand.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    // Flush 潛力：最大同花數量 / 5
    let mut suit_counts = [0u8; 4];
    for card in hand {
        if (card.suit as usize) < 4 {
            suit_counts[card.suit as usize] += 1;
        }
    }
    let max_suit = *suit_counts.iter().max().unwrap_or(&0) as f32;
    let flush_potential = (max_suit / 5.0).min(1.0);

    // Straight 潛力：最長連續 rank / 5
    let mut ranks: Vec<u8> = hand.iter().map(|c| c.rank).collect();
    ranks.sort_unstable();
    ranks.dedup();

    let mut max_consecutive = 1u8;
    let mut current = 1u8;
    for i in 1..ranks.len() {
        if ranks[i] == ranks[i - 1] + 1 {
            current += 1;
            max_consecutive = max_consecutive.max(current);
        } else if ranks[i] > ranks[i - 1] + 1 {
            current = 1;
        }
    }
    // 特殊處理 A-2-3-4-5 順子（Ace 可以當 1）
    if ranks.contains(&1) && ranks.contains(&2) {
        // Ace 已經在 ranks 中作為 1，檢查是否有 wheel
        let has_wheel = [1, 2, 3, 4, 5].iter().all(|&r| ranks.contains(&r));
        if has_wheel {
            max_consecutive = max_consecutive.max(5);
        }
    }
    let straight_potential = (max_consecutive as f32 / 5.0).min(1.0);

    // Pairs 潛力：最大同 rank 數量 / 4
    let mut rank_counts = [0u8; 14]; // ranks 1-13
    for card in hand {
        if (card.rank as usize) < 14 {
            rank_counts[card.rank as usize] += 1;
        }
    }
    let max_rank = *rank_counts.iter().max().unwrap_or(&0) as f32;
    let pairs_potential = (max_rank / 4.0).min(1.0);

    (flush_potential, straight_potential, pairs_potential)
}

// ============================================================================
// Step 執行結果（用於共用 step 邏輯）
// ============================================================================

/// Step 邏輯執行結果，包含所有需要回傳的中間數據
struct StepResult {
    done: bool,
    action_cost: i64,
    blind_cleared: bool,
    cards_played: i32,
    cards_discarded: i32,
    hand_type_id: i32,
    joker_chip_contrib: f32,
    joker_mult_contrib: f32,
    joker_xmult_contrib: f32,
    score_delta: i64,
    money_delta: i64,
}

impl Default for StepResult {
    fn default() -> Self {
        Self {
            done: false,
            action_cost: 0,
            blind_cleared: false,
            cards_played: 0,
            cards_discarded: 0,
            hand_type_id: -1,
            joker_chip_contrib: 0.0,
            joker_mult_contrib: 0.0,
            joker_xmult_contrib: 0.0,
            score_delta: 0,
            money_delta: 0,
        }
    }
}

/// 構建 StepResponse（從 state 和 StepResult）
fn build_step_response(
    state: &EnvState,
    result: &StepResult,
    action_type: i32,
) -> StepResponse {
    let features = observation_from_state(state);
    let action_mask = action_mask_from_state(state, result.done);

    let observation = Observation {
        features: Some(features),
        action_mask: Some(action_mask),
    };

    let (flush_pot, straight_pot, pairs_pot) = calculate_hand_potential(&state.hand);
    let blind_target = state.required_score();
    let expected_per_play = blind_target as f32 / 4.0;
    let score_efficiency = if expected_per_play > 0.0 && result.score_delta > 0 {
        result.score_delta as f32 / expected_per_play
    } else {
        0.0
    };

    let game_end = match state.stage {
        Stage::End(GameEnd::Win) => 1,
        Stage::End(GameEnd::Lose) => 2,
        _ => 0,
    };

    let info = EnvInfo {
        episode_step: state.episode_step,
        chips: state.score,
        mult: 1,
        blind_target,
        ante: state.ante.to_int(),
        stage: match state.stage {
            Stage::PreBlind => 0,
            Stage::Blind => 1,
            Stage::PostBlind => 2,
            Stage::Shop => 3,
            Stage::End(_) => 4,
        },
        blind_type: state.blind_type.map(|b| b.to_int()).unwrap_or(-1),
        plays_left: state.plays_left as i32,
        discards_left: state.discards_left as i32,
        money: state.money as i32,
        score_delta: result.score_delta,
        money_delta: result.money_delta as i32,
        last_action_type: action_type,
        last_action_cost: result.action_cost as i32,
        joker_count: state.jokers.len() as i32,
        joker_slot_limit: state.effective_joker_slot_limit() as i32,
        game_end,
        blind_cleared: result.blind_cleared,
        cards_played: result.cards_played,
        cards_discarded: result.cards_discarded,
        hand_type: result.hand_type_id,
        tag_id: state.last_tag_id,
        consumable_id: state.last_consumable_id,
        joker_sold_id: state.last_sold_joker_id,
        best_shop_joker_cost: state.shop.items.iter().map(|item| item.cost as i32).max().unwrap_or(0),
        flush_potential: flush_pot,
        straight_potential: straight_pot,
        pairs_potential: pairs_pot,
        joker_chip_contrib: result.joker_chip_contrib,
        joker_mult_contrib: result.joker_mult_contrib,
        joker_xmult_contrib: result.joker_xmult_contrib,
        score_efficiency,
        boss_blind_id: state.boss_blind.map(|b| b.to_int()).unwrap_or(-1),
        shop_quality_score: calculate_shop_quality(&state.shop, &state.jokers),
        reroll_count_this_shop: state.shop.reroll_count,
    };

    StepResponse {
        observation: Some(observation),
        reward: 0.0, // 獎勵由 Python 端計算
        done: result.done,
        info: Some(info),
    }
}

// ============================================================================
// 共享 Step 邏輯（用於 unary RPC 和 streaming RPC）
// ============================================================================

/// 執行完整的 Step 邏輯，返回 StepResult
/// 這個函數包含所有遊戲邏輯，確保 unary 和 streaming 行為一致
fn execute_step_logic(
    state: &mut EnvState,
    action_type: i32,
    action_id: u32,
    _params: &[i32],
) -> StepResult {
    let mut result = StepResult::default();

    // 記錄動作前狀態（用於計算 delta）
    let score_before = state.score;
    let money_before = state.money;

    match state.stage {
        Stage::PreBlind => {
            match action_type {
                ACTION_TYPE_SELECT_BLIND => {
                    let next_blind = state
                        .blind_type
                        .and_then(|b| b.next())
                        .unwrap_or(BlindType::Small);
                    state.blind_type = Some(next_blind);
                    state.stage = Stage::Blind;

                    if next_blind == BlindType::Boss {
                        state.select_random_boss();
                        state.plays_left = state
                            .boss_blind
                            .and_then(|b| b.max_plays())
                            .unwrap_or(PLAYS_PER_BLIND);
                    } else {
                        state.boss_blind = None;
                        state.plays_left = PLAYS_PER_BLIND;
                    }

                    state.discards_left = DISCARDS_PER_BLIND;

                    // Drunkard: +1 棄牌次數每輪
                    let drunkard_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Drunkard)
                        .count() as i32;
                    state.discards_left += drunkard_count;

                    // Troubadour: -1 出牌次數每輪
                    let troubadour_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Troubadour)
                        .count() as i32;
                    state.plays_left = (state.plays_left - troubadour_count).max(1);

                    // Burglar: +3 出牌次數，無法棄牌
                    let burglar_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Burglar)
                        .count() as i32;
                    if burglar_count > 0 {
                        state.plays_left += 3 * burglar_count;
                        state.discards_left = 0;
                    }

                    state.score = 0;
                    state.played_hand_types.clear();
                    state.first_hand_type = None;
                    state.discards_used_this_blind = 0;
                    state.hands_played_this_blind = 0;
                    state.pillar_played_cards.clear();

                    // Hit The Road: 每 Blind 開始時重置 X Mult
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::Hit_The_Road {
                            if let JokerState::Accumulator { x_mult, .. } = &mut joker.state {
                                *x_mult = 1.0;
                            }
                        }
                    }

                    // TurtleBean: 每輪 -1 手牌大小，到 0 時自毀
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::TurtleBean {
                            if joker.update_turtle_bean_on_round() {
                                joker.enabled = false;
                            }
                        }
                    }

                    // MarbleJoker: 選擇 Blind 時加 Stone 卡到牌組
                    let marble_joker_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::MarbleJoker)
                        .count();
                    for _ in 0..marble_joker_count {
                        let stone_card = Card {
                            rank: 1,
                            suit: 0,
                            enhancement: Enhancement::Stone,
                            seal: Seal::None,
                            edition: Edition::Base,
                            face_down: false,
                            bonus_chips: 0,
                        };
                        state.deck.push(stone_card);
                    }

                    // Hologram: 每加牌到牌組 +0.25 X Mult
                    if marble_joker_count > 0 {
                        state.update_jokers(JokerId::Hologram, |j| {
                            j.update_hologram_on_card_added(marble_joker_count as i32);
                        });
                    }

                    // RiffRaff: 選擇 Blind 時生成 2 個 Common Joker
                    let riff_raff_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::RiffRaff)
                        .count();
                    let common_jokers = JokerId::by_rarity(1);
                    let effective_slots = state.effective_joker_slot_limit();
                    for _ in 0..riff_raff_count {
                        for _ in 0..2 {
                            if state.jokers.len() < effective_slots && !common_jokers.is_empty() {
                                let idx = state.rng.gen_range(0..common_jokers.len());
                                state.jokers.push(JokerSlot::new(common_jokers[idx]));
                            }
                        }
                    }

                    // AncientJoker: 每回合開始時隨機設置花色
                    let ancient_joker_random_suit: u8 = state.rng.gen_range(0..4);
                    state.update_jokers(JokerId::AncientJoker, |j| {
                        j.set_ancient_suit(ancient_joker_random_suit);
                    });

                    // Castle: 每回合開始時隨機設置花色
                    let castle_random_suit: u8 = state.rng.gen_range(0..4);
                    state.update_jokers(JokerId::Castle, |j| {
                        j.set_castle_suit(castle_random_suit);
                    });

                    // TheIdol: 每回合開始時隨機設置目標牌
                    let idol_random_rank: u8 = state.rng.gen_range(1..=13);
                    let idol_random_suit: u8 = state.rng.gen_range(0..4);
                    state.update_jokers(JokerId::TheIdol, |j| {
                        j.set_idol_target(idol_random_rank, idol_random_suit);
                    });

                    // ChaosTheClown: 每回合重置免費 reroll
                    state.update_jokers(JokerId::ChaosTheClown, |j| {
                        j.reset_chaos_free_reroll();
                    });

                    // GreenJoker: 每輪重置 Mult 計數器
                    state.update_jokers(JokerId::GreenJoker, |j| {
                        j.reset_green_joker();
                    });

                    // Wee: 每輪 +8 Chips
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::Wee {
                            joker.update_wee_on_round();
                        }
                    }

                    // Merry: 每輪 +3 Mult
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::Merry {
                            joker.update_merry_on_round();
                        }
                    }

                    // Popcorn: 每輪 -4 Mult，到 0 時自毀
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::Popcorn {
                            if joker.update_popcorn_on_round() {
                                joker.enabled = false;
                            }
                        }
                    }

                    // SteakJoker: 每輪售價 -$1，到 0 時自毀
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::SteakJoker {
                            joker.sell_value -= 1;
                            if joker.sell_value <= 0 {
                                joker.enabled = false;
                            }
                        }
                    }

                    // Egg: 每輪 +$3 售價
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::Egg {
                            joker.sell_value += 3;
                        }
                    }

                    // Ceremonial: 選擇 Blind 時銷毀最右邊的其他 Joker
                    let ceremonial_indices: Vec<usize> = state
                        .jokers
                        .iter()
                        .enumerate()
                        .filter(|(_, j)| j.enabled && j.id == JokerId::Ceremonial)
                        .map(|(i, _)| i)
                        .collect();
                    let mut jokers_destroyed_by_ceremonial = 0;
                    for ceremonial_idx in ceremonial_indices {
                        let rightmost_target =
                            state.jokers.iter().enumerate().rev().find(|(i, j)| {
                                *i != ceremonial_idx && j.enabled && j.id != JokerId::Ceremonial
                            });
                        if let Some((target_idx, target_joker)) = rightmost_target {
                            let sell_value = target_joker.sell_value;
                            state.jokers[ceremonial_idx].counter += (sell_value * 2) as i32;
                            state.jokers[target_idx].enabled = false;
                            jokers_destroyed_by_ceremonial += 1;
                        }
                    }

                    // 使用觸發系統處理 Madness 和 Chicot
                    let is_small_or_big_blind =
                        next_blind == BlindType::Small || next_blind == BlindType::Big;
                    let is_boss_blind = next_blind == BlindType::Boss;
                    let trigger_ctx = TriggerContext {
                        rng_value: state.rng.gen(),
                        is_boss_blind,
                        is_small_or_big_blind,
                        ..Default::default()
                    };
                    let trigger_result = trigger_joker_slot_events(
                        GameEvent::BlindSelected,
                        &mut state.jokers,
                        &trigger_ctx,
                    );

                    // Chicot: 禁用 Boss Blind
                    if trigger_result.disable_boss_blind {
                        state.boss_blind = None;
                    }

                    // Madness: 銷毀隨機非 Madness Joker
                    let mut jokers_destroyed_by_madness = 0;
                    for _ in 0..trigger_result.madness_destroys {
                        let targets: Vec<usize> = state
                            .jokers
                            .iter()
                            .enumerate()
                            .filter(|(_, j)| j.enabled && j.id != JokerId::Madness)
                            .map(|(i, _)| i)
                            .collect();
                        if !targets.is_empty() {
                            let target_idx = targets[state.rng.gen_range(0..targets.len())];
                            state.jokers[target_idx].enabled = false;
                            jokers_destroyed_by_madness += 1;
                        }
                    }

                    // Madness: 每銷毀 Joker +0.5 X Mult
                    let total_jokers_destroyed =
                        jokers_destroyed_by_ceremonial + jokers_destroyed_by_madness;
                    if total_jokers_destroyed > 0 {
                        state.update_jokers(JokerId::Madness, |j| {
                            j.update_madness_on_joker_destroyed(total_jokers_destroyed);
                        });
                    }

                    state.deal();

                    // Certificate: 回合開始時獲得一張帶有隨機封印的隨機牌
                    let certificate_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Certificate)
                        .count();
                    for _ in 0..certificate_count {
                        let rank = state.rng.gen_range(1..=13) as u8;
                        let suit = state.rng.gen_range(0..4) as u8;
                        let seal = match state.rng.gen_range(0..4) {
                            0 => Seal::Gold,
                            1 => Seal::Red,
                            2 => Seal::Blue,
                            _ => Seal::Purple,
                        };
                        let mut card = Card::new(rank, suit);
                        card.seal = seal;
                        state.hand.push(card);
                    }

                    // Boss Blind 效果應用
                    if state.boss_blind == Some(BossBlind::TheHook) {
                        state.apply_hook_discard();
                    }

                    // TheWheel: 1/7 的牌面朝下
                    if state.boss_blind == Some(BossBlind::TheWheel) {
                        let hand_len = state.hand.len();
                        let face_down_flags: Vec<bool> = (0..hand_len)
                            .map(|_| state.rng.gen_range(0..7) == 0)
                            .collect();
                        for (card, face_down) in state.hand.iter_mut().zip(face_down_flags.iter()) {
                            if *face_down {
                                card.face_down = true;
                            }
                        }
                    }

                    // TheHouse: 第一手全部面朝下
                    if state.boss_blind == Some(BossBlind::TheHouse) {
                        for card in &mut state.hand {
                            card.face_down = true;
                        }
                    }

                    // Verdant: 所有牌在回合開始時面朝下
                    if state.boss_blind == Some(BossBlind::Verdant) {
                        for card in &mut state.hand {
                            card.face_down = true;
                        }
                    }

                    // Cerulean: 強制使用第一張消耗品
                    if state.boss_blind == Some(BossBlind::Cerulean) {
                        if let Some(consumable) = state.consumables.use_item(0) {
                            if let Consumable::Planet(planet_id) = &consumable {
                                let hand_type_idx = planet_id.hand_type_index();
                                state.hand_levels.upgrade(hand_type_idx);
                                state.planets_used_this_run += 1;
                            }
                            state.last_used_consumable = Some(consumable);
                        }
                    }

                    // TheMark: 所有 Face Card 面朝下
                    if state.boss_blind == Some(BossBlind::TheMark) {
                        for card in &mut state.hand {
                            if card.is_face() {
                                card.face_down = true;
                            }
                        }
                    }

                    // TheFish: 面朝下的牌打亂順序
                    if state.boss_blind == Some(BossBlind::TheFish) {
                        let face_down_indices: Vec<usize> = state
                            .hand
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| c.face_down)
                            .map(|(i, _)| i)
                            .collect();

                        if face_down_indices.len() > 1 {
                            let mut shuffled = face_down_indices.clone();
                            let shuffle_indices: Vec<usize> = (1..shuffled.len())
                                .rev()
                                .map(|i| state.rng.gen_range(0..=i))
                                .collect();
                            for (idx, j) in (1..shuffled.len()).rev().zip(shuffle_indices.iter()) {
                                shuffled.swap(idx, *j);
                            }
                            for (old_idx, new_idx) in
                                face_down_indices.iter().zip(shuffled.iter())
                            {
                                if old_idx != new_idx {
                                    state.hand.swap(*old_idx, *new_idx);
                                }
                            }
                        }
                    }
                }

                ACTION_TYPE_SKIP_BLIND => {
                    state.skip_blind();
                    state.blinds_skipped += 1;

                    let trigger_ctx = TriggerContext {
                        rng_value: state.rng.gen(),
                        ..Default::default()
                    };
                    let trigger_result = trigger_joker_slot_events(
                        GameEvent::BlindSkipped,
                        &mut state.jokers,
                        &trigger_ctx,
                    );

                    // RedCard: 增加 Mult
                    if trigger_result.red_card_mult_increase > 0 {
                        for joker in &mut state.jokers {
                            if joker.enabled && joker.id == JokerId::RedCard {
                                joker.red_card_mult += trigger_result.red_card_mult_increase;
                            }
                        }
                    }

                    // Cartomancer: 生成隨機 Tarot 卡
                    for _ in 0..trigger_result.tarot_to_create {
                        if !state.consumables.is_full() {
                            let all_tarots = TarotId::all();
                            let idx = state.rng.gen_range(0..all_tarots.len());
                            state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                        }
                    }

                    // Astronomer: 生成隨機 Planet 卡
                    for _ in 0..trigger_result.planet_to_create {
                        if !state.consumables.is_full() {
                            let all_planets = PlanetId::all();
                            let idx = state.rng.gen_range(0..all_planets.len());
                            state.consumables.add(Consumable::Planet(all_planets[idx]));
                        }
                    }
                }

                _ => {}
            }
        }

        Stage::Blind => {
            execute_blind_stage_logic(state, action_type, action_id, &mut result);
        }

        Stage::PostBlind => {
            execute_post_blind_logic(state, action_type, &mut result);
        }

        Stage::Shop => {
            execute_shop_logic(state, action_type, action_id, &mut result);
        }

        Stage::End(_) => {
            result.done = true;
        }
    }

    state.episode_step += 1;
    if state.episode_step >= MAX_STEPS {
        state.stage = Stage::End(GameEnd::Lose);
        result.done = true;
    }

    // 計算 delta
    result.score_delta = state.score - score_before;
    result.money_delta = state.money - money_before;

    result
}

/// 執行 Blind 階段邏輯
fn execute_blind_stage_logic(
    state: &mut EnvState,
    action_type: i32,
    action_id: u32,
    result: &mut StepResult,
) {
    match action_type {
        ACTION_TYPE_SELECT => {
            let mask = action_id & ((1 << HAND_SIZE) - 1);
            let count = mask.count_ones() as usize;
            if count <= MAX_SELECTED {
                state.selected_mask = mask;
            }
        }

        ACTION_TYPE_PLAY => {
            if state.plays_left > 0 && state.selected_mask > 0 {
                let mut selected = build_selected_hand(&state.hand, state.selected_mask);
                let selected_count = selected.len();
                result.cards_played = selected_count as i32;

                // ThePillar: 已打過的牌不再計分
                if state.boss_blind == Some(BossBlind::ThePillar) {
                    for card in &mut selected {
                        let key = (card.rank as u8, card.suit as u8);
                        if state.pillar_played_cards.contains(&key) {
                            card.face_down = true;
                        }
                    }
                }

                let psychic_ok = !state
                    .boss_blind
                    .map(|b| b.requires_five_cards() && selected_count != 5)
                    .unwrap_or(false);

                if psychic_ok {
                    let jokers_clone = state.jokers.clone();
                    let boss_blind = state.boss_blind;
                    let discards_remaining = state.discards_left;
                    let enhanced_cards_in_deck = state
                        .deck
                        .iter()
                        .filter(|c| c.enhancement != Enhancement::None)
                        .count() as i32;
                    let is_first_hand = state.hands_played_this_blind == 0;
                    let is_final_hand = state.plays_left == 1;
                    let selzer_charges = state
                        .jokers
                        .iter()
                        .find(|j| j.enabled && j.id == JokerId::Selzer)
                        .map(|j| j.get_selzer_charges())
                        .unwrap_or(0);
                    let hand_levels_clone = state.hand_levels.clone();
                    let uses_plasma_scoring = state.deck_type.uses_plasma_scoring();
                    let observatory_x_mult = state.voucher_effects.observatory_x_mult;
                    let planet_used_hand_types = state.planet_used_hand_types;

                    let score_result = calculate_play_score(
                        &selected,
                        &jokers_clone,
                        boss_blind,
                        discards_remaining,
                        state.rerolls_this_run,
                        state.blinds_skipped,
                        state.joker_slot_limit,
                        enhanced_cards_in_deck,
                        is_first_hand,
                        is_final_hand,
                        selzer_charges,
                        &hand_levels_clone,
                        uses_plasma_scoring,
                        observatory_x_mult,
                        planet_used_hand_types,
                        &mut state.rng,
                    );

                    let score_gained = score_result.score;
                    let hand_id = score_result.hand_id;
                    let hand_type_idx = hand_id.to_index();
                    result.hand_type_id = hand_type_idx as i32;

                    // Joker 貢獻
                    result.joker_chip_contrib = score_result.joker_chip_contrib;
                    result.joker_mult_contrib = score_result.joker_mult_contrib;
                    result.joker_xmult_contrib = score_result.joker_xmult_contrib;

                    let eye_ok = !state
                        .boss_blind
                        .map(|b| {
                            matches!(b, BossBlind::TheEye)
                                && state.played_hand_types.contains(&hand_type_idx)
                        })
                        .unwrap_or(false);

                    let mouth_ok = !state
                        .boss_blind
                        .map(|b| {
                            matches!(b, BossBlind::TheMouth)
                                && state.first_hand_type.is_some()
                                && state.first_hand_type != Some(hand_type_idx)
                        })
                        .unwrap_or(false);

                    let _violated_boss_rule = !eye_ok || !mouth_ok;

                    state.played_hand_types.push(hand_type_idx);
                    if state.first_hand_type.is_none() {
                        state.first_hand_type = Some(hand_type_idx);
                    }

                    // Obelisk: 更新牌型計數
                    state.hand_type_counts[hand_type_idx] += 1;
                    let max_count = *state.hand_type_counts.iter().max().unwrap_or(&0);
                    let most_played_idx = state
                        .hand_type_counts
                        .iter()
                        .position(|&c| c == max_count)
                        .unwrap_or(0);
                    let is_most_played = hand_type_idx == most_played_idx;

                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::Obelisk {
                            if is_most_played {
                                joker.reset_obelisk_streak();
                            } else {
                                joker.increment_obelisk_streak();
                            }
                        }
                    }

                    state.score += score_gained;
                    state.plays_left -= 1;
                    state.hands_played_this_blind += 1;
                    state.money += score_result.money_gained;

                    // Selzer: 更新 charges
                    if score_result.selzer_charges_used > 0 {
                        for joker in &mut state.jokers {
                            if joker.enabled && joker.id == JokerId::Selzer {
                                if joker.use_selzer_charges(score_result.selzer_charges_used) {
                                    joker.enabled = false;
                                }
                                break;
                            }
                        }
                    }

                    // Lucky_Cat
                    if score_result.lucky_triggers > 0 {
                        state.update_jokers(JokerId::Lucky_Cat, |j| {
                            j.update_lucky_cat_on_trigger(score_result.lucky_triggers);
                        });
                    }

                    // SpaceJoker
                    let space_joker_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::SpaceJoker)
                        .count();
                    for _ in 0..space_joker_count {
                        if state.rng.gen_range(0..4) == 0 {
                            state.hand_levels.upgrade(hand_type_idx);
                        }
                    }

                    // TheArm: 降低牌型等級
                    if state.boss_blind == Some(BossBlind::TheArm) {
                        state.hand_levels.downgrade(hand_type_idx);
                    }

                    // TheOx: 出特定牌型失去 $1
                    if state.boss_blind == Some(BossBlind::TheOx) {
                        let ante_hand_idx = (state.ante.to_int() - 1) as usize;
                        if hand_type_idx == ante_hand_idx {
                            state.money = (state.money - 1).max(0);
                        }
                    }

                    // ThePillar: 記錄打出的牌
                    if state.boss_blind == Some(BossBlind::ThePillar) {
                        for card in &selected {
                            if !card.face_down {
                                let key = (card.rank as u8, card.suit as u8);
                                state.pillar_played_cards.insert(key);
                            }
                        }
                    }

                    // Vagabond
                    if selected_count <= 4 {
                        let vagabond_count = state
                            .jokers
                            .iter()
                            .filter(|j| j.enabled && j.id == JokerId::Vagabond)
                            .count();
                        for _ in 0..vagabond_count {
                            if !state.consumables.is_full() {
                                let all_tarots = TarotId::all();
                                let idx = state.rng.gen_range(0..all_tarots.len());
                                state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                            }
                        }
                    }

                    // EightBall
                    let has_eight = selected.iter().any(|c| c.rank == 8);
                    if has_eight {
                        let eight_ball_count = state
                            .jokers
                            .iter()
                            .filter(|j| j.enabled && j.id == JokerId::EightBall)
                            .count();
                        for _ in 0..eight_ball_count {
                            if !state.consumables.is_full() {
                                let all_tarots = TarotId::all();
                                let idx = state.rng.gen_range(0..all_tarots.len());
                                state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                            }
                        }
                    }

                    // Hallucination
                    let hallucination_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Hallucination)
                        .count();
                    for _ in 0..hallucination_count {
                        if state.rng.gen_range(0..2) == 0 && !state.consumables.is_full() {
                            let all_tarots = TarotId::all();
                            let idx = state.rng.gen_range(0..all_tarots.len());
                            state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                        }
                    }

                    // MidasMask
                    let has_midas = state
                        .jokers
                        .iter()
                        .any(|j| j.enabled && j.id == JokerId::MidasMask);
                    if has_midas {
                        let selected_mask = state.selected_mask;
                        for (idx, card) in state.hand.iter_mut().enumerate() {
                            if ((selected_mask >> idx) & 1) == 1 && card.is_face() {
                                card.enhancement = Enhancement::Gold;
                            }
                        }
                    }

                    // Vampire
                    let vampire_idx = state
                        .jokers
                        .iter()
                        .position(|j| j.enabled && j.id == JokerId::Vampire);
                    if let Some(v_idx) = vampire_idx {
                        let selected_mask = state.selected_mask;
                        let mut enhancements_absorbed = 0;
                        for (idx, card) in state.hand.iter_mut().enumerate() {
                            if ((selected_mask >> idx) & 1) == 1
                                && card.enhancement != Enhancement::None
                            {
                                enhancements_absorbed += 1;
                                card.enhancement = Enhancement::None;
                            }
                        }
                        if enhancements_absorbed > 0 {
                            state.jokers[v_idx].update_vampire_on_enhancement(enhancements_absorbed);
                        }
                    }

                    // Hiker
                    let hiker_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Hiker)
                        .count() as i64;
                    if hiker_count > 0 {
                        let selected_mask = state.selected_mask;
                        for (idx, card) in state.hand.iter_mut().enumerate() {
                            if ((selected_mask >> idx) & 1) == 1 {
                                card.bonus_chips += 2 * hiker_count;
                            }
                        }
                    }

                    // ToDoList
                    let todo_matches: Vec<usize> = state
                        .jokers
                        .iter()
                        .enumerate()
                        .filter(|(_, j)| {
                            j.enabled
                                && j.id == JokerId::ToDoList
                                && hand_type_idx == j.get_todo_hand_type() as usize
                        })
                        .map(|(i, _)| i)
                        .collect();
                    state.money += todo_matches.len() as i64 * 4;
                    for idx in todo_matches {
                        let new_type = state.rng.gen_range(0..13) as u8;
                        state.jokers[idx].set_todo_hand_type(new_type);
                    }

                    // Seance
                    if hand_type_idx == 8 || hand_type_idx == 9 {
                        let seance_count = state
                            .jokers
                            .iter()
                            .filter(|j| j.enabled && j.id == JokerId::Seance)
                            .count();
                        for _ in 0..seance_count {
                            if !state.consumables.is_full() {
                                let all_spectrals = SpectralId::all();
                                let idx = state.rng.gen_range(0..all_spectrals.len());
                                state.consumables.add(Consumable::Spectral(all_spectrals[idx]));
                            }
                        }
                    }

                    // HandPlayed 觸發
                    let has_face = selected.iter().any(|c| c.is_face());
                    let has_rank_2 = selected.iter().any(|c| c.rank == 2);
                    let has_rank_13 = selected.iter().any(|c| c.rank == 13);
                    let trigger_ctx = TriggerContext {
                        rng_value: state.rng.gen(),
                        has_face_card: has_face,
                        has_rank_2,
                        has_rank_13,
                        played_hand_type: hand_type_idx,
                        is_most_played_hand: is_most_played,
                        ..Default::default()
                    };
                    let _trigger_result = trigger_joker_slot_events(
                        GameEvent::HandPlayed,
                        &mut state.jokers,
                        &trigger_ctx,
                    );

                    // IceCream 自毀
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::IceCream {
                            if let JokerState::Accumulator { chips, .. } = &joker.state {
                                if *chips <= 0 {
                                    joker.enabled = false;
                                }
                            }
                        }
                    }

                    // LoyaltyCard 重置
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::LoyaltyCard {
                            if let JokerState::Counter { current, .. } = &mut joker.state {
                                if *current >= 6 {
                                    *current = 0;
                                }
                            }
                        }
                    }

                    // Cavendish
                    let cavendish_rng: u32 = state.rng.gen_range(0..1000);
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::Cavendish {
                            if cavendish_rng == 0 {
                                joker.enabled = false;
                            }
                        }
                    }

                    let selected_mask = state.selected_mask;

                    // GlassJoker
                    let glass_broken_count = score_result.glass_to_break.len() as i32;
                    let mut face_cards_destroyed = 0;
                    if glass_broken_count > 0 {
                        let mut selected_idx = 0;
                        for (idx, card) in state.hand.iter().enumerate() {
                            if ((selected_mask >> idx) & 1) == 1 {
                                if score_result.glass_to_break.contains(&selected_idx) {
                                    if card.is_face() {
                                        face_cards_destroyed += 1;
                                    }
                                }
                                selected_idx += 1;
                            }
                        }
                    }

                    state.break_glass_cards(selected_mask, &score_result.glass_to_break);

                    if glass_broken_count > 0 {
                        for joker in &mut state.jokers {
                            if joker.enabled && joker.id == JokerId::GlassJoker {
                                joker.update_glass_on_break(glass_broken_count);
                            }
                        }
                    }

                    // Canio
                    if face_cards_destroyed > 0 {
                        for joker in &mut state.jokers {
                            if joker.enabled && joker.id == JokerId::Canio {
                                joker.update_canio_on_face_destroyed(face_cards_destroyed);
                            }
                        }
                    }

                    let required = state.required_score();

                    if state.score >= required {
                        result.blind_cleared = true;
                        state.reward = state.calc_reward();
                        state.stage = Stage::PostBlind;
                    } else if state.plays_left == 0 {
                        state.stage = Stage::End(GameEnd::Lose);
                        result.done = true;
                    } else {
                        state.deal();

                        if state.boss_blind == Some(BossBlind::TheSerpent) {
                            state.apply_serpent_effect();
                        }

                        if state.boss_blind == Some(BossBlind::TheHook) {
                            state.apply_hook_discard();
                        }
                    }

                    state.selected_mask = 0;
                }
            }
        }

        ACTION_TYPE_DISCARD => {
            if state.discards_left > 0 && state.selected_mask > 0 {
                let mask = state.selected_mask;

                let selected_cards: Vec<Card> = state
                    .hand
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| (mask >> i) & 1 == 1)
                    .map(|(_, c)| *c)
                    .collect();

                result.cards_discarded = mask.count_ones() as i32;
                let _purple_count = state.discard_with_seals(mask);
                state.discards_left -= 1;
                state.discards_used_this_blind += 1;
                state.selected_mask = 0;

                let face_count = selected_cards.iter().filter(|c| c.is_face()).count() as i32;
                let jack_count = selected_cards.iter().filter(|c| c.rank == 11).count() as i32;
                let king_count = selected_cards.iter().filter(|c| c.rank == 13).count() as i32;
                let mut suit_count = [0i32; 4];
                for card in &selected_cards {
                    if (card.suit as usize) < 4 {
                        suit_count[card.suit as usize] += 1;
                    }
                }

                let discarded_hand_type = if !selected_cards.is_empty() {
                    score_hand(&selected_cards).id.to_index()
                } else {
                    0
                };

                let trigger_ctx = TriggerContext {
                    discarded_face_count: face_count,
                    discarded_jack_count: jack_count,
                    discarded_king_count: king_count,
                    discarded_suit_count: suit_count,
                    discarded_count: result.cards_discarded,
                    discarded_hand_type,
                    ..Default::default()
                };

                let trigger_result = trigger_joker_slot_events(
                    GameEvent::CardDiscarded,
                    &mut state.jokers,
                    &trigger_ctx,
                );

                state.money += trigger_result.money_delta;

                for _ in 0..trigger_result.tarot_to_create {
                    if !state.consumables.is_full() {
                        let all_tarots = TarotId::all();
                        let idx = state.rng.gen_range(0..all_tarots.len());
                        state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                    }
                }

                for _ in 0..trigger_result.spectral_to_create {
                    if !state.consumables.is_full() {
                        let all_spectrals = SpectralId::all();
                        let idx = state.rng.gen_range(0..all_spectrals.len());
                        state.consumables.add(Consumable::Spectral(all_spectrals[idx]));
                    }
                }

                for hand_idx in &trigger_result.hand_levels_to_upgrade {
                    state.hand_levels.upgrade(*hand_idx);
                }

                for &idx in &trigger_result.jokers_to_destroy {
                    if idx < state.jokers.len() {
                        state.jokers[idx].enabled = false;
                    }
                }
            }
        }

        ACTION_TYPE_USE_CONSUMABLE => {
            execute_use_consumable_in_blind(state, action_id);
        }

        _ => {}
    }
}

/// 執行消耗品使用邏輯（Blind 階段）
fn execute_use_consumable_in_blind(state: &mut EnvState, action_id: u32) {
    let index = action_id as usize;
    if let Some(consumable) = state.consumables.use_item(index) {
        state.last_consumable_id = consumable.to_global_index() as i32;

        match &consumable {
            Consumable::Planet(planet_id) => {
                let hand_type_idx = planet_id.hand_type_index();
                state.hand_levels.upgrade(hand_type_idx);
                state.planet_used_hand_types |= 1 << hand_type_idx;

                let trigger_ctx = TriggerContext::default();
                let _trigger_result = trigger_joker_slot_events(
                    GameEvent::PlanetUsed,
                    &mut state.jokers,
                    &trigger_ctx,
                );
                state.planets_used_this_run += 1;
                state.last_used_consumable = Some(consumable.clone());
            }
            Consumable::Tarot(tarot_id) => {
                state.tarots_used_this_run += 1;
                execute_tarot_effect(state, *tarot_id, &consumable);
            }
            Consumable::Spectral(spectral_id) => {
                execute_spectral_effect(state, *spectral_id, &consumable);
            }
        }
    }
}

/// 執行 Tarot 效果
fn execute_tarot_effect(state: &mut EnvState, tarot_id: TarotId, consumable: &Consumable) {
    let selected_indices: Vec<usize> = (0..state.hand.len())
        .filter(|&i| ((state.selected_mask >> i) & 1) == 1)
        .collect();

    match tarot_id {
        TarotId::TheMagician => {
            for &idx in selected_indices.iter().take(2) {
                state.hand[idx].enhancement = Enhancement::Lucky;
            }
        }
        TarotId::TheEmpress => {
            for &idx in selected_indices.iter().take(2) {
                state.hand[idx].enhancement = Enhancement::Mult;
            }
        }
        TarotId::TheHierophant => {
            for &idx in selected_indices.iter().take(2) {
                state.hand[idx].enhancement = Enhancement::Bonus;
            }
        }
        TarotId::TheLovers => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].enhancement = Enhancement::Wild;
            }
        }
        TarotId::TheChariot => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].enhancement = Enhancement::Steel;
            }
        }
        TarotId::Justice => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].enhancement = Enhancement::Glass;
            }
        }
        TarotId::TheDevil => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].enhancement = Enhancement::Gold;
            }
        }
        TarotId::TheTower => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].enhancement = Enhancement::Stone;
            }
        }
        TarotId::TheHermit => {
            let doubled = (state.money * 2).min(state.money + 20);
            state.money = doubled;
        }
        TarotId::Strength => {
            for &idx in selected_indices.iter().take(2) {
                if state.hand[idx].rank < 13 {
                    state.hand[idx].rank += 1;
                }
            }
        }
        TarotId::TheHighPriestess => {
            let all_planets = PlanetId::all();
            for _ in 0..2 {
                if !state.consumables.is_full() {
                    let idx = state.rng.gen_range(0..all_planets.len());
                    state.consumables.add(Consumable::Planet(all_planets[idx]));
                }
            }
        }
        TarotId::TheEmperor => {
            let all_tarots = TarotId::all();
            for _ in 0..2 {
                if !state.consumables.is_full() {
                    let idx = state.rng.gen_range(0..all_tarots.len());
                    state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                }
            }
        }
        TarotId::Temperance => {
            let total_sell_value: i64 = state
                .jokers
                .iter()
                .filter(|j| j.enabled)
                .map(|j| j.sell_value)
                .sum();
            state.money += total_sell_value.min(50);
        }
        TarotId::TheStar => {
            for &idx in selected_indices.iter().take(3) {
                state.hand[idx].suit = 1;
            }
        }
        TarotId::TheMoon => {
            for &idx in selected_indices.iter().take(3) {
                state.hand[idx].suit = 0;
            }
        }
        TarotId::TheSun => {
            for &idx in selected_indices.iter().take(3) {
                state.hand[idx].suit = 2;
            }
        }
        TarotId::TheWorld => {
            for &idx in selected_indices.iter().take(3) {
                state.hand[idx].suit = 3;
            }
        }
        TarotId::TheHangedMan => {
            let mut to_remove: Vec<usize> = selected_indices.iter().take(2).copied().collect();
            to_remove.sort_by(|a, b| b.cmp(a));
            for idx in to_remove {
                if idx < state.hand.len() {
                    state.hand.remove(idx);
                }
            }
        }
        TarotId::Death => {
            if selected_indices.len() >= 2 {
                let left = selected_indices[0];
                let right = selected_indices[1];
                state.hand[left] = state.hand[right];
            }
        }
        TarotId::Judgement => {
            if state.jokers.len() < state.effective_joker_slot_limit() {
                let joker_id = JokerId::random_common(&mut state.rng);
                state.jokers.push(JokerSlot::new(joker_id));
            }
        }
        TarotId::TheWheelOfFortune => {
            let joker_count = state.jokers.len();
            if joker_count > 0 && state.rng.gen_range(0..4) == 0 {
                let idx = state.rng.gen_range(0..joker_count);
                let edition = match state.rng.gen_range(0..3) {
                    0 => Edition::Foil,
                    1 => Edition::Holographic,
                    _ => Edition::Polychrome,
                };
                state.jokers[idx].edition = edition;
            }
        }
        TarotId::TheFool => {
            if let Some(last) = state.last_used_consumable.clone() {
                if !state.consumables.is_full() {
                    state.consumables.add(last);
                }
            }
        }
    }

    if tarot_id != TarotId::TheFool {
        state.last_used_consumable = Some(consumable.clone());
    }
}

/// 執行 Spectral 效果
fn execute_spectral_effect(state: &mut EnvState, spectral_id: SpectralId, consumable: &Consumable) {
    let selected_indices: Vec<usize> = (0..state.hand.len())
        .filter(|&i| ((state.selected_mask >> i) & 1) == 1)
        .collect();

    match spectral_id {
        SpectralId::BlackHole => {
            state.hand_levels.upgrade_all();
        }
        SpectralId::Familiar => {
            if let Some(&idx) = selected_indices.first() {
                if idx < state.hand.len() {
                    state.hand.remove(idx);
                }
                for _ in 0..3 {
                    let rank = state.rng.gen_range(11..=13);
                    let suit = state.rng.gen_range(0..4);
                    state.deck.push(Card::new(rank, suit));
                }
            }
        }
        SpectralId::Grim => {
            if let Some(&idx) = selected_indices.first() {
                if idx < state.hand.len() {
                    state.hand.remove(idx);
                }
                for _ in 0..2 {
                    let suit = state.rng.gen_range(0..4);
                    state.deck.push(Card::new(1, suit));
                }
            }
        }
        SpectralId::Incantation => {
            if let Some(&idx) = selected_indices.first() {
                if idx < state.hand.len() {
                    state.hand.remove(idx);
                }
                for _ in 0..4 {
                    let rank = state.rng.gen_range(2..=10);
                    let suit = state.rng.gen_range(0..4);
                    state.deck.push(Card::new(rank, suit));
                }
            }
        }
        SpectralId::Talisman => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].seal = Seal::Gold;
            }
        }
        SpectralId::Aura => {
            if let Some(&idx) = selected_indices.first() {
                let editions = Edition::all_common();
                let edition = editions[state.rng.gen_range(0..editions.len())];
                state.hand[idx].edition = edition;
            }
        }
        SpectralId::Wraith => {
            if state.jokers.len() < state.effective_joker_slot_limit() {
                let joker_id = JokerId::random_rare(&mut state.rng);
                state.jokers.push(JokerSlot::new(joker_id));
            }
            state.money = 0;
        }
        SpectralId::Sigil => {
            let suit = state.rng.gen_range(0..4);
            for card in &mut state.hand {
                card.suit = suit;
            }
        }
        SpectralId::Ouija => {
            let rank = state.rng.gen_range(1..=13);
            for card in &mut state.hand {
                card.rank = rank;
            }
            state.hand_size_modifier -= 1;
        }
        SpectralId::Ectoplasm => {
            let non_negative_jokers: Vec<usize> = state
                .jokers
                .iter()
                .enumerate()
                .filter(|(_, j)| j.enabled && !j.is_negative)
                .map(|(i, _)| i)
                .collect();
            if !non_negative_jokers.is_empty() {
                let idx = non_negative_jokers[state.rng.gen_range(0..non_negative_jokers.len())];
                state.jokers[idx].is_negative = true;
            }
            state.hand_size_modifier -= 1;
        }
        SpectralId::Immolate => {
            let to_remove = selected_indices.iter().take(5).copied().collect::<Vec<_>>();
            let mut sorted = to_remove;
            sorted.sort_by(|a, b| b.cmp(a));
            for idx in sorted {
                if idx < state.hand.len() {
                    state.hand.remove(idx);
                }
            }
            state.money += 20;
        }
        SpectralId::Ankh => {
            let enabled_jokers: Vec<usize> = state
                .jokers
                .iter()
                .enumerate()
                .filter(|(_, j)| j.enabled)
                .map(|(i, _)| i)
                .collect();
            if !enabled_jokers.is_empty() {
                let keep_idx = enabled_jokers[state.rng.gen_range(0..enabled_jokers.len())];
                let kept = state.jokers[keep_idx].clone();
                state.jokers.clear();
                state.jokers.push(kept.clone());
                state.jokers.push(kept);
            }
        }
        SpectralId::DejaVu => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].seal = Seal::Red;
            }
        }
        SpectralId::Hex => {
            let enabled_jokers: Vec<usize> = state
                .jokers
                .iter()
                .enumerate()
                .filter(|(_, j)| j.enabled)
                .map(|(i, _)| i)
                .collect();
            if !enabled_jokers.is_empty() {
                let keep_idx = enabled_jokers[state.rng.gen_range(0..enabled_jokers.len())];
                let mut kept = state.jokers[keep_idx].clone();
                kept.edition = Edition::Polychrome;
                state.jokers.clear();
                state.jokers.push(kept);
            }
        }
        SpectralId::Trance => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].seal = Seal::Blue;
            }
        }
        SpectralId::Medium => {
            if let Some(&idx) = selected_indices.first() {
                state.hand[idx].seal = Seal::Purple;
            }
        }
        SpectralId::Cryptid => {
            if let Some(&idx) = selected_indices.first() {
                let card = state.hand[idx];
                state.deck.push(card);
                state.deck.push(card);
            }
        }
        SpectralId::TheSoul => {
            if state.jokers.len() < state.effective_joker_slot_limit() {
                let joker_id = JokerId::random_legendary(&mut state.rng);
                state.jokers.push(JokerSlot::new(joker_id));
            }
        }
    }
    state.last_used_consumable = Some(consumable.clone());
}

/// 執行 PostBlind 階段邏輯
fn execute_post_blind_logic(
    state: &mut EnvState,
    action_type: i32,
    result: &mut StepResult,
) {
    if action_type == ACTION_TYPE_CASH_OUT {
        state.money += state.reward;
        state.reward = 0;

        // ReservedParking
        let face_cards_in_hand = state.hand.iter().filter(|c| c.is_face()).count();
        let reserved_parking_count = state
            .jokers
            .iter()
            .filter(|j| j.enabled && j.id == JokerId::ReservedParking)
            .count();
        let total_rolls = face_cards_in_hand * reserved_parking_count;
        for _ in 0..total_rolls {
            if state.rng.gen_bool(0.5) {
                state.money += 1;
            }
        }

        // Egg
        for joker in state.jokers.iter_mut() {
            if joker.enabled && joker.id == JokerId::Egg {
                joker.sell_value += 3;
            }
        }

        // GiftCard
        let gift_card_count = state
            .jokers
            .iter()
            .filter(|j| j.enabled && j.id == JokerId::GiftCard)
            .count() as i64;
        if gift_card_count > 0 {
            for joker in state.jokers.iter_mut() {
                joker.sell_value += gift_card_count;
            }
        }

        // Rocket
        let rocket_money: i64 = state
            .jokers
            .iter()
            .filter(|j| j.enabled && j.id == JokerId::Rocket)
            .map(|j| j.get_rocket_money() as i64)
            .sum();
        state.money += rocket_money;

        // Satellite
        let satellite_count = state
            .jokers
            .iter()
            .filter(|j| j.enabled && j.id == JokerId::Satellite)
            .count() as i64;
        state.money += satellite_count * state.planets_used_this_run as i64;

        // Certificate
        let certificate_count = state
            .jokers
            .iter()
            .filter(|j| j.enabled && j.id == JokerId::Certificate)
            .count() as i64;
        let gold_seal_count = state.hand.iter().filter(|c| c.seal == Seal::Gold).count() as i64;
        state.money += certificate_count * gold_seal_count;

        // CloudNine
        let cloud_nine_count = state
            .jokers
            .iter()
            .filter(|j| j.enabled && j.id == JokerId::CloudNine)
            .count() as i64;
        let nines_in_deck = state
            .deck
            .iter()
            .chain(state.hand.iter())
            .chain(state.discarded.iter())
            .filter(|c| c.rank == 9)
            .count() as i64;
        state.money += cloud_nine_count * nines_in_deck;

        // Golden_Ticket
        let golden_ticket_count = state
            .jokers
            .iter()
            .filter(|j| j.enabled && j.id == JokerId::Golden_Ticket)
            .count() as i64;
        let gold_cards_in_full_deck = state
            .deck
            .iter()
            .chain(state.hand.iter())
            .chain(state.discarded.iter())
            .filter(|c| c.enhancement == Enhancement::Gold)
            .count() as i64;
        state.money += golden_ticket_count * gold_cards_in_full_deck * 3;

        // Delayed
        if state.discards_used_this_blind == 0 {
            let delayed_count = state
                .jokers
                .iter()
                .filter(|j| j.enabled && j.id == JokerId::Delayed)
                .count() as i64;
            state.money += delayed_count * 2;
        }

        // Blue Seal
        let blue_seal_count = state.hand.iter().filter(|c| c.seal == Seal::Blue).count();
        if blue_seal_count > 0 {
            if let Some(&last_hand_type) = state.played_hand_types.last() {
                if let Some(planet_id) = PlanetId::from_hand_type_index(last_hand_type) {
                    for _ in 0..blue_seal_count {
                        if !state.consumables.is_full() {
                            state.consumables.add(Consumable::Planet(planet_id));
                        }
                    }
                }
            }
        }

        // RoundEnded 觸發
        let trigger_ctx = TriggerContext {
            rng_value: state.rng.gen(),
            ..Default::default()
        };
        let trigger_result = trigger_joker_slot_events(
            GameEvent::RoundEnded,
            &mut state.jokers,
            &trigger_ctx,
        );

        for &idx in &trigger_result.jokers_to_destroy {
            if idx < state.jokers.len() {
                state.jokers[idx].enabled = false;
            }
        }

        // InvisibleJoker
        for joker in &mut state.jokers {
            if joker.enabled && joker.id == JokerId::InvisibleJoker {
                joker.counter += 1;
            }
        }

        state.stage = Stage::Shop;
        state.refresh_shop();

        // Tag 效果
        execute_tag_effects(state);

        // Perkeo
        let perkeo_count = state
            .jokers
            .iter()
            .filter(|j| j.enabled && j.id == JokerId::Perkeo)
            .count();
        for _ in 0..perkeo_count {
            let items_len = state.consumables.items.len();
            if items_len > 0 {
                let idx = state.rng.gen_range(0..items_len);
                let copy = state.consumables.items[idx].clone();
                state.consumables.items.push(copy);
            }
        }
    }
    let _ = result;
}

/// 執行 Tag 效果
fn execute_tag_effects(state: &mut EnvState) {
    // TopUpTag
    let topup_count = state.tags.iter().filter(|t| !t.used && t.id == TagId::TopUpTag).count();
    if topup_count > 0 {
        let common_jokers = JokerId::by_rarity(1);
        let effective_slots = state.effective_joker_slot_limit();
        let mut added = 0;
        for _ in 0..topup_count {
            for _ in 0..2 {
                if state.jokers.len() < effective_slots && !common_jokers.is_empty() && added < 2 {
                    let idx = state.rng.gen_range(0..common_jokers.len());
                    state.jokers.push(JokerSlot::new(common_jokers[idx]));
                    added += 1;
                }
            }
        }
        for tag in &mut state.tags {
            if !tag.used && tag.id == TagId::TopUpTag {
                tag.used = true;
            }
        }
    }

    // CharmTag
    let charm_count = state.tags.iter().filter(|t| !t.used && t.id == TagId::CharmTag).count();
    for _ in 0..charm_count {
        let all_tarots = TarotId::all();
        for _ in 0..2 {
            if !state.consumables.is_full() {
                let idx = state.rng.gen_range(0..all_tarots.len());
                state.consumables.add(Consumable::Tarot(all_tarots[idx]));
            }
        }
    }
    for tag in &mut state.tags {
        if !tag.used && tag.id == TagId::CharmTag {
            tag.used = true;
        }
    }

    // MeteorTag
    let meteor_count = state.tags.iter().filter(|t| !t.used && t.id == TagId::MeteorTag).count();
    for _ in 0..meteor_count {
        let all_planets = PlanetId::all();
        for _ in 0..2 {
            if !state.consumables.is_full() {
                let idx = state.rng.gen_range(0..all_planets.len());
                state.consumables.add(Consumable::Planet(all_planets[idx]));
            }
        }
    }
    for tag in &mut state.tags {
        if !tag.used && tag.id == TagId::MeteorTag {
            tag.used = true;
        }
    }

    // EtherealTag
    let ethereal_count = state.tags.iter().filter(|t| !t.used && t.id == TagId::EtherealTag).count();
    for _ in 0..ethereal_count {
        if !state.consumables.is_full() {
            let all_spectrals = SpectralId::all();
            let idx = state.rng.gen_range(0..all_spectrals.len());
            state.consumables.add(Consumable::Spectral(all_spectrals[idx]));
        }
    }
    for tag in &mut state.tags {
        if !tag.used && tag.id == TagId::EtherealTag {
            tag.used = true;
        }
    }

    // BuffoonTag
    let buffoon_count = state.tags.iter().filter(|t| !t.used && t.id == TagId::BuffoonTag).count();
    for _ in 0..buffoon_count {
        let effective_slots = state.effective_joker_slot_limit();
        if state.jokers.len() < effective_slots {
            let all_jokers = JokerId::all_available();
            let idx = state.rng.gen_range(0..all_jokers.len());
            state.jokers.push(JokerSlot::new(all_jokers[idx]));
        }
    }
    for tag in &mut state.tags {
        if !tag.used && tag.id == TagId::BuffoonTag {
            tag.used = true;
        }
    }

    // StandardTag
    let standard_count = state.tags.iter().filter(|t| !t.used && t.id == TagId::StandardTag).count();
    for _ in 0..standard_count {
        for _ in 0..3 {
            let rank = state.rng.gen_range(1..=13) as u8;
            let suit = state.rng.gen_range(0..4) as u8;
            state.deck.push(Card::new(rank, suit));
        }
    }
    for tag in &mut state.tags {
        if !tag.used && tag.id == TagId::StandardTag {
            tag.used = true;
        }
    }
}

/// 執行 Shop 階段邏輯
fn execute_shop_logic(
    state: &mut EnvState,
    action_type: i32,
    action_id: u32,
    result: &mut StepResult,
) {
    match action_type {
        ACTION_TYPE_BUY_JOKER => {
            let index = action_id as usize;
            if let Some(item) = state.shop.items.get(index) {
                if state.can_afford(item.cost)
                    && state.jokers.len() < state.effective_joker_slot_limit()
                {
                    let cost = item.cost;
                    result.action_cost = cost;
                    state.money -= cost;
                    if let Some(bought) = state.shop.buy(index) {
                        let mut joker = bought.joker;
                        if joker.id == JokerId::ToDoList {
                            let hand_type = state.rng.gen_range(0..13) as u8;
                            joker.set_todo_hand_type(hand_type);
                        }
                        state.jokers.push(joker);
                    }
                }
            }
        }

        ACTION_TYPE_NEXT_ROUND => {
            let current_blind = state.blind_type.unwrap_or(BlindType::Small);

            if current_blind == BlindType::Boss {
                if state.advance_ante() {
                    state.blind_type = None;
                    state.stage = Stage::PreBlind;
                    state.round += 1;

                    if state.deck_type == DeckType::Anaglyph {
                        state.tags.push(Tag::new(TagId::DoubleTag));
                    }

                    state.update_jokers(JokerId::Rocket, |j| {
                        j.increment_rocket_money();
                    });
                } else {
                    state.stage = Stage::End(GameEnd::Win);
                    result.done = true;
                }
            } else {
                state.stage = Stage::PreBlind;
                state.round += 1;
            }

            // Perishable
            for joker in &mut state.jokers {
                if joker.is_perishable {
                    joker.perishable_rounds -= 1;
                }
            }
            state.jokers.retain(|j| !j.is_perishable || j.perishable_rounds > 0);

            // Rental
            let rental_cost = 3i64;
            let rental_count = state.jokers.iter().filter(|j| j.is_rental).count() as i64;
            let total_rental_cost = rental_count * rental_cost;
            if state.money >= total_rental_cost {
                state.money -= total_rental_cost;
            } else {
                while state.jokers.iter().filter(|j| j.is_rental).count() > 0 {
                    let current_rental = state.jokers.iter().filter(|j| j.is_rental).count() as i64;
                    let current_cost = current_rental * rental_cost;
                    if state.money >= current_cost {
                        state.money -= current_cost;
                        break;
                    }
                    if let Some(idx) = state.jokers.iter().position(|j| j.is_rental) {
                        state.jokers.remove(idx);
                    }
                }
            }
        }

        ACTION_TYPE_REROLL => {
            let mut reroll_cost = state.shop.current_reroll_cost();

            let mut chaos_free_reroll = false;
            for joker in &state.jokers {
                if joker.enabled && joker.has_chaos_free_reroll() {
                    chaos_free_reroll = true;
                    break;
                }
            }
            if chaos_free_reroll {
                reroll_cost = 0;
            }

            if reroll_cost <= state.money + state.debt_limit() {
                result.action_cost = reroll_cost;
                state.money -= reroll_cost;
                state.reroll_shop();

                if chaos_free_reroll {
                    state.update_first_joker(JokerId::ChaosTheClown, |j| {
                        j.use_chaos_free_reroll();
                    });
                }

                state.rerolls_this_run += 1;

                state.update_jokers(JokerId::Flash, |j| {
                    j.flash_card_mult += 2;
                });
            }
        }

        ACTION_TYPE_SELL_JOKER => {
            let index = action_id as usize;
            if index < state.jokers.len() && !state.jokers[index].is_eternal {
                let sold_joker = state.jokers.remove(index);
                let sell_value = sold_joker.sell_value;

                state.last_sold_joker_id = sold_joker.id.to_index() as i32;

                if sold_joker.id == JokerId::DietCola {
                    state.tags.push(Tag::new(TagId::DoubleTag));
                }

                if sold_joker.id == JokerId::InvisibleJoker && sold_joker.counter >= 2 {
                    let enabled_jokers: Vec<usize> = state
                        .jokers
                        .iter()
                        .enumerate()
                        .filter(|(_, j)| j.enabled)
                        .map(|(i, _)| i)
                        .collect();
                    if !enabled_jokers.is_empty()
                        && state.jokers.len() < state.effective_joker_slot_limit()
                    {
                        let target_idx = enabled_jokers[state.rng.gen_range(0..enabled_jokers.len())];
                        let copied = state.jokers[target_idx].clone();
                        state.jokers.push(copied);
                    }
                }

                if sold_joker.id == JokerId::Luchador {
                    state.boss_blind = None;
                }

                state.money += sell_value;

                let trigger_ctx = TriggerContext::default();
                let _trigger_result = trigger_joker_slot_events(
                    GameEvent::JokerSold,
                    &mut state.jokers,
                    &trigger_ctx,
                );
            }
        }

        ACTION_TYPE_USE_CONSUMABLE => {
            let index = action_id as usize;
            if let Some(consumable) = state.consumables.use_item(index) {
                state.last_consumable_id = consumable.to_global_index() as i32;

                match &consumable {
                    Consumable::Planet(planet_id) => {
                        let hand_type_idx = planet_id.hand_type_index();
                        state.hand_levels.upgrade(hand_type_idx);
                        state.planet_used_hand_types |= 1 << hand_type_idx;

                        let trigger_ctx = TriggerContext::default();
                        let _trigger_result = trigger_joker_slot_events(
                            GameEvent::PlanetUsed,
                            &mut state.jokers,
                            &trigger_ctx,
                        );
                        state.planets_used_this_run += 1;
                        state.last_used_consumable = Some(consumable.clone());
                    }
                    Consumable::Tarot(tarot_id) => {
                        state.tarots_used_this_run += 1;
                        if *tarot_id == TarotId::TheFool {
                            if let Some(last) = state.last_used_consumable.clone() {
                                if !state.consumables.is_full() {
                                    state.consumables.add(last);
                                }
                            }
                        } else {
                            state.last_used_consumable = Some(consumable.clone());
                        }
                    }
                    Consumable::Spectral(spectral_id) => {
                        match spectral_id {
                            SpectralId::BlackHole => {
                                state.hand_levels.upgrade_all();
                            }
                            _ => {}
                        }
                        state.last_used_consumable = Some(consumable.clone());
                    }
                }
            }
        }

        ACTION_TYPE_BUY_VOUCHER => {
            if let Some(voucher_id) = state.shop_voucher {
                let cost = voucher_id.cost();
                if state.can_afford(cost) {
                    result.action_cost = cost;
                    state.money -= cost;
                    state.voucher_effects.buy(voucher_id);
                    state.shop_voucher = None;
                }
            }
        }

        ACTION_TYPE_BUY_PACK => {
            let index = action_id as usize;
            if let Some(pack) = state.shop_packs.get(index).cloned() {
                if state.can_afford(pack.cost) {
                    let cost = pack.cost;
                    result.action_cost = cost;
                    state.money -= cost;

                    if state.has_joker(JokerId::Hallucination) && state.rng.gen_range(0..2) == 0 {
                        let tarot_id = TarotId::from_index(state.rng.gen_range(0..22));
                        if let Some(tarot) = tarot_id {
                            state.consumables.add(Consumable::Tarot(tarot));
                        }
                    }

                    let full_pack_type = pack.pack_type.to_pack_type();
                    let contents = PackContents::generate(full_pack_type, &mut state.rng);
                    let pick_count = full_pack_type.pick_count();
                    let effective_joker_slots = state.effective_joker_slot_limit();

                    for item in contents.items.into_iter().take(pick_count) {
                        match item {
                            PackItem::Tarot(tarot_id) => {
                                state.consumables.add(Consumable::Tarot(tarot_id));
                            }
                            PackItem::Planet(planet_id) => {
                                state.consumables.add(Consumable::Planet(planet_id));
                            }
                            PackItem::Spectral(spectral_id) => {
                                state.consumables.add(Consumable::Spectral(spectral_id));
                            }
                            PackItem::Joker(joker_id, edition) => {
                                if state.jokers.len() < effective_joker_slots {
                                    let mut new_joker = JokerSlot::new(joker_id);
                                    new_joker.edition = edition;
                                    state.jokers.push(new_joker);
                                }
                            }
                            PackItem::PlayingCard(card) => {
                                state.deck.push(card.clone());
                                for joker in &mut state.jokers {
                                    if joker.enabled {
                                        joker.update_hologram_on_card_added(1);
                                    }
                                }
                            }
                        }
                    }

                    state.shop_packs.remove(index);
                }
            }
        }

        _ => {}
    }
}

// ============================================================================
// gRPC 服務（支援多遊戲並發）
// ============================================================================

/// Session TTL for cleanup (30 minutes)
const SESSION_TTL: Duration = Duration::from_secs(30 * 60);

/// Session wrapper that tracks last access time for TTL cleanup.
struct Session {
    state: EnvState,
    last_accessed: Instant,
}

impl Session {
    fn new(state: EnvState) -> Self {
        Self {
            state,
            last_accessed: Instant::now(),
        }
    }

    fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }

    fn is_stale(&self) -> bool {
        self.last_accessed.elapsed() > SESSION_TTL
    }
}

impl std::ops::Deref for Session {
    type Target = EnvState;
    fn deref(&self) -> &EnvState {
        &self.state
    }
}

impl std::ops::DerefMut for Session {
    fn deref_mut(&mut self) -> &mut EnvState {
        &mut self.state
    }
}

struct EnvService {
    /// Game sessions keyed by session_id.
    games: std::sync::Arc<DashMap<u64, Session>>,
    /// 下一個 session_id
    next_session_id: std::sync::Arc<AtomicU64>,
    /// 每 N 步輸出一次 profiling，0 表示關閉
    profile_every: u64,
    /// profiling step counter
    profile_counter: AtomicU64,
}

impl Default for EnvService {
    fn default() -> Self {
        let profile_every = env::var("JOKER_PROFILE_EVERY")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        Self {
            games: std::sync::Arc::new(DashMap::new()),
            next_session_id: std::sync::Arc::new(AtomicU64::new(1)),
            profile_every,
            profile_counter: AtomicU64::new(0),
        }
    }
}

impl EnvService {
    /// 獲取或創建遊戲狀態
    fn get_or_create_game(&self, session_id: u64, seed: u64) -> Result<u64, Status> {
        if session_id == 0 {
            // Sweep stale sessions before creating a new one
            self.cleanup_stale_sessions();

            let new_id = self.next_session_id.fetch_add(1, Ordering::SeqCst);
            self.games.insert(new_id, Session::new(EnvState::new(seed)));
            Ok(new_id)
        } else {
            // Reset existing session
            self.games.insert(session_id, Session::new(EnvState::new(seed)));
            Ok(session_id)
        }
    }

    /// Remove sessions that have not been accessed within SESSION_TTL.
    fn cleanup_stale_sessions(&self) {
        self.games.retain(|_id, session| !session.is_stale());
    }
}

#[tonic::async_trait]
impl JokerEnv for EnvService {
    async fn reset(
        &self,
        request: Request<ResetRequest>,
    ) -> Result<Response<ResetResponse>, Status> {
        let req = request.into_inner();
        let seed = req.seed;
        let session_id = self.get_or_create_game(req.session_id, seed)?;

        let state = self
            .games
            .get(&session_id)
            .ok_or_else(|| Status::internal("session not found"))?;

        let observation = Observation {
            features: Some(observation_from_state(&state)),
            action_mask: Some(action_mask_from_state(&state, false)),
        };

        let mut info = EnvInfo {
            // 基本狀態
            episode_step: state.episode_step,
            chips: state.score,
            mult: 1,
            blind_target: state.required_score(),

            // 擴展狀態
            ante: state.ante.to_int(),
            stage: 0,       // PreBlind
            blind_type: -1, // None
            plays_left: state.plays_left as i32,
            discards_left: state.discards_left as i32,
            money: state.money as i32,

            // 事件追蹤（reset 時無 delta）
            score_delta: 0,
            money_delta: 0,
            last_action_type: -1,
            last_action_cost: 0,

            // Joker 狀態
            joker_count: state.jokers.len() as i32,
            joker_slot_limit: state.effective_joker_slot_limit() as i32,

            // 遊戲結束狀態
            game_end: 0,
            blind_cleared: false,

            // 動作細節（reset 時無動作）
            cards_played: 0,
            cards_discarded: 0,
            hand_type: -1,

            // Skip Blind 相關（reset 時無 Tag）
            tag_id: -1,

            // 消耗品相關（reset 時無消耗品使用）
            consumable_id: -1,

            // Joker 交易相關
            joker_sold_id: -1,
            best_shop_joker_cost: state.shop.items.iter().map(|item| item.cost as i32).max().unwrap_or(0),

            // v6.4: 手牌潛力指標
            ..Default::default()
        };

        // v6.4: 計算手牌潛力
        let (flush_pot, straight_pot, pairs_pot) = calculate_hand_potential(&state.hand);
        info.flush_potential = flush_pot;
        info.straight_potential = straight_pot;
        info.pairs_potential = pairs_pot;

        // v7.0: Boss Blind ID（Reset 時無 Boss）
        info.boss_blind_id = -1;

        Ok(Response::new(ResetResponse {
            observation: Some(observation),
            info: Some(info),
            session_id,
        }))
    }

    async fn step(&self, request: Request<StepRequest>) -> Result<Response<StepResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let action = req.action.unwrap_or(Action {
            action_id: 0,
            params: vec![],
            action_type: ACTION_TYPE_SELECT,
        });

        let mut state = self.games.get_mut(&session_id).ok_or_else(|| {
            Status::not_found(format!("session {} not found, call Reset first", session_id))
        })?;
        state.touch(); // Update session TTL

        let do_profile = self.profile_every > 0
            && (self.profile_counter.fetch_add(1, Ordering::Relaxed) + 1) % self.profile_every == 0;
        let t0 = if do_profile { Some(Instant::now()) } else { None };

        let action_type = action.action_type;
        let action_id = action.action_id as u32;
        let params: Vec<i32> = action.params.clone();

        // 調用共享函數執行完整的遊戲邏輯
        let result = execute_step_logic(&mut state, action_type, action_id, &params);

        // Profiling
        if let Some(t0) = t0 {
            let elapsed = t0.elapsed();
            println!(
                "PROFILE step={} session={} total={:?}",
                state.episode_step, session_id, elapsed
            );
        }

        // 使用共享函數構建響應
        let step_response = build_step_response(&state, &result, action_type);
        Ok(Response::new(step_response))
    }

    async fn step_batch(
        &self,
        request: Request<StepBatchRequest>,
    ) -> Result<Response<StepBatchResponse>, Status> {
        let req = request.into_inner();
        let mut responses = Vec::with_capacity(req.requests.len());
        for step_req in req.requests {
            let resp = self.step(Request::new(step_req)).await?;
            responses.push(resp.into_inner());
        }
        Ok(Response::new(StepBatchResponse { responses }))
    }

    async fn get_spec(
        &self,
        _request: Request<GetSpecRequest>,
    ) -> Result<Response<GetSpecResponse>, Status> {
        let observation = TensorSpec {
            shape: vec![OBS_SIZE],
            dtype: "f32".to_string(),
        };

        let action_mask = TensorSpec {
            shape: vec![ACTION_MASK_SIZE],
            dtype: "f32".to_string(),
        };

        Ok(Response::new(GetSpecResponse {
            observation: Some(observation),
            action_mask: Some(action_mask),
            action_space: ACTION_MASK_SIZE,
        }))
    }

    type TrainingStreamStream = Pin<Box<dyn tokio_stream::Stream<Item = Result<StreamResponse, Status>> + Send>>;

    async fn training_stream(
        &self,
        request: Request<Streaming<StreamRequest>>,
    ) -> Result<Response<Self::TrainingStreamStream>, Status> {
        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(32);  // 背壓緩衝
        let games = self.games.clone();
        let next_session_id = self.next_session_id.clone();
        let profile_every = self.profile_every;
        let profile_counter = std::sync::Arc::new(AtomicU64::new(0));

        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(req) => {
                        let response = match req.request_type {
                            Some(stream_request::RequestType::Reset(reset_req)) => {
                                // 處理 Reset - 複用邏輯
                                let seed = reset_req.seed;
                                let session_id = if reset_req.session_id == 0 {
                                    let new_id = next_session_id.fetch_add(1, Ordering::SeqCst);
                                    games.insert(new_id, Session::new(EnvState::new(seed)));
                                    new_id
                                } else {
                                    games.insert(reset_req.session_id, Session::new(EnvState::new(seed)));
                                    reset_req.session_id
                                };

                                let state = match games.get(&session_id) {
                                    Some(s) => s,
                                    None => {
                                        let _ = tx.send(Err(Status::internal("session not found"))).await;
                                        break;
                                    }
                                };

                                let observation = Observation {
                                    features: Some(observation_from_state(&state)),
                                    action_mask: Some(action_mask_from_state(&state, false)),
                                };

                                let (flush_pot, straight_pot, pairs_pot) = calculate_hand_potential(&state.hand);

                                let info = EnvInfo {
                                    episode_step: state.episode_step,
                                    chips: state.score,
                                    mult: 1,
                                    blind_target: state.required_score(),
                                    ante: state.ante.to_int(),
                                    stage: 0,
                                    blind_type: -1,
                                    plays_left: state.plays_left as i32,
                                    discards_left: state.discards_left as i32,
                                    money: state.money as i32,
                                    score_delta: 0,
                                    money_delta: 0,
                                    last_action_type: -1,
                                    last_action_cost: 0,
                                    joker_count: state.jokers.len() as i32,
                                    joker_slot_limit: state.effective_joker_slot_limit() as i32,
                                    game_end: 0,
                                    blind_cleared: false,
                                    cards_played: 0,
                                    cards_discarded: 0,
                                    hand_type: -1,
                                    tag_id: -1,
                                    consumable_id: -1,
                                    joker_sold_id: -1,
                                    best_shop_joker_cost: state.shop.items.iter().map(|item| item.cost as i32).max().unwrap_or(0),
                                    flush_potential: flush_pot,
                                    straight_potential: straight_pot,
                                    pairs_potential: pairs_pot,
                                    boss_blind_id: -1,
                                    ..Default::default()
                                };

                                StreamResponse {
                                    response_type: Some(stream_response::ResponseType::Reset(ResetResponse {
                                        observation: Some(observation),
                                        info: Some(info),
                                        session_id,
                                    }))
                                }
                            }
                            Some(stream_request::RequestType::Step(step_req)) => {
                                // 處理 Step - 使用共享函數確保與 unary RPC 行為一致
                                let session_id = step_req.session_id;
                                let action = step_req.action.unwrap_or(Action {
                                    action_id: 0,
                                    params: vec![],
                                    action_type: ACTION_TYPE_SELECT,
                                });

                                let mut state = match games.get_mut(&session_id) {
                                    Some(s) => s,
                                    None => {
                                        let _ = tx.send(Err(Status::not_found(format!(
                                            "session {} not found",
                                            session_id
                                        )))).await;
                                        continue;
                                    }
                                };
                                state.touch();

                                let do_profile = profile_every > 0
                                    && (profile_counter.fetch_add(1, Ordering::Relaxed) + 1) % profile_every == 0;
                                let t0 = if do_profile { Some(Instant::now()) } else { None };

                                let action_type = action.action_type;
                                let action_id = action.action_id as u32;
                                let params: Vec<i32> = action.params.clone();

                                // 調用共享函數執行完整邏輯
                                let result = execute_step_logic(&mut state, action_type, action_id, &params);

                                // 使用共享函數構建響應
                                let step_response = build_step_response(&state, &result, action_type);

                                if do_profile {
                                    let total = t0.unwrap().elapsed();
                                    println!(
                                        "PROFILE_STREAM step={} session={} total={:?}",
                                        state.episode_step, session_id, total
                                    );
                                }

                                StreamResponse {
                                    response_type: Some(stream_response::ResponseType::Step(step_response))
                                }
                            }
                            None => continue,
                        };

                        if tx.send(Ok(response)).await.is_err() {
                            break;  // 客戶端斷開
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(e.to_string()))).await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行參數
    let args: Vec<String> = env::args().collect();
    let mut port: u16 = 50051;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(50051);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let addr = format!("127.0.0.1:{}", port).parse()?;
    let env = EnvService::default();

    println!("JokerEnv gRPC server listening on {}", addr);
    println!("Full game flow enabled: PreBlind -> Blind -> PostBlind -> Shop -> ...");

    tonic::transport::Server::builder()
        .add_service(JokerEnvServer::new(env))
        .serve(addr)
        .await?;

    Ok(())
}
