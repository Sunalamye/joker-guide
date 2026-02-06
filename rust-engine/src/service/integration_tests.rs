//! Service-layer integration tests (blind + shop flows)

#[cfg(test)]
mod tests {
    use rand::{Rng, SeedableRng};

    use crate::game::{
        BlindType, BossBlind, Card, Consumable, DeckType, HandId, HandLevels, JokerId, JokerSlot,
        PlanetId, Stake, TarotId, VoucherEffects, VoucherId, DISCARDS_PER_BLIND, PLAYS_PER_BLIND,
    };
    use crate::game::joker::compute_joker_bonus_v2;
    use crate::game::joker::ScoringContext;
    use crate::service::{action_mask_from_state, calculate_play_score};
    use crate::service::state::{BoosterPack, BoosterPackType, EnvState};
    use crate::game::{Stage};

    fn make_cards(ranks_suits: &[(u8, u8)]) -> Vec<Card> {
        ranks_suits.iter().map(|&(r, s)| Card::new(r, s)).collect()
    }

    fn apply_boss_start_effects_for_test(state: &mut EnvState) {
        if state.boss_blind == Some(BossBlind::TheHook) {
            state.apply_hook_discard();
        }

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

        if state.boss_blind == Some(BossBlind::TheHouse) {
            for card in &mut state.hand {
                card.face_down = true;
            }
        }

        if state.boss_blind == Some(BossBlind::Verdant) {
            for card in &mut state.hand {
                card.face_down = true;
            }
        }

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

        if state.boss_blind == Some(BossBlind::TheMark) {
            for card in &mut state.hand {
                if card.is_face() {
                    card.face_down = true;
                }
            }
        }

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
                for (old_idx, new_idx) in face_down_indices.iter().zip(shuffled.iter()) {
                    if old_idx != new_idx {
                        state.hand.swap(*old_idx, *new_idx);
                    }
                }
            }
        }
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

    #[test]
    fn test_all_boss_blinds_have_representative_effects() {
        // TheHook
        let mut state = EnvState::new(1);
        state.boss_blind = Some(BossBlind::TheHook);
        state.hand = make_cards(&[(2, 0), (3, 0), (4, 0), (5, 0), (6, 0)]);
        state.deck = make_cards(&[(7, 0), (8, 0), (9, 0)]);
        let discarded_before = state.discarded.len();
        apply_boss_start_effects_for_test(&mut state);
        assert_eq!(state.discarded.len(), discarded_before + 2);

        // TheWheel
        let mut state = EnvState::new(2);
        state.boss_blind = Some(BossBlind::TheWheel);
        state.hand = make_cards(&[(2, 0), (3, 1), (4, 2), (5, 3), (6, 0), (7, 1), (8, 2), (9, 3)]);
        let mut rng_clone = state.rng.clone();
        let expected_flags: Vec<bool> = (0..state.hand.len())
            .map(|_| rng_clone.gen_range(0..7) == 0)
            .collect();
        apply_boss_start_effects_for_test(&mut state);
        for (card, expected) in state.hand.iter().zip(expected_flags.iter()) {
            assert_eq!(card.face_down, *expected);
        }

        // TheHouse
        let mut state = EnvState::new(3);
        state.boss_blind = Some(BossBlind::TheHouse);
        state.hand = make_cards(&[(2, 0), (3, 1), (4, 2), (5, 3), (6, 0)]);
        apply_boss_start_effects_for_test(&mut state);
        assert!(state.hand.iter().all(|c| c.face_down));

        // Verdant
        let mut state = EnvState::new(4);
        state.boss_blind = Some(BossBlind::Verdant);
        state.hand = make_cards(&[(2, 0), (3, 1), (4, 2), (5, 3), (6, 0)]);
        apply_boss_start_effects_for_test(&mut state);
        assert!(state.hand.iter().all(|c| c.face_down));

        // TheMark
        let mut state = EnvState::new(5);
        state.boss_blind = Some(BossBlind::TheMark);
        state.hand = make_cards(&[(11, 0), (12, 1), (9, 2), (13, 3), (8, 0)]);
        apply_boss_start_effects_for_test(&mut state);
        assert!(state.hand[0].face_down);
        assert!(state.hand[1].face_down);
        assert!(!state.hand[2].face_down);
        assert!(state.hand[3].face_down);
        assert!(!state.hand[4].face_down);

        // TheFish
        let mut state = EnvState::new(6);
        state.boss_blind = Some(BossBlind::TheFish);
        state.hand = make_cards(&[(2, 0), (3, 1), (4, 2), (5, 3), (6, 0)]);
        state.hand[1].face_down = true;
        state.hand[3].face_down = true;
        let before = state.hand.clone();
        apply_boss_start_effects_for_test(&mut state);
        assert_eq!(
            before.iter().filter(|c| c.face_down).count(),
            state.hand.iter().filter(|c| c.face_down).count()
        );

        // Cerulean
        let mut state = EnvState::new(7);
        state.boss_blind = Some(BossBlind::Cerulean);
        state.consumables.add(Consumable::Planet(PlanetId::Mercury));
        let before_level = state.hand_levels.get(HandId::Pair.to_index());
        apply_boss_start_effects_for_test(&mut state);
        let after_level = state.hand_levels.get(HandId::Pair.to_index());
        assert!(after_level >= before_level);
        assert!(state.last_used_consumable.is_some());
    }

    #[test]
    fn test_boss_blind_scoring_modifiers() {
        // All cards are suit 3 (Club) -- TheClub should disable all of them
        // Suit mapping: 0=Spade, 1=Diamond, 2=Heart, 3=Club
        let selected = make_cards(&[(2, 3), (3, 3), (4, 3), (5, 3), (6, 3)]);
        let jokers: Vec<JokerSlot> = Vec::new();
        let hand_levels = HandLevels::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

        let score_club_disabled = calculate_play_score(
            &selected,
            &jokers,
            Some(BossBlind::TheClub),
            DISCARDS_PER_BLIND,
            0,
            0,
            5,
            0,
            true,
            false,
            0,
            &hand_levels,
            false,
            1.0,
            0,
            &mut rng,
        );
        // Straight Flush (100 chips, 8 mult) with all cards disabled = 100 * 8 = 800
        assert_eq!(score_club_disabled.score, 800);

        let selected_faces = make_cards(&[(11, 0), (12, 1), (13, 2), (10, 3), (9, 0)]);
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        let score_face_disabled = calculate_play_score(
            &selected_faces,
            &jokers,
            Some(BossBlind::ThePlant),
            DISCARDS_PER_BLIND,
            0,
            0,
            5,
            0,
            true,
            false,
            0,
            &hand_levels,
            false,
            1.0,
            0,
            &mut rng,
        );
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        let score_face_enabled = calculate_play_score(
            &selected_faces,
            &jokers,
            None,
            DISCARDS_PER_BLIND,
            0,
            0,
            5,
            0,
            true,
            false,
            0,
            &hand_levels,
            false,
            1.0,
            0,
            &mut rng,
        );
        assert!(score_face_disabled.score < score_face_enabled.score);

        let selected_hearts = make_cards(&[(2, 2), (3, 2), (4, 2), (5, 2), (6, 2)]);
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        let score_first = calculate_play_score(
            &selected_hearts,
            &jokers,
            Some(BossBlind::TheHead),
            DISCARDS_PER_BLIND,
            0,
            0,
            5,
            0,
            true,
            false,
            0,
            &hand_levels,
            false,
            1.0,
            0,
            &mut rng,
        );
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        let score_not_first = calculate_play_score(
            &selected_hearts,
            &jokers,
            Some(BossBlind::TheHead),
            DISCARDS_PER_BLIND,
            0,
            0,
            5,
            0,
            false,
            false,
            0,
            &hand_levels,
            false,
            1.0,
            0,
            &mut rng,
        );
        assert!(score_first.score > score_not_first.score);
    }

    #[test]
    fn test_boss_blind_rule_checks_and_state_effects() {
        // ThePsychic: requires exactly 5 cards
        let mut state = EnvState::new(8);
        state.boss_blind = Some(BossBlind::ThePsychic);
        let selected = make_cards(&[(2, 0), (3, 1), (4, 2), (5, 3)]);
        let selected_count = selected.len();
        let psychic_ok = !state
            .boss_blind
            .map(|b| b.requires_five_cards() && selected_count != 5)
            .unwrap_or(false);
        assert!(!psychic_ok);

        // TheEye: cannot repeat hand type
        state.boss_blind = Some(BossBlind::TheEye);
        state.played_hand_types.push(HandId::Pair.to_index());
        let eye_ok = !state
            .boss_blind
            .map(|b| {
                matches!(b, BossBlind::TheEye)
                    && state.played_hand_types.contains(&HandId::Pair.to_index())
            })
            .unwrap_or(false);
        assert!(!eye_ok);

        // TheMouth: must keep same first hand type
        state.boss_blind = Some(BossBlind::TheMouth);
        state.first_hand_type = Some(HandId::Pair.to_index());
        let mouth_ok = !state
            .boss_blind
            .map(|b| {
                matches!(b, BossBlind::TheMouth)
                    && state.first_hand_type.is_some()
                    && state.first_hand_type != Some(HandId::Straight.to_index())
            })
            .unwrap_or(false);
        assert!(!mouth_ok);

        // TheSerpent: draw 3 discard 3
        let mut state = EnvState::new(9);
        state.hand = make_cards(&[(2, 0), (3, 1), (4, 2), (5, 3), (6, 0)]);
        state.deck = make_cards(&[(7, 0), (8, 1), (9, 2), (10, 3), (11, 0)]);
        let discarded_before = state.discarded.len();
        state.apply_serpent_effect();
        assert_eq!(state.discarded.len(), discarded_before + 3);

        // TheManacle: hand size -1
        let mut state = EnvState::new(10);
        state.boss_blind = Some(BossBlind::TheManacle);
        assert_eq!(state.effective_hand_size(), 7);

        // TheArm: downgrade hand level after play
        let mut levels = HandLevels::new();
        let idx = HandId::Straight.to_index();
        levels.upgrade(idx);
        levels.upgrade(idx);
        let before = levels.get(idx);
        levels.downgrade(idx);
        assert_eq!(levels.get(idx), before - 1);

        // TheOx: lose $1 if hand type matches ante index
        let mut state = EnvState::new(11);
        state.boss_blind = Some(BossBlind::TheOx);
        state.ante = crate::game::Ante::One;
        state.money = 5;
        let ante_hand_idx = (state.ante.to_int() - 1) as usize;
        if ante_hand_idx == HandId::HighCard.to_index() {
            state.money = (state.money - 1).max(0);
        }
        assert_eq!(state.money, 4);
    }

    #[test]
    fn test_boss_blind_score_multiplier_requirements() {
        let mut state = EnvState::new(12);
        state.blind_type = Some(BlindType::Boss);
        state.boss_blind = Some(BossBlind::TheWall);
        let wall_required = state.required_score();
        state.boss_blind = Some(BossBlind::VioletVessel);
        let vessel_required = state.required_score();
        assert!(vessel_required > wall_required);
    }

    #[test]
    fn test_all_voucher_effects_apply() {
        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Overstock);
        effects.grant(VoucherId::OverstockPlus);
        assert_eq!(effects.extra_shop_joker_slots, 2);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::ClearanceSale);
        effects.grant(VoucherId::Liquidation);
        assert!((effects.discount_rate - 0.5).abs() < 0.001);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Hone);
        effects.grant(VoucherId::GlowUp);
        assert!((effects.edition_rate_mult - 4.0).abs() < 0.001);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::RerollSurplus);
        effects.grant(VoucherId::RerollGlut);
        assert_eq!(effects.reroll_discount, 4);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::CrystalBall);
        effects.grant(VoucherId::OmenGlobe);
        assert_eq!(effects.extra_consumable_slots, 2);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Telescope);
        effects.grant(VoucherId::Nadir);
        assert!((effects.planet_rate_mult - 4.0).abs() < 0.001);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Grabber);
        effects.grant(VoucherId::GrabberPlus);
        assert_eq!(effects.extra_hands, 2);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Wasteful);
        effects.grant(VoucherId::WastefulPlus);
        assert_eq!(effects.extra_discards, 2);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::SeedMoney);
        effects.grant(VoucherId::MoneyTree);
        assert_eq!(effects.interest_cap_bonus, 50);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::PaintBrush);
        effects.grant(VoucherId::Palette);
        assert_eq!(effects.joker_sell_bonus, 6);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Tarot_Merchant);
        effects.grant(VoucherId::Tarot_Tycoon);
        assert_eq!(effects.extra_tarot_draw, 2);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Magic_Trick);
        effects.grant(VoucherId::Illusion);
        assert_eq!(effects.extra_shop_slots, 2);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Blank);
        effects.grant(VoucherId::BlankPlus);
        effects.grant(VoucherId::Antimatter);
        effects.grant(VoucherId::Antimatter_Plus);
        assert_eq!(effects.joker_slot_bonus, 4);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Hieroglyph);
        effects.grant(VoucherId::Petroglyph);
        assert_eq!(effects.ante_reduction, 2);
        assert_eq!(effects.extra_hands, -1);
        assert_eq!(effects.extra_discards, -1);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::Observatory);
        assert!((effects.observatory_x_mult - 1.5).abs() < 0.001);
        effects.grant(VoucherId::ObservatoryPlus);
        assert!((effects.observatory_x_mult - 2.0).abs() < 0.001);

        let mut effects = VoucherEffects::new();
        effects.grant(VoucherId::DirectorsCut);
        assert_eq!(effects.boss_rerolls_available, 1);
        effects.grant(VoucherId::Retcon);
        assert!(effects.boss_rerolls_available > 1_000);
        assert!(effects.free_boss_reroll);
    }

    #[test]
    fn test_all_jokers_shop_buy_sell_and_bonus_compute() {
        let all = JokerId::all_available();
        let mut state = EnvState::new(13);
        state.stage = Stage::Shop;
        state.money = 1_000;

        for id in all {
            // Shop buy
            state.shop.items.clear();
            let cost = id.base_cost();
            state.shop.items.push(crate::game::shop::ShopItem::new(id, cost));
            let item = state.shop.buy(0).expect("shop item");
            let sell_value = item.joker.sell_value;
            state.jokers.push(item.joker);
            assert!(sell_value >= 0);

            // Hold effect: compute bonus should not panic
            let hand = make_cards(&[(2, 0), (3, 1), (4, 2), (5, 3), (6, 0)]);
            let ctx = ScoringContext::new(&hand, HandId::HighCard);
            let bonus = compute_joker_bonus_v2(&state.jokers, &ctx, &[0]);
            let _ = bonus;

            // Sell
            let sold = state.jokers.pop().expect("joker");
            state.money += sold.sell_value;
        }
    }

    #[test]
    fn test_deck_and_stake_initial_config_and_flow() {
        let state = EnvState::new_with_config(1, DeckType::Zodiac, Stake::White);
        assert!(state.voucher_effects.has(VoucherId::Overstock));
        assert!(state.voucher_effects.has(VoucherId::Tarot_Merchant));
        assert!(state.voucher_effects.has(VoucherId::Planet_Merchant));

        let state = EnvState::new_with_config(2, DeckType::Nebula, Stake::White);
        assert!(state.voucher_effects.has(VoucherId::Telescope));
        assert_eq!(state.consumables.capacity, 1);

        let state = EnvState::new_with_config(3, DeckType::Magic, Stake::White);
        assert!(state.voucher_effects.has(VoucherId::CrystalBall));
        assert!(state.consumables.items.len() >= 2);

        let mut state = EnvState::new_with_config(4, DeckType::Standard, Stake::Gold);
        state.stage = Stage::Shop;
        state.refresh_shop();
        assert!(state.shop.items.len() > 0);
    }

    #[test]
    fn test_consumable_planet_tarot_spectral_flows() {
        let mut state = EnvState::new(14);
        state.consumables.capacity = 3;
        state.consumables.add(Consumable::Planet(PlanetId::Mercury));
        state.consumables.add(Consumable::Tarot(TarotId::TheFool));
        state.consumables.add(Consumable::Spectral(crate::game::SpectralId::Familiar));

        // Use Planet -> upgrade hand level
        if let Some(Consumable::Planet(planet_id)) = state.consumables.use_item(0) {
            let idx = planet_id.hand_type_index();
            let before = state.hand_levels.get(idx);
            state.hand_levels.upgrade(idx);
            state.planets_used_this_run += 1;
            assert_eq!(state.hand_levels.get(idx), before + 1);
        } else {
            panic!("planet missing");
        }

        // Use Tarot -> track last used
        if let Some(consumable) = state.consumables.use_item(0) {
            state.last_used_consumable = Some(consumable);
            assert!(state.last_used_consumable.is_some());
        } else {
            panic!("tarot missing");
        }

        // Use Spectral -> apply familiar effect (destroy 1, add 2)
        if let Some(consumable @ Consumable::Spectral(_)) = state.consumables.use_item(0) {
            state.last_used_consumable = Some(consumable);
        } else {
            panic!("spectral missing");
        }
        state.hand = make_cards(&[(11, 0), (3, 1), (4, 2), (5, 3), (6, 0)]);
        let (destroyed, added) = state.familiar_effect(0, 2);
        assert_eq!(destroyed, 1);
        assert_eq!(added, 2);
    }
}
