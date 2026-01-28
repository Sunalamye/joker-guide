// 這些 API 是為未來擴展保留的公開介面
#![allow(dead_code)]

use std::sync::Mutex;

use rand::Rng;
use tonic::{Request, Response, Status};

use joker_env::proto::joker_env_server::{JokerEnv, JokerEnvServer};
use joker_env::proto::{
    Action, EnvInfo, GetSpecRequest, GetSpecResponse, Observation, ResetRequest, ResetResponse,
    StepRequest, StepResponse, TensorSpec,
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
};

// 從 service 模組導入
use service::{
    action_mask_from_state, build_selected_hand, calculate_play_score, observation_from_state,
    EnvState,
};

// ============================================================================
// gRPC 服務
// ============================================================================

struct EnvService {
    state: Mutex<EnvState>,
}

impl Default for EnvService {
    fn default() -> Self {
        Self {
            state: Mutex::new(EnvState::new(0)),
        }
    }
}

#[tonic::async_trait]
impl JokerEnv for EnvService {
    async fn reset(
        &self,
        request: Request<ResetRequest>,
    ) -> Result<Response<ResetResponse>, Status> {
        let seed = request.into_inner().seed;
        let mut state = self
            .state
            .lock()
            .map_err(|_| Status::internal("lock error"))?;

        *state = EnvState::new(seed);

        let observation = Observation {
            features: Some(observation_from_state(&state)),
            action_mask: Some(action_mask_from_state(&state, false)),
        };

        let info = EnvInfo {
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
        };

        Ok(Response::new(ResetResponse {
            observation: Some(observation),
            info: Some(info),
        }))
    }

