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
    Stage, GameEnd, BlindType, BossBlind, JokerId, Card, Enhancement, Edition, Seal,
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

                        state.score = 0;
                        state.played_hand_types.clear();
                        state.first_hand_type = None;

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
                                let score_result = calculate_play_score(
                                    &selected,
                                    &jokers_clone,
                                    boss_blind,
                                    discards_remaining,
                                    state.rerolls_this_run,
                                    state.blinds_skipped,
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
                                state.money += score_result.money_gained;

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

                    state.stage = Stage::Shop;
                    state.refresh_shop();
                }
            }

            Stage::Shop => {
                match action_type {
                    ACTION_TYPE_BUY_JOKER => {
                        let index = action_id as usize;
                        if let Some(item) = state.shop.items.get(index) {
                            if item.cost <= state.money
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
                        if reroll_cost <= state.money {
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
                            if cost <= state.money {
                                action_cost = cost;
                                state.money -= cost;
                                state.voucher_effects.buy(voucher_id);
                                state.shop_voucher = None;
                            }
                        }
                    }

                    ACTION_TYPE_BUY_PACK => {
                        let index = action_id as usize;
                        if let Some(pack) = state.shop_packs.get(index) {
                            if pack.cost <= state.money {
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
