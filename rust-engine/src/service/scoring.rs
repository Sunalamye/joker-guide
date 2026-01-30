//! 計分服務

use rand::rngs::StdRng;
use rand::Rng;

use crate::game::{
    compute_joker_bonus_v2, score_hand_with_rules, BossBlind, Card, Enhancement, HandId, HandLevels,
    JokerId, JokerRules, JokerSlot, ScoringContext, Seal,
};

/// 從手牌中構建選中的牌
/// 注意：當 mask=0 時返回空 Vec，調用者需處理此情況
pub fn build_selected_hand(hand: &[Card], mask: u32) -> Vec<Card> {
    let mut selected = Vec::new();
    for (idx, card) in hand.iter().enumerate() {
        if ((mask >> idx) & 1) == 1 {
            selected.push(*card);
        }
    }
    selected
}

/// 卡片計分結果（用於追蹤金幣和破碎效果）
pub struct CardScoreResult {
    pub score: i64,
    pub hand_id: HandId,
    pub money_gained: i64,          // 從 Gold Seal / Lucky 獲得的金幣
    pub glass_to_break: Vec<usize>, // 需要破碎的 Glass 牌索引
    pub selzer_charges_used: i32,   // Selzer 使用的重觸發次數
    pub lucky_triggers: i32,        // Lucky 牌觸發次數 (for Lucky_Cat)

    // v6.9: Joker 貢獻追蹤（用於獎勵計算）
    pub joker_chip_contrib: f32,    // Joker chips 貢獻比例 [0, 1]
    pub joker_mult_contrib: f32,    // Joker mult 貢獻比例 [0, 1]
    pub joker_xmult_contrib: f32,   // Joker x_mult 正規化值 [0, 1]
}

