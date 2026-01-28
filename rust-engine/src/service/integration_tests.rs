//! Service-layer integration tests (blind + shop flows)

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use crate::game::{
        BlindType, JokerId, JokerSlot, VoucherId, PLAYS_PER_BLIND, DISCARDS_PER_BLIND,
    };
    use crate::service::{action_mask_from_state, calculate_play_score};
    use crate::service::state::{BoosterPack, BoosterPackType, EnvState};
    use crate::game::{Card, HandLevels, Stage};

    fn make_cards(ranks_suits: &[(u8, u8)]) -> Vec<Card> {
        ranks_suits.iter().map(|&(r, s)| Card::new(r, s)).collect()
    }

    #[test]
    fn test_full_blind_to_shop_with_specific_jokers() {
        let mut state = EnvState::new(7);
        state.stage = Stage::Blind;
        state.blind_type = Some(BlindType::Small);
        state.plays_left = PLAYS_PER_BLIND;
        state.discards_left = DISCARDS_PER_BLIND;
        state.money = 10;

        // Banner affects scoring + end-of-blind reward; Chaos affects shop size.
        state.jokers.push(JokerSlot::new(JokerId::Banner));
        state.jokers.push(JokerSlot::new(JokerId::GoldenJoker));
        state.jokers.push(JokerSlot::new(JokerId::ToTheMoon));
        state.jokers.push(JokerSlot::new(JokerId::ChaosTheClown));

        // Play a royal flush to guarantee clearing the blind.
        let selected = make_cards(&[(10, 3), (11, 3), (12, 3), (13, 3), (1, 3)]);
        let hand_levels = HandLevels::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);

        let score_result = calculate_play_score(
            &selected,
            &state.jokers,
            None,
            state.discards_left,
            state.rerolls_this_run,
            state.blinds_skipped,
            state.joker_slot_limit,
            0,
            true,
            false,
            0,
            &hand_levels,
            state.deck_type.uses_plasma_scoring(),
            state.voucher_effects.observatory_x_mult,
            state.planet_used_hand_types,
            &mut rng,
        );

        state.score += score_result.score;
        state.plays_left -= 1;
        assert!(state.score >= state.required_score());

        // Cash out -> shop
        let reward = state.calc_reward();
        state.money += reward;
        state.stage = Stage::Shop;
        state.refresh_shop();

        // ChaosTheClown: only 1 joker in shop
        assert_eq!(state.shop.items.len(), 1);
        assert_eq!(state.shop_packs.len(), 2);
    }

    #[test]
    fn test_shop_buy_joker_and_voucher_flow() {
        let mut state = EnvState::new(3);
        state.stage = Stage::Shop;
        state.money = 30;

        // Deterministic shop setup
        state.shop.items.clear();
        state.shop.items.push(crate::game::shop::ShopItem::new(JokerId::Joker, 6));
        state.shop.items.push(crate::game::shop::ShopItem::new(JokerId::JollyJoker, 7));
        state.shop_voucher = Some(VoucherId::Grabber);
        state.shop_packs = vec![
            BoosterPack {
                pack_type: BoosterPackType::Arcana,
                cost: 4,
            },
            BoosterPack {
                pack_type: BoosterPackType::Celestial,
                cost: 4,
            },
        ];

        let mask = action_mask_from_state(&state, false).data;
        assert_eq!(mask[5], 1.0);  // BUY_JOKER
        assert_eq!(mask[11], 1.0); // BUY_VOUCHER
        assert_eq!(mask[12], 1.0); // BUY_PACK

        // Buy a Joker
        let item = state.shop.buy(0).expect("shop item");
        state.money -= item.cost;
        state.jokers.push(item.joker);
        assert_eq!(state.jokers.len(), 1);

        // Buy a voucher
        let voucher = state.shop_voucher.take().expect("voucher");
        let cost = voucher.cost();
        state.money -= cost;
        assert!(state.voucher_effects.buy(voucher));

        // Ensure money decreased and voucher owned
        assert!(state.money < 30);
        assert!(state.voucher_effects.has(voucher));
    }
}
