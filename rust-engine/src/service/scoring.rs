//! 計分服務

use rand::rngs::StdRng;
use rand::Rng;

use crate::game::{
    BossBlind, Card, Enhancement, HandId, JokerSlot, Seal,
    ScoringContext, compute_joker_bonus, score_hand_with_rules, JokerRules,
};

/// 從手牌中構建選中的牌
pub fn build_selected_hand(hand: &[Card], mask: u32) -> Vec<Card> {
    let mut selected = Vec::new();
    for (idx, card) in hand.iter().enumerate() {
        if ((mask >> idx) & 1) == 1 {
            selected.push(*card);
        }
    }
    // 確保至少有一張牌
    if selected.is_empty() && !hand.is_empty() {
        selected.push(hand[0]);
    }
    selected
}

/// 卡片計分結果（用於追蹤金幣和破碎效果）
pub struct CardScoreResult {
    pub score: i64,
    pub hand_id: HandId,
    pub money_gained: i64,          // 從 Gold Seal / Lucky 獲得的金幣
    pub glass_to_break: Vec<usize>, // 需要破碎的 Glass 牌索引
}

/// 計算出牌分數（考慮 Boss Blind debuff 和卡片增強）
pub fn calculate_play_score(
    selected: &[Card],
    jokers: &[JokerSlot],
    boss_blind: Option<BossBlind>,
    discards_remaining: i32,
    rerolls_this_run: i32,
    blinds_skipped: i32,
    rng: &mut StdRng,
) -> CardScoreResult {
    // 從 Joker 構建規則（FourFingers, Shortcut, Splash, Smeared 等）
    let rules = JokerRules::from_jokers(jokers);
    let hand_score = score_hand_with_rules(selected, &rules);

    // 創建計分上下文
    let mut ctx = ScoringContext::new(selected, hand_score.id);
    ctx.discards_remaining = discards_remaining;
    ctx.joker_count = jokers.len();
    ctx.rerolls_this_run = rerolls_this_run;
    ctx.blinds_skipped = blinds_skipped;

    // 生成隨機值給需要隨機效果的 Joker（如 Misprint）
    let rng_values: Vec<u8> = (0..jokers.len()).map(|_| rng.gen()).collect();

    let bonus = compute_joker_bonus(jokers, &ctx, &rng_values);

    let mut total_chips = hand_score.base_chips + bonus.chip_bonus;
    let mut total_mult = hand_score.base_mult + bonus.add_mult;
    let mut x_mult = bonus.mul_mult;
    let mut money_gained: i64 = 0;
    let mut glass_to_break = Vec::new();

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

        if effectively_disabled || face_disabled {
            continue;
        }

        // Red Seal: 效果觸發兩次
        let trigger_count = if card.seal == Seal::Red { 2 } else { 1 };

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
                }
                // 1/15 機率 +$20
                if rng.gen_range(0..15) == 0 {
                    money_gained += 20;
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

    let final_mult = ((total_mult as f32) * x_mult).max(1.0) as i64;
    let score = total_chips * final_mult;

    CardScoreResult {
        score,
        hand_id: hand_score.id,
        money_gained,
        glass_to_break,
    }
}