    async fn step(&self, request: Request<StepRequest>) -> Result<Response<StepResponse>, Status> {
        let StepRequest { action } = request.into_inner();
        let action = action.unwrap_or(Action {
            action_id: 0,
            params: vec![],
            action_type: ACTION_TYPE_SELECT,
        });

        let mut state = self
            .state
            .lock()
            .map_err(|_| Status::internal("lock error"))?;

        let action_type = action.action_type;
        let action_id = action.action_id as u32;

        // 記錄動作前狀態（用於計算 delta）
        let score_before = state.score;
        let money_before = state.money;

        let reward = 0.0; // 獎勵由 Python 端計算
        let mut done = false;
        let mut action_cost = 0i64;
        let mut blind_cleared = false;
        let mut cards_played = 0i32;
        let mut cards_discarded = 0i32;
        let mut hand_type_id = -1i32;

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
                                    joker.enabled = false; // 自毀
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
                                rank: 1, // Stone cards don't use rank
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
                                if state.jokers.len() < effective_slots && !common_jokers.is_empty()
                                {
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

                        // TheIdol: 每回合開始時隨機設置目標牌（rank + suit）
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

                        // Ceremonial: 選擇 Blind 時銷毀最右邊的其他 Joker，獲得 2x 售價 Mult
                        let ceremonial_indices: Vec<usize> = state
                            .jokers
                            .iter()
                            .enumerate()
                            .filter(|(_, j)| j.enabled && j.id == JokerId::Ceremonial)
                            .map(|(i, _)| i)
                            .collect();
                        let mut jokers_destroyed_by_ceremonial = 0;
                        for ceremonial_idx in ceremonial_indices {
                            // 找最右邊的非 Ceremonial 且 enabled 的 Joker
                            let rightmost_target =
                                state.jokers.iter().enumerate().rev().find(|(i, j)| {
                                    *i != ceremonial_idx && j.enabled && j.id != JokerId::Ceremonial
                                });
                            if let Some((target_idx, target_joker)) = rightmost_target {
                                let sell_value = target_joker.sell_value;
                                // 用 counter 存儲累積的 Mult (2x 售價)
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
                            // 找所有非 Madness 且 enabled 的 Joker
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
                            for (card, face_down) in
                                state.hand.iter_mut().zip(face_down_flags.iter())
                            {
                                if *face_down {
                                    card.face_down = true;
                                }
                            }
                        }

                        // TheHouse: 第一手全部面朝下（在 deal() 後設置）
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
                                // 自動使用的消耗品只處理 Planet 升級
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

                        // TheManacle: 手牌上限 -1（已在 deal() 中處理，這裡記錄效果）
                        // Note: 實際效果需要在抽牌邏輯中處理

                        // TheFish: 面朝下的牌打亂順序（已經面朝下的牌互換位置）
                        if state.boss_blind == Some(BossBlind::TheFish) {
                            // 找出所有面朝下牌的索引
                            let face_down_indices: Vec<usize> = state
                                .hand
                                .iter()
                                .enumerate()
                                .filter(|(_, c)| c.face_down)
                                .map(|(i, _)| i)
                                .collect();

                            // 打亂這些牌的位置
                            if face_down_indices.len() > 1 {
                                let mut shuffled = face_down_indices.clone();
                                // 先生成所有隨機值
                                let shuffle_indices: Vec<usize> = (1..shuffled.len())
                                    .rev()
                                    .map(|i| state.rng.gen_range(0..=i))
                                    .collect();
                                // Fisher-Yates shuffle
                                for (idx, j) in
                                    (1..shuffled.len()).rev().zip(shuffle_indices.iter())
                                {
                                    shuffled.swap(idx, *j);
                                }
                                // 交換牌
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
                        let _blind_type = state.blind_type.unwrap_or(BlindType::Small);
                        state.skip_blind();

                        // 更新全局計數器（用於 ScoringContext）
                        state.blinds_skipped += 1;

                        // 使用觸發系統處理跳過 Blind 的效果
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
                match action_type {
                    ACTION_TYPE_SELECT => {
                        let mask = action_id & ((1 << HAND_SIZE) - 1);
                        let count = mask.count_ones() as usize;
                        if count <= MAX_SELECTED {
                            state.selected_mask = mask;
                        }
                    }

                    ACTION_TYPE_PLAY => {
                        if state.plays_left > 0 {
                            let mut selected =
                                build_selected_hand(&state.hand, state.selected_mask);
                            let selected_count = selected.len();
                            cards_played = selected_count as i32;

                            // ThePillar: 已打過的牌不再計分（標記為 face_down）
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
                                // 計算增強牌數量 (DriversLicense)
                                let enhanced_cards_in_deck = state
                                    .deck
                                    .iter()
                                    .filter(|c| c.enhancement != Enhancement::None)
                                    .count()
                                    as i32;
                                // DNA: 是否是第一手牌；DuskJoker/Acrobat: 是否是最後一手牌
                                let is_first_hand = state.hands_played_this_blind == 0;
                                let is_final_hand = state.plays_left == 1;
                                // Selzer: 獲取剩餘重觸發次數
                                let selzer_charges = state
                                    .jokers
                                    .iter()
                                    .find(|j| j.enabled && j.id == JokerId::Selzer)
                                    .map(|j| j.get_selzer_charges())
                                    .unwrap_or(0);
                                // 克隆 hand_levels 以避免借用檢查問題
                                let hand_levels_clone = state.hand_levels.clone();
                                // Plasma Deck 計分模式
                                let uses_plasma_scoring = state.deck_type.uses_plasma_scoring();
                                // Observatory Voucher 效果
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
                                hand_type_id = hand_type_idx as i32;

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

                                // Boss Blind 限制檢查（Python 端計算獎勵懲罰）
                                let _violated_boss_rule = !eye_ok || !mouth_ok;

                                state.played_hand_types.push(hand_type_idx);
                                if state.first_hand_type.is_none() {
                                    state.first_hand_type = Some(hand_type_idx);
                                }

                                // Obelisk: 更新牌型計數和連續非最常打牌型 streak
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
                                            joker.reset_obelisk_streak(); // 打了最常打的，重置
                                        } else {
                                            joker.increment_obelisk_streak(); // 連續非最常打 +1
                                        }
                                    }
                                }

                                state.score += score_gained;
                                state.plays_left -= 1;
                                state.hands_played_this_blind += 1; // DNA: 追蹤第一手牌
                                state.money += score_result.money_gained;

                                // Selzer: 更新 charges 並在用完時銷毀
                                if score_result.selzer_charges_used > 0 {
                                    for joker in &mut state.jokers {
                                        if joker.enabled && joker.id == JokerId::Selzer {
                                            if joker.use_selzer_charges(score_result.selzer_charges_used) {
                                                joker.enabled = false; // 用完自毀
                                            }
                                            break;
                                        }
                                    }
                                }

                                // Lucky_Cat: 更新 Lucky 觸發累積的 X Mult
                                if score_result.lucky_triggers > 0 {
                                    state.update_jokers(JokerId::Lucky_Cat, |j| {
                                        j.update_lucky_cat_on_trigger(score_result.lucky_triggers);
                                    });
                                }

                                // SpaceJoker: 1/4 機率升級出過的牌型
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

                                // TheArm (Boss Blind): 降低出過的牌型等級
                                if state.boss_blind == Some(BossBlind::TheArm) {
                                    state.hand_levels.downgrade(hand_type_idx);
                                }

                                // TheOx (Boss Blind): 出 #(ante) 牌型時失去 $1
                                // Ante 1 = High Card (0), Ante 2 = Pair (1), etc.
                                if state.boss_blind == Some(BossBlind::TheOx) {
                                    let ante_hand_idx = (state.ante.to_int() - 1) as usize;
                                    if hand_type_idx == ante_hand_idx {
                                        state.money = (state.money - 1).max(0);
                                    }
                                }

                                // ThePillar: 記錄這次打出的牌（只記錄首次打出，非 face_down 的）
                                if state.boss_blind == Some(BossBlind::ThePillar) {
                                    for card in &selected {
                                        if !card.face_down {
                                            let key = (card.rank as u8, card.suit as u8);
                                            state.pillar_played_cards.insert(key);
                                        }
                                    }
                                }

                                // Vagabond: 出 ≤4 張牌時生成隨機 Tarot 卡
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
                                            state
                                                .consumables
                                                .add(Consumable::Tarot(all_tarots[idx]));
                                        }
                                    }
                                }

                                // EightBall: 打出 8 時創建隨機 Tarot 卡
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
                                            state
                                                .consumables
                                                .add(Consumable::Tarot(all_tarots[idx]));
                                        }
                                    }
                                }

                                // Hallucination: 出牌後 1/2 機率生成隨機 Tarot 卡
                                let hallucination_count = state
                                    .jokers
                                    .iter()
                                    .filter(|j| j.enabled && j.id == JokerId::Hallucination)
                                    .count();
                                for _ in 0..hallucination_count {
                                    if state.rng.gen_range(0..2) == 0
                                        && !state.consumables.is_full()
                                    {
                                        let all_tarots = TarotId::all();
                                        let idx = state.rng.gen_range(0..all_tarots.len());
                                        state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                                    }
                                }

                                // MidasMask: 打出人頭牌時變為 Gold 增強
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

                                // Vampire: 吸收打出牌的增強效果，獲得 +0.1 X Mult 並移除增強
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
                                            // 移除增強
                                        }
                                    }
                                    if enhancements_absorbed > 0 {
                                        state.jokers[v_idx]
                                            .update_vampire_on_enhancement(enhancements_absorbed);
                                    }
                                }

                                // Hiker: 打出的牌永久 +2 Chips
                                let hiker_count = state
                                    .jokers
                                    .iter()
                                    .filter(|j| j.enabled && j.id == JokerId::Hiker)
                                    .count()
                                    as i64;
                                if hiker_count > 0 {
                                    let selected_mask = state.selected_mask;
                                    for (idx, card) in state.hand.iter_mut().enumerate() {
                                        if ((selected_mask >> idx) & 1) == 1 {
                                            card.bonus_chips += 2 * hiker_count;
                                            // 多個 Hiker 疊加
                                        }
                                    }
                                }

                                // ToDoList: 打出特定牌型時 +$4，然後重新隨機選擇
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
                                    // 重新隨機選擇牌型 (0-12)
                                    let new_type = state.rng.gen_range(0..13) as u8;
                                    state.jokers[idx].set_todo_hand_type(new_type);
                                }

                                // Seance: Straight Flush 或 Royal Flush 時生成 Spectral 卡
                                // StraightFlush = 8, RoyalFlush = 9
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
                                            state
                                                .consumables
                                                .add(Consumable::Spectral(all_spectrals[idx]));
                                        }
                                    }
                                }

                                // 使用觸發系統處理 HandPlayed 事件
                                // 處理: IceCream, GreenJoker, RideTheBus, LoyaltyCard, Wee, Merry, Obelisk, Selzer
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

                                // IceCream: 如果 chips <= 0，自毀
                                for joker in &mut state.jokers {
                                    if joker.enabled && joker.id == JokerId::IceCream {
                                        if let JokerState::Accumulator { chips, .. } = &joker.state {
                                            if *chips <= 0 {
                                                joker.enabled = false;
                                            }
                                        }
                                    }
                                }

                                // LoyaltyCard: 計數器達到 6 時重置
                                for joker in &mut state.jokers {
                                    if joker.enabled && joker.id == JokerId::LoyaltyCard {
                                        if let JokerState::Counter { current, .. } = &mut joker.state {
                                            if *current >= 6 {
                                                *current = 0; // 觸發後重置
                                            }
                                        }
                                    }
                                }

                                // Cavendish: 每手牌後有 1/1000 機率自毀
                                let cavendish_rng: u32 = state.rng.gen_range(0..1000);
                                for joker in &mut state.jokers {
                                    if joker.enabled && joker.id == JokerId::Cavendish {
                                        if cavendish_rng == 0 {
                                            joker.enabled = false;
                                        }
                                    }
                                }

                                let selected_mask = state.selected_mask;

                                // GlassJoker: 記錄被破碎的 Glass 牌數量和人頭牌數量 (for Canio)
                                let glass_broken_count = score_result.glass_to_break.len() as i32;
                                let mut face_cards_destroyed = 0;
                                if glass_broken_count > 0 {
                                    // 計算被破碎的牌中有多少是人頭牌
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

                                state
                                    .break_glass_cards(selected_mask, &score_result.glass_to_break);

                                // GlassJoker: Glass 牌破碎時 +0.75 X Mult
                                if glass_broken_count > 0 {
                                    for joker in &mut state.jokers {
                                        if joker.enabled && joker.id == JokerId::GlassJoker {
                                            joker.update_glass_on_break(glass_broken_count);
                                        }
                                    }
                                }

                                // Canio: 人頭牌被銷毀時 +X1 Mult
                                if face_cards_destroyed > 0 {
                                    for joker in &mut state.jokers {
                                        if joker.enabled && joker.id == JokerId::Canio {
                                            joker.update_canio_on_face_destroyed(
                                                face_cards_destroyed,
                                            );
                                        }
                                    }
                                }

                                let required = state.required_score();

                                if state.score >= required {
                                    // Blind 過關
                                    blind_cleared = true;
                                    state.reward = state.calc_reward();
                                    state.stage = Stage::PostBlind;
                                } else if state.plays_left == 0 {
                                    // 出牌次數耗盡，遊戲失敗
                                    state.stage = Stage::End(GameEnd::Lose);
                                    done = true;
                                } else {
                                    state.deal();

                                    if state.boss_blind == Some(BossBlind::TheSerpent) {
                                        state.apply_serpent_effect();
                                    }

                                    if state.boss_blind == Some(BossBlind::TheHook) {
                                        state.apply_hook_discard();
                                    }
                                }
                            }
                        }
                    }

                    ACTION_TYPE_DISCARD => {
                        if state.discards_left > 0 && state.selected_mask > 0 {
                            let mask = state.selected_mask;

                            // 計算棄牌資訊（在棄牌前）
                            let selected_cards: Vec<Card> = state
                                .hand
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| (mask >> i) & 1 == 1)
                                .map(|(_, c)| *c)
                                .collect();

                            cards_discarded = mask.count_ones() as i32;
                            let _purple_count = state.discard_with_seals(mask);
                            state.discards_left -= 1;
                            state.discards_used_this_blind += 1;
                            state.selected_mask = 0;

                            // 計算棄牌相關統計
                            let face_count = selected_cards.iter().filter(|c| c.is_face()).count() as i32;
                            let jack_count = selected_cards.iter().filter(|c| c.rank == 11).count() as i32;
                            let king_count = selected_cards.iter().filter(|c| c.rank == 13).count() as i32;
                            let mut suit_count = [0i32; 4];
                            for card in &selected_cards {
                                if (card.suit as usize) < 4 {
                                    suit_count[card.suit as usize] += 1;
                                }
                            }

                            // 計算棄牌的牌型（用於 BurntJoker）
                            let discarded_hand_type = if !selected_cards.is_empty() {
                                score_hand(&selected_cards).id.to_index()
                            } else {
                                0
                            };

                            // 使用 trigger 系統處理所有棄牌相關 Joker
                            let trigger_ctx = TriggerContext {
                                discarded_face_count: face_count,
                                discarded_jack_count: jack_count,
                                discarded_king_count: king_count,
                                discarded_suit_count: suit_count,
                                discarded_count: cards_discarded,
                                discarded_hand_type,
                                ..Default::default()
                            };

                            let trigger_result = trigger_joker_slot_events(
                                GameEvent::CardDiscarded,
                                &mut state.jokers,
                                &trigger_ctx,
                            );

                            // 處理觸發結果
                            state.money += trigger_result.money_delta;

                            // 創建 Tarot 卡 (TradingCard)
                            for _ in 0..trigger_result.tarot_to_create {
                                if !state.consumables.is_full() {
                                    let all_tarots = TarotId::all();
                                    let idx = state.rng.gen_range(0..all_tarots.len());
                                    state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                                }
                            }

                            // 創建 Spectral 卡 (Sixth)
                            for _ in 0..trigger_result.spectral_to_create {
                                if !state.consumables.is_full() {
                                    let all_spectrals = SpectralId::all();
                                    let idx = state.rng.gen_range(0..all_spectrals.len());
                                    state.consumables.add(Consumable::Spectral(all_spectrals[idx]));
                                }
                            }

                            // 升級牌型等級 (BurntJoker)
                            for hand_idx in &trigger_result.hand_levels_to_upgrade {
                                state.hand_levels.upgrade(*hand_idx);
                            }

                            // 銷毀 Joker (Sixth, Ramen)
                            for &idx in &trigger_result.jokers_to_destroy {
                                if idx < state.jokers.len() {
                                    state.jokers[idx].enabled = false;
                                }
                            }
                        }
                    }

                    ACTION_TYPE_USE_CONSUMABLE => {
                        let index = action_id as usize;
                        if let Some(consumable) = state.consumables.use_item(index) {
                            // 根據消耗品類型更新狀態
                            match &consumable {
                                Consumable::Planet(planet_id) => {
                                    // 升級對應的牌型等級
                                    let hand_type_idx = planet_id.hand_type_index();
                                    state.hand_levels.upgrade(hand_type_idx);

                                    // Observatory: 標記此牌型已使用 Planet
                                    state.planet_used_hand_types |= 1 << hand_type_idx;

                                    // 使用 trigger 系統處理 PlanetUsed 事件
                                    // Constellation: 每使用 Planet 卡 +0.1 X Mult
                                    let trigger_ctx = TriggerContext::default();
                                    let _trigger_result = trigger_joker_slot_events(
                                        GameEvent::PlanetUsed,
                                        &mut state.jokers,
                                        &trigger_ctx,
                                    );
                                    // Satellite: 追蹤使用的 Planet 數量
                                    state.planets_used_this_run += 1;
                                    // 更新 last_used_consumable
                                    state.last_used_consumable = Some(consumable.clone());
                                }
                                Consumable::Tarot(tarot_id) => {
                                    // FortuneTeller: 使用 ctx.tarots_used_this_run 計分
                                    state.tarots_used_this_run += 1;

                                    // 獲取選中的牌索引
                                    let selected_indices: Vec<usize> = (0..state.hand.len())
                                        .filter(|&i| ((state.selected_mask >> i) & 1) == 1)
                                        .collect();

                                    match tarot_id {
                                        TarotId::TheMagician => {
                                            // 增強 1-2 張選中牌為 Lucky
                                            for &idx in selected_indices.iter().take(2) {
                                                state.hand[idx].enhancement = Enhancement::Lucky;
                                            }
                                        }
                                        TarotId::TheEmpress => {
                                            // 增強 1-2 張選中牌為 Mult
                                            for &idx in selected_indices.iter().take(2) {
                                                state.hand[idx].enhancement = Enhancement::Mult;
                                            }
                                        }
                                        TarotId::TheHierophant => {
                                            // 增強 1-2 張選中牌為 Bonus
                                            for &idx in selected_indices.iter().take(2) {
                                                state.hand[idx].enhancement = Enhancement::Bonus;
                                            }
                                        }
                                        TarotId::TheLovers => {
                                            // 增強 1 張選中牌為 Wild
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].enhancement = Enhancement::Wild;
                                            }
                                        }
                                        TarotId::TheChariot => {
                                            // 增強 1 張選中牌為 Steel
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].enhancement = Enhancement::Steel;
                                            }
                                        }
                                        TarotId::Justice => {
                                            // 增強 1 張選中牌為 Glass
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].enhancement = Enhancement::Glass;
                                            }
                                        }
                                        TarotId::TheDevil => {
                                            // 增強 1 張選中牌為 Gold
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].enhancement = Enhancement::Gold;
                                            }
                                        }
                                        TarotId::TheTower => {
                                            // 增強 1 張選中牌為 Stone
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].enhancement = Enhancement::Stone;
                                            }
                                        }
                                        TarotId::TheHermit => {
                                            // 金幣翻倍，最多 $20
                                            let doubled = (state.money * 2).min(state.money + 20);
                                            state.money = doubled;
                                        }
                                        TarotId::Strength => {
                                            // 選中牌 +1 點數（最高 K=13）
                                            for &idx in selected_indices.iter().take(2) {
                                                if state.hand[idx].rank < 13 {
                                                    state.hand[idx].rank += 1;
                                                }
                                            }
                                        }
                                        TarotId::TheHighPriestess => {
                                            // 生成最多 2 張隨機 Planet 卡
                                            let all_planets = PlanetId::all();
                                            for _ in 0..2 {
                                                if !state.consumables.is_full() {
                                                    let idx =
                                                        state.rng.gen_range(0..all_planets.len());
                                                    state
                                                        .consumables
                                                        .add(Consumable::Planet(all_planets[idx]));
                                                }
                                            }
                                        }
                                        TarotId::TheEmperor => {
                                            // 生成最多 2 張隨機 Tarot 卡
                                            let all_tarots = TarotId::all();
                                            for _ in 0..2 {
                                                if !state.consumables.is_full() {
                                                    let idx =
                                                        state.rng.gen_range(0..all_tarots.len());
                                                    state
                                                        .consumables
                                                        .add(Consumable::Tarot(all_tarots[idx]));
                                                }
                                            }
                                        }
                                        TarotId::Temperance => {
                                            // 獲得 Joker 售價總和，最多 $50
                                            let total_sell_value: i64 = state
                                                .jokers
                                                .iter()
                                                .filter(|j| j.enabled)
                                                .map(|j| j.sell_value)
                                                .sum();
                                            state.money += total_sell_value.min(50);
                                        }
                                        TarotId::TheStar => {
                                            // 轉換 1-3 張選中牌為 Diamonds
                                            for &idx in selected_indices.iter().take(3) {
                                                state.hand[idx].suit = 1; // Diamonds
                                            }
                                        }
                                        TarotId::TheMoon => {
                                            // 轉換 1-3 張選中牌為 Clubs
                                            for &idx in selected_indices.iter().take(3) {
                                                state.hand[idx].suit = 0; // Clubs
                                            }
                                        }
                                        TarotId::TheSun => {
                                            // 轉換 1-3 張選中牌為 Hearts
                                            for &idx in selected_indices.iter().take(3) {
                                                state.hand[idx].suit = 2; // Hearts
                                            }
                                        }
                                        TarotId::TheWorld => {
                                            // 轉換 1-3 張選中牌為 Spades
                                            for &idx in selected_indices.iter().take(3) {
                                                state.hand[idx].suit = 3; // Spades
                                            }
                                        }
                                        TarotId::TheHangedMan => {
                                            // 銷毀最多 2 張選中的牌
                                            // 從高索引到低索引移除，避免索引偏移問題
                                            let mut to_remove: Vec<usize> =
                                                selected_indices.iter().take(2).copied().collect();
                                            to_remove.sort_by(|a, b| b.cmp(a));
                                            for idx in to_remove {
                                                if idx < state.hand.len() {
                                                    state.hand.remove(idx);
                                                }
                                            }
                                        }
                                        TarotId::Death => {
                                            // 選 2 張牌，左邊變成右邊的複製
                                            if selected_indices.len() >= 2 {
                                                let left = selected_indices[0];
                                                let right = selected_indices[1];
                                                state.hand[left] = state.hand[right];
                                            }
                                        }
                                        TarotId::Judgement => {
                                            // 生成隨機 Joker（如果有槽位）
                                            if state.jokers.len()
                                                < state.effective_joker_slot_limit()
                                            {
                                                // 創建一個隨機 common joker
                                                let joker_id =
                                                    JokerId::random_common(&mut state.rng);
                                                state.jokers.push(JokerSlot::new(joker_id));
                                            }
                                        }
                                        TarotId::TheWheelOfFortune => {
                                            // 1/4 機率給隨機 Joker 添加 edition
                                            let joker_count = state.jokers.len();
                                            if joker_count > 0 && state.rng.gen_range(0..4) == 0 {
                                                let idx = state.rng.gen_range(0..joker_count);
                                                // 隨機選擇 Foil/Holographic/Polychrome
                                                let edition = match state.rng.gen_range(0..3) {
                                                    0 => Edition::Foil,
                                                    1 => Edition::Holographic,
                                                    _ => Edition::Polychrome,
                                                };
                                                state.jokers[idx].edition = edition;
                                            }
                                        }
                                        TarotId::TheFool => {
                                            // 複製上一張使用的消耗品（不包括 TheFool 自己）
                                            if let Some(last) = state.last_used_consumable.clone() {
                                                if !state.consumables.is_full() {
                                                    state.consumables.add(last);
                                                }
                                            }
                                        }
                                    }
                                    // 更新 last_used_consumable（TheFool 除外，它不會更新）
                                    if *tarot_id != TarotId::TheFool {
                                        state.last_used_consumable = Some(consumable.clone());
                                    }
                                }
                                Consumable::Spectral(spectral_id) => {
                                    // 獲取選中的牌索引
                                    let selected_indices: Vec<usize> = (0..state.hand.len())
                                        .filter(|&i| ((state.selected_mask >> i) & 1) == 1)
                                        .collect();

                                    match spectral_id {
                                        SpectralId::BlackHole => {
                                            // 所有牌型等級 +1
                                            state.hand_levels.upgrade_all();
                                        }
                                        SpectralId::Familiar => {
                                            // 銷毀 1 張選中牌，加 3 張隨機人頭牌到牌組
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
                                            // 銷毀 1 張選中牌，加 2 張 Ace 到牌組
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
                                            // 銷毀 1 張選中牌，加 4 張隨機數字牌到牌組
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
                                            // 加 Gold Seal 到選中牌
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].seal = Seal::Gold;
                                            }
                                        }
                                        SpectralId::Aura => {
                                            // 加 Foil/Holo/Poly 到選中牌
                                            if let Some(&idx) = selected_indices.first() {
                                                let editions = Edition::all_common();
                                                let edition = editions
                                                    [state.rng.gen_range(0..editions.len())];
                                                state.hand[idx].edition = edition;
                                            }
                                        }
                                        SpectralId::Wraith => {
                                            // 生成 Rare Joker，金幣歸零
                                            if state.jokers.len()
                                                < state.effective_joker_slot_limit()
                                            {
                                                let joker_id = JokerId::random_rare(&mut state.rng);
                                                state.jokers.push(JokerSlot::new(joker_id));
                                            }
                                            state.money = 0;
                                        }
                                        SpectralId::Sigil => {
                                            // 轉換所有手牌為隨機同一花色
                                            let suit = state.rng.gen_range(0..4);
                                            for card in &mut state.hand {
                                                card.suit = suit;
                                            }
                                        }
                                        SpectralId::Ouija => {
                                            // 轉換所有手牌為隨機同一點數，-1 手牌大小
                                            let rank = state.rng.gen_range(1..=13);
                                            for card in &mut state.hand {
                                                card.rank = rank;
                                            }
                                            state.hand_size_modifier -= 1;
                                        }
                                        SpectralId::Ectoplasm => {
                                            // 加 Negative 到隨機 Joker，-1 手牌大小
                                            let non_negative_jokers: Vec<usize> = state
                                                .jokers
                                                .iter()
                                                .enumerate()
                                                .filter(|(_, j)| j.enabled && !j.is_negative)
                                                .map(|(i, _)| i)
                                                .collect();
                                            if !non_negative_jokers.is_empty() {
                                                let idx = non_negative_jokers[state
                                                    .rng
                                                    .gen_range(0..non_negative_jokers.len())];
                                                state.jokers[idx].is_negative = true;
                                            }
                                            state.hand_size_modifier -= 1;
                                        }
                                        SpectralId::Immolate => {
                                            // 銷毀選中的牌（最多5張），得 $20
                                            let to_remove = selected_indices
                                                .iter()
                                                .take(5)
                                                .copied()
                                                .collect::<Vec<_>>();
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
                                            // 複製隨機 Joker，銷毀其他
                                            let enabled_jokers: Vec<usize> = state
                                                .jokers
                                                .iter()
                                                .enumerate()
                                                .filter(|(_, j)| j.enabled)
                                                .map(|(i, _)| i)
                                                .collect();
                                            if !enabled_jokers.is_empty() {
                                                let keep_idx = enabled_jokers
                                                    [state.rng.gen_range(0..enabled_jokers.len())];
                                                let kept = state.jokers[keep_idx].clone();
                                                state.jokers.clear();
                                                state.jokers.push(kept.clone());
                                                state.jokers.push(kept);
                                            }
                                        }
                                        SpectralId::DejaVu => {
                                            // 加 Red Seal 到選中牌
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].seal = Seal::Red;
                                            }
                                        }
                                        SpectralId::Hex => {
                                            // 加 Polychrome 到隨機 Joker，銷毀其他
                                            let enabled_jokers: Vec<usize> = state
                                                .jokers
                                                .iter()
                                                .enumerate()
                                                .filter(|(_, j)| j.enabled)
                                                .map(|(i, _)| i)
                                                .collect();
                                            if !enabled_jokers.is_empty() {
                                                let keep_idx = enabled_jokers
                                                    [state.rng.gen_range(0..enabled_jokers.len())];
                                                let mut kept = state.jokers[keep_idx].clone();
                                                kept.edition = Edition::Polychrome;
                                                state.jokers.clear();
                                                state.jokers.push(kept);
                                            }
                                        }
                                        SpectralId::Trance => {
                                            // 加 Blue Seal 到選中牌
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].seal = Seal::Blue;
                                            }
                                        }
                                        SpectralId::Medium => {
                                            // 加 Purple Seal 到選中牌
                                            if let Some(&idx) = selected_indices.first() {
                                                state.hand[idx].seal = Seal::Purple;
                                            }
                                        }
                                        SpectralId::Cryptid => {
                                            // 複製 1 張選中牌到牌組（2 張複製）
                                            if let Some(&idx) = selected_indices.first() {
                                                let card = state.hand[idx];
                                                state.deck.push(card);
                                                state.deck.push(card);
                                            }
                                        }
                                        SpectralId::TheSoul => {
                                            // 生成 Legendary Joker
                                            if state.jokers.len()
                                                < state.effective_joker_slot_limit()
                                            {
                                                let joker_id =
                                                    JokerId::random_legendary(&mut state.rng);
                                                state.jokers.push(JokerSlot::new(joker_id));
                                            }
                                        }
                                    }
                                    // 更新 last_used_consumable
                                    state.last_used_consumable = Some(consumable.clone());
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }

            Stage::PostBlind => {
                if action_type == ACTION_TYPE_CASH_OUT {
                    state.money += state.reward;
                    state.reward = 0;

                    // ReservedParking: 手中人頭牌 1/2 機率 +$1 (回合結束)
                    let face_cards_in_hand = state.hand.iter().filter(|c| c.is_face()).count();
                    let reserved_parking_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::ReservedParking)
                        .count();
                    // 為每個 ReservedParking，每張人頭牌 50% 機率 +$1
                    let total_rolls = face_cards_in_hand * reserved_parking_count;
                    for _ in 0..total_rolls {
                        if state.rng.gen_bool(0.5) {
                            state.money += 1;
                        }
                    }

                    // Egg: 每輪 +$3 售價
                    for joker in state.jokers.iter_mut() {
                        if joker.enabled && joker.id == JokerId::Egg {
                            joker.sell_value += 3;
                        }
                    }

                    // GiftCard: 每輪結束每個 Joker +$1 售價
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

                    // Rocket: 每輪結束 +rocket_money 金幣
                    let rocket_money: i64 = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Rocket)
                        .map(|j| j.get_rocket_money() as i64)
                        .sum();
                    state.money += rocket_money;

                    // Satellite: 每用過的 Planet +$1
                    let satellite_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Satellite)
                        .count() as i64;
                    state.money += satellite_count * state.planets_used_this_run as i64;

                    // Certificate: 每張手中 Gold Seal 牌 +$1
                    let certificate_count = state
                        .jokers
                        .iter()
                        .filter(|j| j.enabled && j.id == JokerId::Certificate)
                        .count() as i64;
                    let gold_seal_count =
                        state.hand.iter().filter(|c| c.seal == Seal::Gold).count() as i64;
                    state.money += certificate_count * gold_seal_count;

                    // CloudNine: 每張牌組中的 9 +$1
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

                    // Golden_Ticket: 牌組中每張 Gold 增強牌 +$3
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

                    // Delayed: 如果本輪沒有使用棄牌 +$2
                    if state.discards_used_this_blind == 0 {
                        let delayed_count = state
                            .jokers
                            .iter()
                            .filter(|j| j.enabled && j.id == JokerId::Delayed)
                            .count() as i64;
                        state.money += delayed_count * 2;
                    }

                    // Blue Seal: 手中持有 Blue Seal 牌時，生成對應最後打出牌型的 Planet 卡
                    let blue_seal_count =
                        state.hand.iter().filter(|c| c.seal == Seal::Blue).count();
                    if blue_seal_count > 0 {
                        // 使用最後打出的牌型（如果有）
                        if let Some(&last_hand_type) = state.played_hand_types.last() {
                            if let Some(planet_id) = PlanetId::from_hand_type_index(last_hand_type)
                            {
                                for _ in 0..blue_seal_count {
                                    if !state.consumables.is_full() {
                                        state.consumables.add(Consumable::Planet(planet_id));
                                    }
                                }
                            }
                        }
                    }

                    // 使用 trigger 系統處理 RoundEnded 事件
                    // 處理: Gros_Michel 自毀、GreenJoker 重置、Popcorn 減少等
                    let trigger_ctx = TriggerContext {
                        rng_value: state.rng.gen(),
                        ..Default::default()
                    };
                    let trigger_result = trigger_joker_slot_events(
                        GameEvent::RoundEnded,
                        &mut state.jokers,
                        &trigger_ctx,
                    );

                    // 處理自毀的 Joker
                    for &idx in &trigger_result.jokers_to_destroy {
                        if idx < state.jokers.len() {
                            state.jokers[idx].enabled = false;
                        }
                    }

                    // InvisibleJoker: 每回合計數 +1，達到 2 時賣出可複製隨機 Joker
                    for joker in &mut state.jokers {
                        if joker.enabled && joker.id == JokerId::InvisibleJoker {
                            joker.counter += 1;
                        }
                    }

                    state.stage = Stage::Shop;
                    state.refresh_shop();

                    // === Tag 效果觸發 ===

                    // TopUpTag: 填滿 Common Joker（最多 2 個）
                    let topup_count = state
                        .tags
                        .iter()
                        .filter(|t| !t.used && t.id == TagId::TopUpTag)
                        .count();
                    if topup_count > 0 {
                        let common_jokers = JokerId::by_rarity(1);
                        let effective_slots = state.effective_joker_slot_limit();
                        let mut added = 0;
                        for _ in 0..topup_count {
                            for _ in 0..2 {
                                if state.jokers.len() < effective_slots
                                    && !common_jokers.is_empty()
                                    && added < 2
                                {
                                    let idx = state.rng.gen_range(0..common_jokers.len());
                                    state.jokers.push(JokerSlot::new(common_jokers[idx]));
                                    added += 1;
                                }
                            }
                        }
                        // 標記 TopUpTag 為已使用
                        for tag in &mut state.tags {
                            if !tag.used && tag.id == TagId::TopUpTag {
                                tag.used = true;
                            }
                        }
                    }

                    // CharmTag: 獲得免費 Mega Arcana Pack（產生 2 張 Tarot 卡）
                    let charm_count = state
                        .tags
                        .iter()
                        .filter(|t| !t.used && t.id == TagId::CharmTag)
                        .count();
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

                    // MeteorTag: 獲得免費 Mega Celestial Pack（產生 2 張 Planet 卡）
                    let meteor_count = state
                        .tags
                        .iter()
                        .filter(|t| !t.used && t.id == TagId::MeteorTag)
                        .count();
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

                    // EtherealTag: 獲得免費 Spectral Pack（產生 1 張 Spectral 卡）
                    let ethereal_count = state
                        .tags
                        .iter()
                        .filter(|t| !t.used && t.id == TagId::EtherealTag)
                        .count();
                    for _ in 0..ethereal_count {
                        if !state.consumables.is_full() {
                            let all_spectrals = SpectralId::all();
                            let idx = state.rng.gen_range(0..all_spectrals.len());
                            state
                                .consumables
                                .add(Consumable::Spectral(all_spectrals[idx]));
                        }
                    }
                    for tag in &mut state.tags {
                        if !tag.used && tag.id == TagId::EtherealTag {
                            tag.used = true;
                        }
                    }

                    // BuffoonTag: 獲得免費 Mega Buffoon Pack（產生 1 個 Joker）
                    let buffoon_count = state
                        .tags
                        .iter()
                        .filter(|t| !t.used && t.id == TagId::BuffoonTag)
                        .count();
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

                    // StandardTag: 獲得免費 Mega Standard Pack（將 3 張牌加入牌組）
                    let standard_count = state
                        .tags
                        .iter()
                        .filter(|t| !t.used && t.id == TagId::StandardTag)
                        .count();
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

                    // === End Tag Effects ===

                    // Perkeo: 進入商店時，為隨機消耗品生成 Negative 複製
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
                            // Negative 複製不受槽位限制，直接添加
                            state.consumables.items.push(copy);
                        }
                    }
                }
            }

            Stage::Shop => {
                match action_type {
                    ACTION_TYPE_BUY_JOKER => {
                        let index = action_id as usize;
                        if let Some(item) = state.shop.items.get(index) {
                            if state.can_afford(item.cost)
                                && state.jokers.len() < state.effective_joker_slot_limit()
                            {
                                let cost = item.cost;
                                action_cost = cost;
                                state.money -= cost;
                                if let Some(bought) = state.shop.buy(index) {
                                    let mut joker = bought.joker;
                                    // ToDoList: 購買時隨機設置目標牌型
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
                                // 成功進入下一個 Ante（或無盡模式繼續）
                                state.blind_type = None;
                                state.stage = Stage::PreBlind;
                                state.round += 1;

                                // AnaglyphDeck: 打敗 Boss Blind 後獲得 Double Tag
                                if state.deck_type == DeckType::Anaglyph {
                                    state.tags.push(Tag::new(TagId::DoubleTag));
                                }

                                // Rocket: 過 Boss Blind 後，每回合獎勵 +$1
                                state.update_jokers(JokerId::Rocket, |j| {
                                    j.increment_rocket_money();
                                });
                            } else {
                                // 遊戲勝利（非無盡模式）
                                state.stage = Stage::End(GameEnd::Win);
                                done = true;
                            }
                        } else {
                            state.stage = Stage::PreBlind;
                            state.round += 1;
                        }

                        // Perishable: 回合結束時遞減計數器並移除到期的 Joker
                        for joker in &mut state.jokers {
                            if joker.is_perishable {
                                joker.perishable_rounds -= 1;
                            }
                        }
                        // 移除 perishable_rounds <= 0 的 Joker
                        state
                            .jokers
                            .retain(|j| !j.is_perishable || j.perishable_rounds > 0);

                        // Rental: 回合結束時支付 $3 租金，付不起則銷毀
                        let rental_cost = 3i64;
                        let rental_count =
                            state.jokers.iter().filter(|j| j.is_rental).count() as i64;
                        let total_rental_cost = rental_count * rental_cost;
                        if state.money >= total_rental_cost {
                            // 付得起全部租金
                            state.money -= total_rental_cost;
                        } else {
                            // 付不起，依次移除 Rental Joker 直到付得起
                            while state.jokers.iter().filter(|j| j.is_rental).count() > 0 {
                                let current_rental =
                                    state.jokers.iter().filter(|j| j.is_rental).count() as i64;
                                let current_cost = current_rental * rental_cost;
                                if state.money >= current_cost {
                                    state.money -= current_cost;
                                    break;
                                }
                                // 移除第一個 Rental Joker
                                if let Some(idx) = state.jokers.iter().position(|j| j.is_rental) {
                                    state.jokers.remove(idx);
                                }
                            }
                        }
                    }

                    ACTION_TYPE_REROLL => {
                        let mut reroll_cost = state.shop.current_reroll_cost();

                        // ChaosTheClown: 每回合 1 次免費 reroll
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
                            action_cost = reroll_cost;
                            state.money -= reroll_cost;
                            state.reroll_shop();

                            // 標記 ChaosTheClown 的免費 reroll 已使用
                            if chaos_free_reroll {
                                state.update_first_joker(JokerId::ChaosTheClown, |j| {
                                    j.use_chaos_free_reroll();
                                });
                            }

                            // 更新全局計數器（用於 ScoringContext）
                            state.rerolls_this_run += 1;

                            // FlashCard: 額外的 per-joker 追蹤
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

                            // DietCola: 賣出時獲得免費 Double Tag
                            if sold_joker.id == JokerId::DietCola {
                                state.tags.push(Tag::new(TagId::DoubleTag));
                            }

                            // InvisibleJoker: counter >= 2 時賣出可複製隨機 Joker
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
                                    let target_idx = enabled_jokers
                                        [state.rng.gen_range(0..enabled_jokers.len())];
                                    let copied = state.jokers[target_idx].clone();
                                    state.jokers.push(copied);
                                }
                            }

                            // Luchador: 賣出時禁用當前 Boss Blind 效果
                            if sold_joker.id == JokerId::Luchador {
                                // 清除 Boss Blind（等同於 Chicot 效果但只持續到回合結束）
                                // 實作方式：暫時清除 boss_blind
                                state.boss_blind = None;
                            }

                            state.money += sell_value;

                            // 使用 trigger 系統處理 JokerSold 事件
                            // Campfire: 每賣一張卡 +0.25 X Mult
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
                            // 根據消耗品類型更新狀態
                            match &consumable {
                                Consumable::Planet(planet_id) => {
                                    // 升級對應的牌型等級
                                    let hand_type_idx = planet_id.hand_type_index();
                                    state.hand_levels.upgrade(hand_type_idx);

                                    // Observatory: 標記此牌型已使用 Planet
                                    state.planet_used_hand_types |= 1 << hand_type_idx;

                                    // 使用 trigger 系統處理 PlanetUsed 事件
                                    // Constellation: 每使用 Planet 卡 +0.1 X Mult
                                    let trigger_ctx = TriggerContext::default();
                                    let _trigger_result = trigger_joker_slot_events(
                                        GameEvent::PlanetUsed,
                                        &mut state.jokers,
                                        &trigger_ctx,
                                    );
                                    // Satellite: 追蹤使用的 Planet 數量
                                    state.planets_used_this_run += 1;
                                    // 更新 last_used_consumable
                                    state.last_used_consumable = Some(consumable.clone());
                                }
                                Consumable::Tarot(tarot_id) => {
                                    // FortuneTeller: 使用 ctx.tarots_used_this_run 計分
                                    state.tarots_used_this_run += 1;
                                    // TheFool: 複製上一張消耗品
                                    if *tarot_id == TarotId::TheFool {
                                        if let Some(last) = state.last_used_consumable.clone() {
                                            if !state.consumables.is_full() {
                                                state.consumables.add(last);
                                            }
                                        }
                                    } else {
                                        // 更新 last_used_consumable（TheFool 除外）
                                        state.last_used_consumable = Some(consumable.clone());
                                    }
                                }
                                Consumable::Spectral(spectral_id) => {
                                    // 處理 Spectral 效果
                                    match spectral_id {
                                        SpectralId::BlackHole => {
                                            // 所有牌型等級 +1
                                            state.hand_levels.upgrade_all();
                                        }
                                        _ => {
                                            // Shop 階段其他 Spectral 無法使用（需要選擇手牌）
                                        }
                                    }
                                    // 更新 last_used_consumable
                                    state.last_used_consumable = Some(consumable.clone());
                                }
                            }
                        }
                    }

                    ACTION_TYPE_BUY_VOUCHER => {
                        if let Some(voucher_id) = state.shop_voucher {
                            let cost = voucher_id.cost();
                            if state.can_afford(cost) {
                                action_cost = cost;
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
                                action_cost = cost;
                                state.money -= cost;

                                // Hallucination (#173): 開包時 1/2 機率生成 Tarot 卡
                                if state.has_joker(JokerId::Hallucination) && state.rng.gen_range(0..2) == 0 {
                                    let tarot_id = TarotId::from_index(state.rng.gen_range(0..22));
                                    if let Some(tarot) = tarot_id {
                                        state.consumables.add(Consumable::Tarot(tarot));
                                    }
                                }

                                // 生成卡包內容並自動選擇
                                let full_pack_type = pack.pack_type.to_pack_type();
                                let contents =
                                    PackContents::generate(full_pack_type, &mut state.rng);
                                let pick_count = full_pack_type.pick_count();
                                let effective_joker_slots = state.effective_joker_slot_limit();

                                // 自動選擇前 N 個項目
                                for item in contents.items.into_iter().take(pick_count) {
                                    match item {
                                        PackItem::Tarot(tarot_id) => {
                                            state.consumables.add(Consumable::Tarot(tarot_id));
                                        }
                                        PackItem::Planet(planet_id) => {
                                            state.consumables.add(Consumable::Planet(planet_id));
                                        }
                                        PackItem::Spectral(spectral_id) => {
                                            state
                                                .consumables
                                                .add(Consumable::Spectral(spectral_id));
                                        }
                                        PackItem::Joker(joker_id, edition) => {
                                            if state.jokers.len() < effective_joker_slots {
                                                let mut new_joker = JokerSlot::new(joker_id);
                                                new_joker.edition = edition;
                                                state.jokers.push(new_joker);
                                            }
                                        }
                                        PackItem::PlayingCard(card) => {
                                            // 將撲克牌加入牌組
                                            state.deck.push(card.clone());
                                            // Hologram: 每加牌到牌組 +0.25 X Mult
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

            Stage::End(_) => {
                done = true;
            }
        }

        state.episode_step += 1;
        if state.episode_step >= MAX_STEPS {
            state.stage = Stage::End(GameEnd::Lose);
            done = true;
        }

        // 計算 delta
        let score_delta = state.score - score_before;
        let money_delta = state.money - money_before;

        // 計算遊戲結束狀態
        let game_end = match state.stage {
            Stage::End(GameEnd::Win) => 1,
            Stage::End(GameEnd::Lose) => 2,
            _ => 0,
        };

        let observation = Observation {
            features: Some(observation_from_state(&state)),
            action_mask: Some(action_mask_from_state(&state, done)),
        };

        let info = EnvInfo {
            // 基本狀態
            episode_step: state.episode_step,
            chips: state.score,
            mult: 1,
            blind_target: state.required_score(),

            // 擴展狀態
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

            // 事件追蹤
            score_delta,
            money_delta: money_delta as i32,
            last_action_type: action_type,
            last_action_cost: action_cost as i32,

            // Joker 狀態
            joker_count: state.jokers.len() as i32,
            joker_slot_limit: state.effective_joker_slot_limit() as i32,

            // 遊戲結束狀態
            game_end,
            blind_cleared,

            // 動作細節
            cards_played,
            cards_discarded,
            hand_type: hand_type_id,
        };

        Ok(Response::new(StepResponse {
            observation: Some(observation),
            reward: reward as f32,
            done,
            info: Some(info),
        }))
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    let env = EnvService::default();

    println!("JokerEnv gRPC server listening on {}", addr);
    println!("Full game flow enabled: PreBlind -> Blind -> PostBlind -> Shop -> ...");

    tonic::transport::Server::builder()
        .add_service(JokerEnvServer::new(env))
        .serve(addr)
        .await?;

    Ok(())
}
