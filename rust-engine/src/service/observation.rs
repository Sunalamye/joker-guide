//! Observation 構建
//!
//! 構建完整的遊戲狀態觀察向量，包含：
//! - 標量特徵（分數、金幣、回合等）
//! - 手牌選擇狀態
//! - 手牌特徵
//! - 牌型 one-hot
//! - 牌組剩餘計數
//! - Joker 特徵（ID one-hot + 狀態）
//! - 商店特徵
//! - Boss Blind one-hot
//! - Deck 類型 one-hot
//! - Stake 類型 one-hot
//! - Voucher 擁有狀態
//! - 消耗品槽位
//! - Tag 計數

use joker_env::proto::Tensor;

use super::scoring::build_selected_hand;
use super::state::EnvState;
use crate::game::{
    card_index, score_hand, HandId, Stage, BOSS_BLIND_COUNT, CARD_FEATURES, CONSUMABLE_FEATURES,
    CONSUMABLE_SLOT_COUNT, DECK_FEATURES, DECK_TYPE_FEATURES, DISCARDS_PER_BLIND, HAND_SIZE,
    HAND_TYPE_COUNT, JOKER_ID_SIZE, JOKER_SINGLE_FEATURES, JOKER_SLOTS, MAX_STEPS, OBS_SIZE,
    PLAYS_PER_BLIND, SCALAR_COUNT, SHOP_JOKER_COUNT, STAKE_FEATURES, TAG_FEATURES,
    VOUCHER_FEATURES,
};

