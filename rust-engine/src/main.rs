use std::sync::Mutex;

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
    Stage, GameEnd, BlindType, BossBlind,
    hand_potential,
    joker_buy_reward, play_reward, blind_clear_reward,
    ante_progress_reward, game_end_reward, money_reward,
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
            episode_step: state.episode_step,
            chips: state.score,
            mult: 1,
            blind_target: state.required_score(),
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

        let mut reward = 0.0;
        let mut done = false;

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
                        state.score = 0;
                        state.played_hand_types.clear();
                        state.first_hand_type = None;
                        state.deal();

                        if state.boss_blind == Some(BossBlind::TheHook) {
                            state.apply_hook_discard();
                        }
                    }

                    ACTION_TYPE_SKIP_BLIND => {
                        // 跳過 Blind 並獲得 Tag
                        if let Some(_tag) = state.skip_blind() {
                            // Tag 獎勵已在 skip_blind 中處理
                            // 小獎勵鼓勵跳過（節省時間但犧牲金錢）
                            reward += 0.05;
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
                                    &mut state.rng,
                                );
                                let score_gained = score_result.score;
                                let hand_id = score_result.hand_id;
                                let hand_type_idx = hand_id.to_index();

                                let eye_ok = !state.boss_blind
                                    .map(|b| matches!(b, BossBlind::TheEye) && state.played_hand_types.contains(&hand_type_idx))
                                    .unwrap_or(false);

                                let mouth_ok = !state.boss_blind
                                    .map(|b| matches!(b, BossBlind::TheMouth) &&
                                         state.first_hand_type.is_some() &&
                                         state.first_hand_type != Some(hand_type_idx))
                                    .unwrap_or(false);

                                if !eye_ok || !mouth_ok {
                                    reward -= 0.1;
                                }

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
                                reward += play_reward(score_gained, required);

                                if state.score >= required {
                                    let blind = state.blind_type.unwrap_or(BlindType::Small);
                                    reward += blind_clear_reward(state.plays_left, blind, state.boss_blind);
                                    state.reward = state.calc_reward();
                                    state.stage = Stage::PostBlind;
                                } else if state.plays_left == 0 {
                                    state.stage = Stage::End(GameEnd::Lose);
                                    reward += game_end_reward(GameEnd::Lose, state.ante);
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
                            let old_potential = hand_potential(&state.hand);
                            let mask = state.selected_mask;

                            let _purple_count = state.discard_with_seals(mask);

                            let new_potential = hand_potential(&state.hand);
                            reward += (new_potential - old_potential).clamp(-0.3, 0.5);
                            state.discards_left -= 1;
                            state.selected_mask = 0;
                        }
                    }

                    ACTION_TYPE_USE_CONSUMABLE => {
                        let index = action_id as usize;
                        if let Some(_consumable) = state.consumables.use_item(index) {
                            // TODO: 實作消耗品效果
                            // 根據消耗品類型執行不同效果
                            reward += 0.1;
                        }
                    }

                    _ => {}
                }
            }

            Stage::PostBlind => {
                if action_type == ACTION_TYPE_CASH_OUT {
                    state.money += state.reward;
                    state.reward = 0;
                    state.stage = Stage::Shop;

                    reward += money_reward(state.money, state.ante);
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
                                let old_jokers = state.jokers.clone();
                                let money_before = state.money;
                                let cost = item.cost;

                                state.money -= cost;
                                if let Some(bought) = state.shop.buy(index) {
                                    state.jokers.push(bought.joker);
                                    reward += joker_buy_reward(&old_jokers, &state.jokers, cost, money_before);
                                }
                            }
                        }
                    }

                    ACTION_TYPE_NEXT_ROUND => {
                        let current_blind = state.blind_type.unwrap_or(BlindType::Small);

                        if current_blind == BlindType::Boss {
                            let old_ante = state.ante;
                            if state.advance_ante() {
                                // 成功進入下一個 Ante（或無盡模式繼續）
                                reward += ante_progress_reward(old_ante, state.ante);
                                state.blind_type = None;
                                state.stage = Stage::PreBlind;
                                state.round += 1;
                            } else {
                                // 遊戲勝利（非無盡模式）
                                state.stage = Stage::End(GameEnd::Win);
                                reward += game_end_reward(GameEnd::Win, state.ante);
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
                            state.money -= reroll_cost;
                            state.reroll_shop();
                            // 小懲罰以鼓勵謹慎使用 reroll
                            reward -= 0.05;
                        }
                    }

                    ACTION_TYPE_SELL_JOKER => {
                        let index = action_id as usize;
                        if index < state.jokers.len() {
                            let joker = state.jokers.remove(index);
                            let sell_value = joker.sell_value;
                            state.money += sell_value;
                            // 賣出 Joker 通常是負面的，除非組合不佳
                            reward -= 0.1;
                        }
                    }

                    ACTION_TYPE_USE_CONSUMABLE => {
                        let index = action_id as usize;
                        if let Some(_consumable) = state.consumables.use_item(index) {
                            // TODO: 實作消耗品效果
                            // 根據消耗品類型執行不同效果
                            reward += 0.1; // 基礎使用獎勵
                        }
                    }

                    ACTION_TYPE_BUY_VOUCHER => {
                        if let Some(voucher_id) = state.shop_voucher {
                            let cost = voucher_id.cost();
                            if cost <= state.money {
                                state.money -= cost;
                                state.voucher_effects.buy(voucher_id);
                                state.shop_voucher = None;
                                // Voucher 提供永久加成，給予獎勵
                                reward += 0.5;
                            }
                        }
                    }

                    ACTION_TYPE_BUY_PACK => {
                        let index = action_id as usize;
                        if let Some(pack) = state.shop_packs.get(index) {
                            if pack.cost <= state.money {
                                let cost = pack.cost;
                                state.money -= cost;
                                // TODO: 實作卡包開啟邏輯
                                state.shop_packs.remove(index);
                                reward += 0.1;
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

        let observation = Observation {
            features: Some(observation_from_state(&state)),
            action_mask: Some(action_mask_from_state(&state, done)),
        };

        let info = EnvInfo {
            episode_step: state.episode_step,
            chips: state.score,
            mult: 1,
            blind_target: state.required_score(),
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
