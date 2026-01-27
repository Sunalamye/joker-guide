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
    PLAYS_PER_BLIND, DISCARDS_PER_BLIND, MAX_SELECTED, HAND_SIZE, MAX_STEPS,
    OBS_SIZE, ACTION_MASK_SIZE,
    ACTION_TYPE_SELECT, ACTION_TYPE_PLAY, ACTION_TYPE_DISCARD, ACTION_TYPE_SELECT_BLIND,
    ACTION_TYPE_CASH_OUT, ACTION_TYPE_BUY_JOKER, ACTION_TYPE_NEXT_ROUND,
    ACTION_TYPE_REROLL, ACTION_TYPE_SELL_JOKER, ACTION_TYPE_SKIP_BLIND,
    ACTION_TYPE_USE_CONSUMABLE, ACTION_TYPE_BUY_VOUCHER, ACTION_TYPE_BUY_PACK,
    Stage, GameEnd, BlindType, BossBlind, JokerId, JokerSlot, Card, Enhancement, Edition, Seal,
    Consumable, TarotId, PlanetId, SpectralId,
};

// 從 service 模組導入
use service::{
    EnvState, observation_from_state, action_mask_from_state,
    build_selected_hand, calculate_play_score,
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
            stage: 0,  // PreBlind
            blind_type: -1,  // None
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
            joker_slot_limit: state.joker_slot_limit as i32,

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

        let reward = 0.0;  // 獎勵由 Python 端計算
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
                            state.plays_left = state.boss_blind
                                .and_then(|b| b.max_plays())
                                .unwrap_or(PLAYS_PER_BLIND);
                        } else {
                            state.boss_blind = None;
                            state.plays_left = PLAYS_PER_BLIND;
                        }

                        state.discards_left = DISCARDS_PER_BLIND;

                        // Drunkard: +1 棄牌次數每輪
                        let drunkard_count = state.jokers.iter()
                            .filter(|j| j.enabled && j.id == JokerId::Drunkard)
                            .count() as i32;
                        state.discards_left += drunkard_count;

                        // Troubadour: -1 出牌次數每輪
                        let troubadour_count = state.jokers.iter()
                            .filter(|j| j.enabled && j.id == JokerId::Troubadour)
                            .count() as i32;
                        state.plays_left = (state.plays_left - troubadour_count).max(1);

                        state.score = 0;
                        state.played_hand_types.clear();
                        state.first_hand_type = None;
                        state.discards_used_this_blind = 0;
                        state.hands_played_this_blind = 0;

                        // Hit The Road: 每 Blind 開始時重置 X Mult
                        for joker in &mut state.jokers {
                            if joker.enabled && joker.id == JokerId::Hit_The_Road {
                                joker.hit_the_road_mult = 1.0;
                            }
                        }

                        // MarbleJoker: 選擇 Blind 時加 Stone 卡到牌組
                        let marble_joker_count = state.jokers.iter()
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
                            };
                            state.deck.push(stone_card);
                        }

                        // RiffRaff: 選擇 Blind 時生成 2 個 Common Joker
                        let riff_raff_count = state.jokers.iter()
                            .filter(|j| j.enabled && j.id == JokerId::RiffRaff)
                            .count();
                        let common_jokers = JokerId::by_rarity(1);
                        for _ in 0..riff_raff_count {
                            for _ in 0..2 {
                                if state.jokers.len() < state.joker_slot_limit && !common_jokers.is_empty() {
                                    let idx = state.rng.gen_range(0..common_jokers.len());
                                    state.jokers.push(JokerSlot::new(common_jokers[idx]));
                                }
                            }
                        }

                        // AncientJoker: 每回合開始時隨機設置花色
                        let ancient_joker_random_suit: u8 = state.rng.gen_range(0..4);
                        for joker in &mut state.jokers {
                            if joker.enabled && joker.id == JokerId::AncientJoker {
                                joker.set_ancient_suit(ancient_joker_random_suit);
                            }
                        }

                        // Castle: 每回合開始時隨機設置花色
                        let castle_random_suit: u8 = state.rng.gen_range(0..4);
                        for joker in &mut state.jokers {
                            if joker.enabled && joker.id == JokerId::Castle {
                                joker.set_castle_suit(castle_random_suit);
                            }
                        }

                        state.deal();

                        if state.boss_blind == Some(BossBlind::TheHook) {
                            state.apply_hook_discard();
                        }
                    }

                    ACTION_TYPE_SKIP_BLIND => {
                        let _blind_type = state.blind_type.unwrap_or(BlindType::Small);
                        state.skip_blind();

                        // 更新全局計數器（用於 ScoringContext）
                        state.blinds_skipped += 1;

                        // RedCard: 額外的 per-joker 追蹤（可選）
                        for joker in &mut state.jokers {
                            if joker.enabled && joker.id == JokerId::RedCard {
                                joker.red_card_mult += 3;
                            }
                        }

                        // Cartomancer: 跳過 Blind 時生成隨機 Tarot 卡
                        let cartomancer_count = state.jokers.iter()
                            .filter(|j| j.enabled && j.id == JokerId::Cartomancer)
                            .count();
                        for _ in 0..cartomancer_count {
                            if !state.consumables.is_full() {
                                let all_tarots = TarotId::all();
                                let idx = state.rng.gen_range(0..all_tarots.len());
                                state.consumables.add(Consumable::Tarot(all_tarots[idx]));
                            }
                        }

                        // Astronomer: 跳過 Blind 時生成隨機 Planet 卡
                        let astronomer_count = state.jokers.iter()
                            .filter(|j| j.enabled && j.id == JokerId::Astronomer)
                            .count();
                        for _ in 0..astronomer_count {
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
                            let selected = build_selected_hand(&state.hand, state.selected_mask);
                            let selected_count = selected.len();
                            cards_played = selected_count as i32;

                            let psychic_ok = !state.boss_blind
                                .map(|b| b.requires_five_cards() && selected_count != 5)
                                .unwrap_or(false);

                            if psychic_ok {
                                let jokers_clone = state.jokers.clone();
                                let boss_blind = state.boss_blind;
                                let discards_remaining = state.discards_left;
                                // 計算增強牌數量 (DriversLicense)
                                let enhanced_cards_in_deck = state.deck.iter()
                                    .filter(|c| c.enhancement != Enhancement::None)
                                    .count() as i32;
                                // DNA: 是否是第一手牌；DuskJoker/Acrobat: 是否是最後一手牌
                                let is_first_hand = state.hands_played_this_blind == 0;
                                let is_final_hand = state.plays_left == 1;
                                // Selzer: 獲取剩餘重觸發次數
                                let selzer_charges = state.jokers.iter()
                                    .find(|j| j.enabled && j.id == JokerId::Selzer)
                                    .map(|j| j.selzer_charges)
                                    .unwrap_or(0);
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
                                    &mut state.rng,
                                );
                                let score_gained = score_result.score;
                                let hand_id = score_result.hand_id;
                                let hand_type_idx = hand_id.to_index();
                                hand_type_id = hand_type_idx as i32;

                                let eye_ok = !state.boss_blind
                                    .map(|b| matches!(b, BossBlind::TheEye) && state.played_hand_types.contains(&hand_type_idx))
                                    .unwrap_or(false);

                                let mouth_ok = !state.boss_blind
                                    .map(|b| matches!(b, BossBlind::TheMouth) &&
                                         state.first_hand_type.is_some() &&
                                         state.first_hand_type != Some(hand_type_idx))
                                    .unwrap_or(false);

                                // Boss Blind 限制檢查（Python 端計算獎勵懲罰）
                                let _violated_boss_rule = !eye_ok || !mouth_ok;

                                state.played_hand_types.push(hand_type_idx);
                                if state.first_hand_type.is_none() {
                                    state.first_hand_type = Some(hand_type_idx);
                                }

                                state.score += score_gained;
                                state.plays_left -= 1;
                                state.hands_played_this_blind += 1; // DNA: 追蹤第一手牌
                                state.money += score_result.money_gained;

                                // Selzer: 更新 charges 並在用完時銷毀
                                if score_result.selzer_charges_used > 0 {
                                    for joker in &mut state.jokers {
                                        if joker.enabled && joker.id == JokerId::Selzer {
                                            joker.selzer_charges -= score_result.selzer_charges_used;
                                            if joker.selzer_charges <= 0 {
                                                joker.enabled = false; // 用完自毀
                                            }
                                            break;
                                        }
                                    }
                                }

                                // SpaceJoker: 1/4 機率升級出過的牌型
                                let space_joker_count = state.jokers.iter()
                                    .filter(|j| j.enabled && j.id == JokerId::SpaceJoker)
                                    .count();
                                for _ in 0..space_joker_count {
                                    if state.rng.gen_range(0..4) == 0 {
                                        state.hand_levels.upgrade(hand_type_idx);
                                    }
                                }

                                // Vagabond: 出 ≤4 張牌時生成隨機 Tarot 卡
                                if selected_count <= 4 {
                                    let vagabond_count = state.jokers.iter()
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

                                // Seance: Straight Flush 或 Royal Flush 時生成 Spectral 卡
                                // StraightFlush = 8, RoyalFlush = 9
                                if hand_type_idx == 8 || hand_type_idx == 9 {
                                    let seance_count = state.jokers.iter()
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

                                let selected_mask = state.selected_mask;
                                state.break_glass_cards(selected_mask, &score_result.glass_to_break);

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
                            let selected_cards: Vec<Card> = state.hand.iter().enumerate()
                                .filter(|(i, _)| (mask >> i) & 1 == 1)
                                .map(|(_, c)| *c)
                                .collect();
                            let face_count = selected_cards.iter().filter(|c| c.is_face()).count();
                            let king_count = selected_cards.iter().filter(|c| c.rank == 13).count();
                            let has_face = face_count > 0;

                            cards_discarded = mask.count_ones() as i32;
                            let _purple_count = state.discard_with_seals(mask);
                            state.discards_left -= 1;
                            state.discards_used_this_blind += 1;
                            state.selected_mask = 0;

                            // 經濟類 Joker 觸發 - 先計算獎勵，避免借用衝突
                            let mut money_bonus = 0i64;
                            for joker in &mut state.jokers {
                                if !joker.enabled { continue; }
                                match joker.id {
                                    // Faceless: 棄 3+ 人頭牌時 +$5
                                    JokerId::Faceless => {
                                        if face_count >= 3 {
                                            money_bonus += 5;
                                        }
                                    }
                                    // TradingCard: 首次棄人頭牌時創建 Tarot
                                    JokerId::TradingCard => {
                                        if has_face && !joker.trading_card_triggered {
                                            joker.trading_card_triggered = true;
                                            // TODO: 創建隨機 Tarot 到消耗品欄位
                                        }
                                    }
                                    // MailInRebate: 棄 K 時 +$5
                                    JokerId::MailInRebate => {
                                        money_bonus += king_count as i64 * 5;
                                    }
                                    _ => {}
                                }
                            }
                            state.money += money_bonus;

                            // Castle: 每棄特定花色牌 +3 Chips (永久)
                            for card in &selected_cards {
                                for joker in &mut state.jokers {
                                    if joker.enabled && joker.id == JokerId::Castle {
                                        joker.update_castle_on_discard(card.suit);
                                    }
                                }
                            }

                            // Hit The Road: 每棄 Jack +0.5 X Mult
                            let jack_count = selected_cards.iter().filter(|c| c.rank == 11).count() as i32;
                            if jack_count > 0 {
                                for joker in &mut state.jokers {
                                    if joker.enabled {
                                        joker.update_hit_the_road_on_jack_discard(jack_count);
                                    }
                                }
                            }

                            // Sixth: 棄 6 張牌時銷毀自身並獲得 Spectral 卡
                            if cards_discarded == 6 {
                                if let Some(idx) = state.jokers.iter()
                                    .position(|j| j.enabled && j.id == JokerId::Sixth)
                                {
                                    state.jokers[idx].enabled = false;
                                    if !state.consumables.is_full() {
                                        let all_spectrals = SpectralId::all();
                                        let spec_idx = state.rng.gen_range(0..all_spectrals.len());
                                        state.consumables.add(Consumable::Spectral(all_spectrals[spec_idx]));
                                    }
                                }
                            }
                        }
                    }

                    ACTION_TYPE_USE_CONSUMABLE => {
                        let index = action_id as usize;
                        if let Some(_consumable) = state.consumables.use_item(index) {
                            // TODO: 實作消耗品效果
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
                    let face_cards_in_hand = state.hand.iter()
                        .filter(|c| c.is_face())
                        .count();
                    let reserved_parking_count = state.jokers.iter()
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

                    // Rocket: 每輪結束 +rocket_money 金幣
                    let rocket_money: i64 = state.jokers.iter()
                        .filter(|j| j.enabled && j.id == JokerId::Rocket)
                        .map(|j| j.rocket_money as i64)
                        .sum();
                    state.money += rocket_money;

                    // Satellite: 每用過的 Planet +$1
                    let satellite_count = state.jokers.iter()
                        .filter(|j| j.enabled && j.id == JokerId::Satellite)
                        .count() as i64;
                    state.money += satellite_count * state.planets_used_this_run as i64;

                    // Certificate: 每張手中 Gold Seal 牌 +$1
                    let certificate_count = state.jokers.iter()
                        .filter(|j| j.enabled && j.id == JokerId::Certificate)
                        .count() as i64;
                    let gold_seal_count = state.hand.iter()
                        .filter(|c| c.seal == Seal::Gold)
                        .count() as i64;
                    state.money += certificate_count * gold_seal_count;

                    // CloudNine: 每張牌組中的 9 +$1
                    let cloud_nine_count = state.jokers.iter()
                        .filter(|j| j.enabled && j.id == JokerId::CloudNine)
                        .count() as i64;
                    let nines_in_deck = state.deck.iter()
                        .chain(state.hand.iter())
                        .chain(state.discarded.iter())
                        .filter(|c| c.rank == 9)
                        .count() as i64;
                    state.money += cloud_nine_count * nines_in_deck;

                    // Golden_Ticket: 牌組中每張 Gold 增強牌 +$3
                    let golden_ticket_count = state.jokers.iter()
                        .filter(|j| j.enabled && j.id == JokerId::Golden_Ticket)
                        .count() as i64;
                    let gold_cards_in_full_deck = state.deck.iter()
                        .chain(state.hand.iter())
                        .chain(state.discarded.iter())
                        .filter(|c| c.enhancement == Enhancement::Gold)
                        .count() as i64;
                    state.money += golden_ticket_count * gold_cards_in_full_deck * 3;

                    // Delayed: 如果本輪沒有使用棄牌 +$2
                    if state.discards_used_this_blind == 0 {
                        let delayed_count = state.jokers.iter()
                            .filter(|j| j.enabled && j.id == JokerId::Delayed)
                            .count() as i64;
                        state.money += delayed_count * 2;
                    }

                    state.stage = Stage::Shop;
                    state.refresh_shop();

                    // Perkeo: 進入商店時，為隨機消耗品生成 Negative 複製
                    let perkeo_count = state.jokers.iter()
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
                        // CreditCard: 允許 $20 負債
                        let has_credit_card = state.jokers.iter()
                            .any(|j| j.enabled && j.id == JokerId::CreditCard);
                        let debt_limit = if has_credit_card { 20 } else { 0 };
                        if let Some(item) = state.shop.items.get(index) {
                            if item.cost <= state.money + debt_limit
                                && state.jokers.len() < state.joker_slot_limit
                            {
                                let cost = item.cost;
                                action_cost = cost;
                                state.money -= cost;
                                if let Some(bought) = state.shop.buy(index) {
                                    state.jokers.push(bought.joker);
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

                                // Rocket: 過 Boss Blind 後，每回合獎勵 +$1
                                for joker in state.jokers.iter_mut() {
                                    if joker.enabled && joker.id == JokerId::Rocket {
                                        joker.rocket_money += 1;
                                    }
                                }
                            } else {
                                // 遊戲勝利（非無盡模式）
                                state.stage = Stage::End(GameEnd::Win);
                                done = true;
                            }
                        } else {
                            state.stage = Stage::PreBlind;
                            state.round += 1;
                        }
                    }

                    ACTION_TYPE_REROLL => {
                        let reroll_cost = state.shop.current_reroll_cost();
                        // CreditCard: 允許 $20 負債
                        let has_credit_card = state.jokers.iter()
                            .any(|j| j.enabled && j.id == JokerId::CreditCard);
                        let debt_limit = if has_credit_card { 20 } else { 0 };
                        if reroll_cost <= state.money + debt_limit {
                            action_cost = reroll_cost;
                            state.money -= reroll_cost;
                            state.reroll_shop();

                            // 更新全局計數器（用於 ScoringContext）
                            state.rerolls_this_run += 1;

                            // FlashCard: 額外的 per-joker 追蹤（可選）
                            for joker in &mut state.jokers {
                                if joker.enabled && joker.id == JokerId::Flash {
                                    joker.flash_card_mult += 2;
                                }
                            }
                        }
                    }

                    ACTION_TYPE_SELL_JOKER => {
                        let index = action_id as usize;
                        if index < state.jokers.len() {
                            let sold_joker = state.jokers.remove(index);
                            let sell_value = sold_joker.sell_value;
                            state.money += sell_value;
                        }
                    }

                    ACTION_TYPE_USE_CONSUMABLE => {
                        let index = action_id as usize;
                        if let Some(_consumable) = state.consumables.use_item(index) {
                            // TODO: 實作消耗品效果
                        }
                    }

                    ACTION_TYPE_BUY_VOUCHER => {
                        if let Some(voucher_id) = state.shop_voucher {
                            let cost = voucher_id.cost();
                            // CreditCard: 允許 $20 負債
                            let has_credit_card = state.jokers.iter()
                                .any(|j| j.enabled && j.id == JokerId::CreditCard);
                            let debt_limit = if has_credit_card { 20 } else { 0 };
                            if cost <= state.money + debt_limit {
                                action_cost = cost;
                                state.money -= cost;
                                state.voucher_effects.buy(voucher_id);
                                state.shop_voucher = None;
                            }
                        }
                    }

                    ACTION_TYPE_BUY_PACK => {
                        let index = action_id as usize;
                        // CreditCard: 允許 $20 負債
                        let has_credit_card = state.jokers.iter()
                            .any(|j| j.enabled && j.id == JokerId::CreditCard);
                        let debt_limit = if has_credit_card { 20 } else { 0 };
                        if let Some(pack) = state.shop_packs.get(index) {
                            if pack.cost <= state.money + debt_limit {
                                let cost = pack.cost;
                                action_cost = cost;
                                state.money -= cost;
                                // TODO: 實作卡包開啟邏輯
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
            joker_slot_limit: state.joker_slot_limit as i32,

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