/// 從遊戲狀態構建 observation tensor
pub fn observation_from_state(state: &EnvState) -> Tensor {
    let mut data = Vec::with_capacity(OBS_SIZE as usize);

    let required = state.required_score().max(1) as f32;

    // ============================================================================
    // Scalars (32)
    // ============================================================================
    data.push(state.score as f32 / required); // 0: 分數進度
    data.push(state.ante.to_int() as f32 / 8.0); // 1: Ante 進度
    data.push(state.blind_type.map(|b| b.to_int()).unwrap_or(-1) as f32 / 2.0); // 2: Blind 類型
    data.push(
        match state.stage {
            // 3: 遊戲階段
            Stage::PreBlind => 0.0,
            Stage::Blind => 1.0,
            Stage::PostBlind => 2.0,
            Stage::Shop => 3.0,
            Stage::End(_) => 4.0,
        } / 4.0,
    );
    data.push(state.plays_left as f32 / PLAYS_PER_BLIND as f32); // 4: 剩餘出牌
    data.push(state.discards_left as f32 / DISCARDS_PER_BLIND as f32); // 5: 剩餘棄牌
    data.push(state.money as f32 / 100.0); // 6: 金幣
    data.push(state.reward as f32 / 20.0); // 7: 待領獎勵
    data.push(state.deck.len() as f32 / 52.0); // 8: 牌組剩餘比例
    data.push(state.jokers.len() as f32 / state.joker_slot_limit as f32); // 9: Joker 使用率
    data.push(state.round as f32 / 24.0); // 10: 回合進度
    data.push(state.episode_step as f32 / MAX_STEPS as f32); // 11: 步數進度
    data.push(
        state
            .boss_blind
            .map(|b| b.to_int() as f32 / BOSS_BLIND_COUNT as f32)
            .unwrap_or(-0.1),
    ); // 12: Boss Blind
    data.push(state.consumables.items.len() as f32 / state.consumables.capacity as f32); // 13: 消耗品使用率
    data.push(state.voucher_effects.owned.len() as f32 / VOUCHER_FEATURES as f32); // 14: Voucher 進度
                                                                                   // 15-27: 13 種牌型等級
    for hand_type in 0..HAND_TYPE_COUNT {
        data.push(state.hand_levels.get(hand_type) as f32 / 10.0);
    }
    data.push(state.tags.len() as f32 / 10.0); // 28: Tag 數量
    data.push(if state.endless_mode { 1.0 } else { 0.0 }); // 29: 無盡模式
    data.push(state.endless_ante as f32 / 10.0); // 30: 無盡模式額外 Ante
    data.push(state.shop.current_reroll_cost() as f32 / 20.0); // 31: 當前 Reroll 費用

    assert_eq!(data.len(), SCALAR_COUNT, "Scalar count mismatch");

    // ============================================================================
    // Selection mask (8)
    // ============================================================================
    for idx in 0..HAND_SIZE {
        let selected = ((state.selected_mask >> idx) & 1) == 1;
        data.push(if selected { 1.0 } else { 0.0 });
    }

    // ============================================================================
    // Hand features (8 * 21 = 168)
    // ============================================================================
    for idx in 0..HAND_SIZE {
        if let Some(card) = state.hand.get(idx) {
            // Rank one-hot (13)
            for r in 0..13 {
                data.push(if r == (card.rank - 1) as usize {
                    1.0
                } else {
                    0.0
                });
            }
            // Suit one-hot (4)
            for s in 0..4 {
                data.push(if s == card.suit as usize { 1.0 } else { 0.0 });
            }
            // Enhancement, Seal, Edition, Face down (4)
            data.push(card.enhancement.to_int() as f32 / 8.0);
            data.push(card.seal.to_int() as f32 / 4.0);
            data.push(card.edition.to_int() as f32 / 4.0);
            data.push(if card.face_down { 1.0 } else { 0.0 });
        } else {
            data.extend(std::iter::repeat(0.0).take(CARD_FEATURES));
        }
    }

    // ============================================================================
    // Hand type one-hot (13)
    // ============================================================================
    let selected_hand = build_selected_hand(&state.hand, state.selected_mask);
    let hand_id = if selected_hand.is_empty() {
        HandId::HighCard
    } else {
        score_hand(&selected_hand).id
    };
    let ht_index = hand_id.to_index();
    for idx in 0..HAND_TYPE_COUNT {
        data.push(if idx == ht_index { 1.0 } else { 0.0 });
    }

    // ============================================================================
    // Deck counts (52)
    // ============================================================================
    let mut deck_counts = [0.0f32; DECK_FEATURES];
    for card in &state.deck {
        let index = card_index(*card);
        deck_counts[index] += 1.0;
    }
    // Normalize by max possible count
    for count in &mut deck_counts {
        *count /= 4.0; // Max 4 of each card normally
    }
    data.extend(deck_counts);

    // ============================================================================
    // Joker features (5 * 153 = 765)
    // ID one-hot (150) + enabled (1) + eternal (1) + negative (1)
    // ============================================================================
    for slot in 0..JOKER_SLOTS {
        if let Some(joker) = state.jokers.get(slot) {
            // Joker ID one-hot (150)
            let joker_idx = joker.id.to_index();
            for i in 0..JOKER_ID_SIZE {
                data.push(if i == joker_idx { 1.0 } else { 0.0 });
            }
            // Enabled flag
            data.push(if joker.enabled { 1.0 } else { 0.0 });
            // Eternal flag
            data.push(if joker.is_eternal { 1.0 } else { 0.0 });
            // Negative flag
            data.push(if joker.is_negative { 1.0 } else { 0.0 });
        } else {
            // Empty slot: all zeros
            data.extend(std::iter::repeat(0.0).take(JOKER_SINGLE_FEATURES));
        }
    }

    // ============================================================================
    // Shop features (2 * 151 = 302)
    // ID one-hot (150) + cost (1)
    // ============================================================================
    for slot in 0..SHOP_JOKER_COUNT {
        if let Some(item) = state.shop.items.get(slot) {
            // Joker ID one-hot (150)
            let joker_idx = item.joker.id.to_index();
            for i in 0..JOKER_ID_SIZE {
                data.push(if i == joker_idx { 1.0 } else { 0.0 });
            }
            // Normalized cost
            data.push(item.cost as f32 / 20.0);
        } else {
            // Empty slot
            data.extend(std::iter::repeat(0.0).take(JOKER_ID_SIZE + 1));
        }
    }

    // ============================================================================
    // Boss Blind one-hot (27)
    // ============================================================================
    let boss_idx = state.boss_blind.map(|b| b.to_int() as usize);
    for i in 0..BOSS_BLIND_COUNT {
        data.push(if boss_idx == Some(i) { 1.0 } else { 0.0 });
    }

    // ============================================================================
    // Deck type one-hot (16)
    // ============================================================================
    let deck_idx = state.deck_type.to_index();
    for i in 0..DECK_TYPE_FEATURES {
        data.push(if i == deck_idx { 1.0 } else { 0.0 });
    }

    // ============================================================================
    // Stake one-hot (8)
    // ============================================================================
    let stake_idx = state.stake.to_index();
    for i in 0..STAKE_FEATURES {
        data.push(if i == stake_idx { 1.0 } else { 0.0 });
    }

    // ============================================================================
    // Voucher ownership flags (36)
    // ============================================================================
    for i in 0..VOUCHER_FEATURES {
        let owned = state
            .voucher_effects
            .owned
            .iter()
            .any(|v| v.to_index() == i);
        data.push(if owned { 1.0 } else { 0.0 });
    }

    // ============================================================================
    // Consumable slots (2 * 52 = 104)
    // Global index one-hot for each slot
    // ============================================================================
    for slot in 0..CONSUMABLE_SLOT_COUNT {
        if let Some(consumable) = state.consumables.items.get(slot) {
            let global_idx = consumable.to_global_index();
            for i in 0..CONSUMABLE_FEATURES {
                data.push(if i == global_idx { 1.0 } else { 0.0 });
            }
        } else {
            // Empty slot
            data.extend(std::iter::repeat(0.0).take(CONSUMABLE_FEATURES));
        }
    }

    // ============================================================================
    // Tag counts (25)
    // Count of each tag type
    // ============================================================================
    let mut tag_counts = [0.0f32; TAG_FEATURES];
    for tag in &state.tags {
        let idx = tag.id.to_index();
        if idx < TAG_FEATURES {
            tag_counts[idx] += 1.0;
        }
    }
    // Normalize
    for count in &mut tag_counts {
        *count = (*count).min(5.0) / 5.0;
    }
    data.extend(tag_counts);

    // ============================================================================
    // Verify size and return
    // ============================================================================
    // Pad if needed (should not be necessary if OBS_SIZE is correct)
    while data.len() < OBS_SIZE as usize {
        data.push(0.0);
    }

    // Truncate if too large (debug safety)
    if data.len() > OBS_SIZE as usize {
        eprintln!(
            "WARNING: Observation size {} exceeds OBS_SIZE {}, truncating",
            data.len(),
            OBS_SIZE
        );
        data.truncate(OBS_SIZE as usize);
    }

    Tensor {
        data,
        shape: vec![OBS_SIZE],
    }
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::state::EnvState;

    #[test]
    fn test_observation_size() {
        let state = EnvState::new(42);
        let obs = observation_from_state(&state);
        assert_eq!(obs.data.len(), OBS_SIZE as usize);
        assert_eq!(obs.shape, vec![OBS_SIZE]);
    }

    #[test]
    fn test_observation_with_jokers() {
        use crate::game::{JokerId, JokerSlot};

        let mut state = EnvState::new(42);
        state.jokers.push(JokerSlot::new(JokerId::Joker));
        state.jokers.push(JokerSlot::new(JokerId::GreedyJoker));

        let obs = observation_from_state(&state);
        assert_eq!(obs.data.len(), OBS_SIZE as usize);
    }

    #[test]
    fn test_observation_with_consumables() {
        use crate::game::{Consumable, TarotId};

        let mut state = EnvState::new(42);
        state.consumables.add(Consumable::Tarot(TarotId::TheFool));

        let obs = observation_from_state(&state);
        assert_eq!(obs.data.len(), OBS_SIZE as usize);
    }

    #[test]
    fn test_observation_with_vouchers() {
        use crate::game::VoucherId;

        let mut state = EnvState::new(42);
        state.voucher_effects.buy(VoucherId::Grabber);

        let obs = observation_from_state(&state);
        assert_eq!(obs.data.len(), OBS_SIZE as usize);

        // Check Grabber flag is set (index 6 in voucher section)
        // Voucher section starts after consumables
    }

    #[test]
    fn test_observation_with_config() {
        use crate::game::{DeckType, Stake};

        let state = EnvState::new_with_config(42, DeckType::Red, Stake::Gold);
        let obs = observation_from_state(&state);
        assert_eq!(obs.data.len(), OBS_SIZE as usize);

        // Verify deck_type and stake are encoded
        assert_eq!(state.deck_type, DeckType::Red);
        assert_eq!(state.stake, Stake::Gold);
    }
}