/// 計算出牌分數（考慮 Boss Blind debuff、卡片增強和牌型等級）
pub fn calculate_play_score(
    selected: &[Card],
    jokers: &[JokerSlot],
    boss_blind: Option<BossBlind>,
    discards_remaining: i32,
    rerolls_this_run: i32,
    blinds_skipped: i32,
    joker_slot_limit: usize,
    enhanced_cards_in_deck: i32,
    is_first_hand: bool,
    is_final_hand: bool,
    selzer_charges: i32,
    hand_levels: &HandLevels,
    uses_plasma_scoring: bool,
    observatory_x_mult: f32,
    planet_used_hand_types: u16,
    rng: &mut StdRng,
) -> CardScoreResult {
    // 從 Joker 構建規則（FourFingers, Shortcut, Splash, Smeared 等）
    let rules = JokerRules::from_jokers(jokers);
    let hand_score = score_hand_with_rules(selected, &rules);

    // 獲取牌型等級加成
    let hand_type_idx = hand_score.id.to_index();
    let (level_chips, level_mult) = hand_levels.bonus(hand_type_idx);

    // 創建計分上下文
    let mut ctx = ScoringContext::new(selected, hand_score.id);
    ctx.discards_remaining = discards_remaining;
    ctx.joker_count = jokers.len();
    ctx.joker_slot_limit = joker_slot_limit;
    ctx.rerolls_this_run = rerolls_this_run;
    ctx.blinds_skipped = blinds_skipped;
    ctx.is_first_hand = is_first_hand;
    ctx.is_final_hand = is_final_hand;
    // 計算 Uncommon Joker 數量 (rarity == 2)
    ctx.uncommon_joker_count = jokers.iter().filter(|j| j.id.rarity() == 2).count();
    // 牌組中增強牌數量 (DriversLicense)
    ctx.enhanced_cards_in_deck = enhanced_cards_in_deck;
    // Mime: 手中持有牌能力重觸發
    ctx.has_mime = jokers.iter().any(|j| j.enabled && j.id == JokerId::Mime);

    // 生成隨機值給需要隨機效果的 Joker（如 Misprint）
    let rng_values: Vec<u8> = (0..jokers.len()).map(|_| rng.gen()).collect();

    let bonus = compute_joker_bonus_v2(jokers, &ctx, &rng_values);

    // 基礎值 + 等級加成 + Joker 加成
    let mut total_chips = hand_score.base_chips + level_chips + bonus.chip_bonus;
    let mut total_mult = hand_score.base_mult + level_mult + bonus.add_mult;
    let mut x_mult = bonus.mul_mult;
    let mut money_gained: i64 = 0;
    let mut glass_to_break = Vec::new();
    let mut selzer_charges_remaining = selzer_charges;
    let mut selzer_charges_used: i32 = 0;
    let mut lucky_triggers: i32 = 0;

    // 計算每張牌的貢獻（考慮增強、封印、版本效果）
    for (idx, card) in selected.iter().enumerate() {
        // 面朝下的牌不計分
        if card.face_down {
            continue;
        }

        // 檢查花色是否被 Boss 禁用
        let suit_disabled = boss_blind
            .map(|b| b.disables_suit(card.suit))
            .unwrap_or(false);

        // Wild 牌不受花色禁用影響
        let effectively_disabled = suit_disabled && card.enhancement != Enhancement::Wild;

        // 檢查 Face Card 是否被禁用
        let is_face = card.is_face();
        let face_disabled = boss_blind
            .map(|b| b.disables_face_cards() && is_face)
            .unwrap_or(false);

        // TheHead: 紅心牌只在第一手有效
        let head_disabled = boss_blind
            .map(|b| matches!(b, BossBlind::TheHead) && card.suit == 2 && !is_first_hand)
            .unwrap_or(false);

        if effectively_disabled || face_disabled || head_disabled {
            continue;
        }

        // Red Seal: 效果觸發兩次
        let mut trigger_count = if card.seal == Seal::Red { 2 } else { 1 };

        // Selzer: 計分牌額外重觸發一次
        if selzer_charges_remaining > 0 {
            trigger_count += 1;
            selzer_charges_remaining -= 1;
            selzer_charges_used += 1;
        }

        for _ in 0..trigger_count {
            // 加上卡片的 chips（含增強和版本加成）
            total_chips += card.chips();

            // 加上卡片的 add mult
            total_mult += card.add_mult();

            // 乘上卡片的 x mult
            x_mult *= card.x_mult();

            // Lucky 牌特殊效果
            if card.enhancement == Enhancement::Lucky {
                // 1/5 機率 +20 Mult
                if rng.gen_range(0..5) == 0 {
                    total_mult += 20;
                    lucky_triggers += 1;
                }
                // 1/15 機率 +$20
                if rng.gen_range(0..15) == 0 {
                    money_gained += 20;
                    lucky_triggers += 1;
                }
            }

            // Glass 牌：1/4 機率破碎
            if card.enhancement == Enhancement::Glass {
                if rng.gen_range(0..4) == 0 {
                    glass_to_break.push(idx);
                }
            }
        }

        // Gold Seal: 打出時 +$3（不受 Red Seal 影響）
        if card.seal == Seal::Gold {
            money_gained += 3;
        }
    }

    // TheFlint: 基礎 chips 和 mult 減半
    if boss_blind == Some(BossBlind::TheFlint) {
        total_chips = (total_chips + 1) / 2;
        total_mult = (total_mult + 1) / 2;
    }

    // Observatory: 如果擁有並且打出的牌型曾用過對應 Planet，每張計分牌 X Mult
    // 這裡簡化為整體 X Mult（而非每張牌）
    let observatory_bonus =
        if observatory_x_mult > 1.0 && (planet_used_hand_types & (1 << hand_type_idx)) != 0 {
            // 計分牌數量 (未被禁用的牌)
            let scoring_cards = selected.iter().filter(|c| !c.face_down).count();
            observatory_x_mult.powi(scoring_cards as i32)
        } else {
            1.0
        };

    let final_mult = ((total_mult as f32) * x_mult * observatory_bonus).max(1.0) as i64;

    // Plasma Deck: chips 和 mult 平衡後計算
    // 公式: balanced = (chips + mult) / 2, score = balanced * balanced
    let score = if uses_plasma_scoring {
        let balanced = (total_chips + final_mult) / 2;
        balanced * balanced
    } else {
        total_chips * final_mult
    };

    // v6.9: 計算 Joker 貢獻比例（用於獎勵計算）
    let joker_chip_contrib = if total_chips > 0 {
        (bonus.chip_bonus as f32) / (total_chips as f32)
    } else {
        0.0
    };

    let joker_mult_contrib = if total_mult > 0 {
        (bonus.add_mult as f32) / (total_mult as f32)
    } else {
        0.0
    };

    // x_mult 正規化：log 縮放，x4.0 = 1.0
    let joker_xmult_contrib = if x_mult > 1.0 {
        (x_mult.ln() / 4.0_f32.ln()).min(1.0)
    } else {
        0.0
    };

    CardScoreResult {
        score,
        hand_id: hand_score.id,
        money_gained,
        glass_to_break,
        selzer_charges_used,
        lucky_triggers,
        joker_chip_contrib,
        joker_mult_contrib,
        joker_xmult_contrib,
    }
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn make_cards(ranks_suits: &[(u8, u8)]) -> Vec<Card> {
        ranks_suits.iter().map(|&(r, s)| Card::new(r, s)).collect()
    }

    #[test]
    fn test_build_selected_hand_empty_mask() {
        let hand = make_cards(&[(2, 0), (3, 1)]);
        let selected = build_selected_hand(&hand, 0);
        // mask=0 時應返回空 Vec，不再自動選第一張
        assert_eq!(selected.len(), 0);
    }

    #[test]
    fn test_build_selected_hand_partial_mask() {
        let hand = make_cards(&[(2, 0), (3, 1), (4, 2)]);
        let selected = build_selected_hand(&hand, 0b101); // 選第 1 和第 3 張
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].rank, 2);
        assert_eq!(selected[1].rank, 4);
    }

    #[test]
    fn test_calculate_play_score_simple_straight() {
        let selected = make_cards(&[(5, 0), (6, 1), (7, 2), (8, 3), (9, 0)]);
        let jokers: Vec<JokerSlot> = Vec::new();
        let hand_levels = HandLevels::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

        let result = calculate_play_score(
            &selected,
            &jokers,
            None,
            2,
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

        // Base: Straight (30 chips, 4 mult)
        // Card chips: 5+6+7+8+9 = 35
        // Total chips: 65, mult: 4, score: 260
        assert_eq!(result.score, 260);
        assert_eq!(result.hand_id, HandId::Straight);
    }

    #[test]
    fn test_calculate_play_score_flint_halves_base() {
        let selected = make_cards(&[(5, 0), (6, 1), (7, 2), (8, 3), (9, 0)]);
        let jokers: Vec<JokerSlot> = Vec::new();
        let hand_levels = HandLevels::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

        let result = calculate_play_score(
            &selected,
            &jokers,
            Some(BossBlind::TheFlint),
            2,
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

        // Base chips 65 -> (65+1)/2 = 33
        // Base mult 4 -> (4+1)/2 = 2
        assert_eq!(result.score, 66);
    }

    #[test]
    fn test_calculate_play_score_plasma_deck() {
        let selected = make_cards(&[(5, 0), (6, 1), (7, 2), (8, 3), (9, 0)]);
        let jokers: Vec<JokerSlot> = Vec::new();
        let hand_levels = HandLevels::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

        let result = calculate_play_score(
            &selected,
            &jokers,
            None,
            2,
            0,
            0,
            5,
            0,
            false,
            false,
            0,
            &hand_levels,
            true,
            1.0,
            0,
            &mut rng,
        );

        // Total chips 65, final mult 4 -> balanced 34, score 1156
        assert_eq!(result.score, 1156);
    }

    #[test]
    fn test_calculate_play_score_observatory_bonus() {
        let selected = make_cards(&[(2, 0), (3, 1)]);
        let jokers: Vec<JokerSlot> = Vec::new();
        let hand_levels = HandLevels::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        let planet_mask = 1u16 << HandId::HighCard.to_index();

        let result = calculate_play_score(
            &selected,
            &jokers,
            None,
            2,
            0,
            0,
            5,
            0,
            false,
            false,
            0,
            &hand_levels,
            false,
            2.0,
            planet_mask,
            &mut rng,
        );

        // Base chips 5 + card chips (2+3) = 10
        // Base mult 1, observatory bonus 2^2 = 4
        assert_eq!(result.score, 40);
    }
}
