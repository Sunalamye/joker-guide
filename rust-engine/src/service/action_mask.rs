//! Action Mask 構建

use joker_env::proto::Tensor;

use super::state::EnvState;
use crate::game::{
    BlindType, BossBlind, Stage, ACTION_MASK_SIZE, ACTION_TYPE_COUNT, CONSUMABLE_SLOT_COUNT,
    HAND_SIZE, JOKER_SLOTS, SHOP_JOKER_COUNT, SHOP_PACK_COUNT, SHOP_VOUCHER_COUNT,
};

/// 從遊戲狀態構建 action mask tensor
pub fn action_mask_from_state(state: &EnvState, done: bool) -> Tensor {
    let mut data = vec![0.0; ACTION_MASK_SIZE as usize];

    if done {
        return Tensor {
            data,
            shape: vec![ACTION_MASK_SIZE],
        };
    }

    let mut offset = 0;

    // Action types (13)
    let in_blind = state.stage == Stage::Blind;
    let in_pre_blind = state.stage == Stage::PreBlind;
    let in_post_blind = state.stage == Stage::PostBlind;
    let in_shop = state.stage == Stage::Shop;

    // 可以跳過的 Blind（Small 和 Big，不能跳過 Boss）
    let can_skip = in_pre_blind && state.blind_type != Some(BlindType::Boss);

    data[0] = if in_blind { 1.0 } else { 0.0 }; // SELECT
    data[1] = if in_blind && state.plays_left > 0 {
        1.0
    } else {
        0.0
    }; // PLAY
    data[2] = if in_blind && state.discards_left > 0 {
        1.0
    } else {
        0.0
    }; // DISCARD
    data[3] = if in_pre_blind { 1.0 } else { 0.0 }; // SELECT_BLIND
    data[4] = if in_post_blind { 1.0 } else { 0.0 }; // CASH_OUT
    data[5] = if in_shop { 1.0 } else { 0.0 }; // BUY_JOKER
    data[6] = if in_shop { 1.0 } else { 0.0 }; // NEXT_ROUND
    data[7] = if in_shop && state.shop.current_reroll_cost() <= state.money {
        1.0
    } else {
        0.0
    }; // REROLL
    data[8] = if in_shop && !state.jokers.is_empty() {
        1.0
    } else {
        0.0
    }; // SELL_JOKER
    data[9] = if can_skip { 1.0 } else { 0.0 }; // SKIP_BLIND

    // 新增的 action types
    let has_consumables = !state.consumables.items.is_empty();
    // Amber Boss Blind: 無法使用消耗品
    let amber_blocks = state.boss_blind == Some(BossBlind::Amber);
    data[10] = if (in_blind || in_shop) && has_consumables && !amber_blocks {
        1.0
    } else {
        0.0
    }; // USE_CONSUMABLE
    data[11] = if in_shop && state.shop_voucher.is_some() {
        1.0
    } else {
        0.0
    }; // BUY_VOUCHER
    data[12] = if in_shop { 1.0 } else { 0.0 }; // BUY_PACK
    offset += ACTION_TYPE_COUNT as usize;

    // Card selection (8 * 2 = 16)
    let can_select = in_blind;
    for _ in 0..HAND_SIZE {
        data[offset] = if can_select { 1.0 } else { 0.0 }; // 不選
        data[offset + 1] = if can_select { 1.0 } else { 0.0 }; // 選
        offset += 2;
    }

    // Blind selection (3)
    data[offset] = if in_pre_blind { 1.0 } else { 0.0 }; // Small
    data[offset + 1] = if in_pre_blind && state.blind_type == Some(BlindType::Small) {
        1.0
    } else {
        0.0
    }; // Big
    data[offset + 2] = if in_pre_blind && state.blind_type == Some(BlindType::Big) {
        1.0
    } else {
        0.0
    }; // Boss
    offset += 3;

    // Shop joker purchase (2)
    let effective_joker_slots = state.effective_joker_slot_limit();
    for i in 0..SHOP_JOKER_COUNT {
        let can_buy = in_shop
            && state
                .shop
                .items
                .get(i)
                .map(|item| item.cost <= state.money)
                .unwrap_or(false)
            && state.jokers.len() < effective_joker_slots;
        data[offset + i] = if can_buy { 1.0 } else { 0.0 };
    }
    offset += SHOP_JOKER_COUNT;

    // Sell joker slots (5)
    for i in 0..JOKER_SLOTS {
        // Eternal Jokers 無法賣出
        let can_sell = in_shop && i < state.jokers.len() && !state.jokers[i].is_eternal;
        data[offset + i] = if can_sell { 1.0 } else { 0.0 };
    }
    offset += JOKER_SLOTS;

    // Reroll (1)
    data[offset] = if in_shop && state.shop.current_reroll_cost() <= state.money {
        1.0
    } else {
        0.0
    };
    offset += 1;

    // Skip Blind (1)
    data[offset] = if can_skip { 1.0 } else { 0.0 };
    offset += 1;

    // Use consumable (2)
    for i in 0..CONSUMABLE_SLOT_COUNT {
        let can_use = (in_blind || in_shop) && i < state.consumables.items.len();
        data[offset + i] = if can_use { 1.0 } else { 0.0 };
    }
    offset += CONSUMABLE_SLOT_COUNT;

    // Buy voucher (1)
    let can_buy_voucher = in_shop
        && state.shop_voucher.is_some()
        && state
            .shop_voucher
            .as_ref()
            .map(|v| v.cost() <= state.money)
            .unwrap_or(false);
    data[offset] = if can_buy_voucher { 1.0 } else { 0.0 };
    offset += SHOP_VOUCHER_COUNT;

    // Buy pack (2)
    for i in 0..SHOP_PACK_COUNT {
        let can_buy = in_shop
            && state
                .shop_packs
                .get(i)
                .map(|p| p.cost <= state.money)
                .unwrap_or(false);
        data[offset + i] = if can_buy { 1.0 } else { 0.0 };
    }

    Tensor {
        data,
        shape: vec![ACTION_MASK_SIZE],
    }
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{BossBlind, Consumable, JokerId, JokerSlot, Stage, TarotId};
    use crate::service::state::{BoosterPack, BoosterPackType, EnvState};

    fn mask_data(state: &EnvState, done: bool) -> Vec<f32> {
        action_mask_from_state(state, done).data
    }

    #[test]
    fn test_action_mask_done_is_zeroed() {
        let state = EnvState::new(1);
        let data = mask_data(&state, true);
        assert_eq!(data.len(), ACTION_MASK_SIZE as usize);
        assert!(data.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_action_mask_pre_blind() {
        let mut state = EnvState::new(1);
        state.stage = Stage::PreBlind;
        state.blind_type = None;

        let data = mask_data(&state, false);
        // Action types
        assert_eq!(data[3], 1.0); // SELECT_BLIND
        assert_eq!(data[9], 1.0); // SKIP_BLIND

        // Card selection should be disabled out of blind
        let selection_start = ACTION_TYPE_COUNT as usize;
        assert_eq!(data[selection_start], 0.0);

        // Blind selection: only Small is available initially
        let blind_start = selection_start + HAND_SIZE * 2;
        assert_eq!(data[blind_start], 1.0);
        assert_eq!(data[blind_start + 1], 0.0);
        assert_eq!(data[blind_start + 2], 0.0);
    }

    #[test]
    fn test_action_mask_blind_with_consumables_and_amber_block() {
        let mut state = EnvState::new(1);
        state.stage = Stage::Blind;
        state.boss_blind = Some(BossBlind::Amber);
        state.consumables.add(Consumable::Tarot(TarotId::TheFool));

        let data = mask_data(&state, false);
        assert_eq!(data[0], 1.0); // SELECT
        assert_eq!(data[1], 1.0); // PLAY
        assert_eq!(data[2], 1.0); // DISCARD
        assert_eq!(data[10], 0.0); // USE_CONSUMABLE blocked by Amber
    }

    #[test]
    fn test_action_mask_shop_purchase_and_sell() {
        let mut state = EnvState::new(1);
        state.stage = Stage::Shop;
        state.money = 10;
        state.shop.items.push(crate::game::shop::ShopItem::new(JokerId::Joker, 6));
        state.shop_packs.push(BoosterPack {
            pack_type: BoosterPackType::Arcana,
            cost: 4,
        });
        state.shop_voucher = Some(crate::game::VoucherId::Grabber);
        state.jokers.push(JokerSlot::new(JokerId::Joker));

        let data = mask_data(&state, false);
        // Action types
        assert_eq!(data[5], 1.0); // BUY_JOKER
        assert_eq!(data[6], 1.0); // NEXT_ROUND
        assert_eq!(data[7], 1.0); // REROLL
        assert_eq!(data[11], 1.0); // BUY_VOUCHER
        assert_eq!(data[12], 1.0); // BUY_PACK

        // Buy Joker slot 0 should be available
        let shop_start = ACTION_TYPE_COUNT as usize + HAND_SIZE * 2 + 3;
        assert_eq!(data[shop_start], 1.0);

        // Sell joker slot 0 available (not eternal)
        let sell_start = shop_start + SHOP_JOKER_COUNT;
        assert_eq!(data[sell_start], 1.0);

        // Buy pack slot 0 available
        let pack_start = sell_start + JOKER_SLOTS + 1 + 1 + CONSUMABLE_SLOT_COUNT + SHOP_VOUCHER_COUNT;
        assert_eq!(data[pack_start], 1.0);
    }
}
