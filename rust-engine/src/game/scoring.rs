//! 計分引擎
//!
//! 處理牌型判定和基礎計分邏輯

use super::cards::{Card, Enhancement};
use super::constants::MAX_SELECTED;
use super::hand_types::{HandId, HandScore};
use super::joker::JokerId;

// ============================================================================
// Joker Rules - 規則修改類 Joker 的效果
// ============================================================================

/// Joker 規則修改
///
/// 用於傳遞規則修改類 Joker 的效果到牌型判定邏輯：
/// - FourFingers (#126): 順子/同花只需 4 張牌
/// - Shortcut (#127): 順子可跳過 1 個點數
/// - Splash (#128): 所有牌計入所有牌型（所有牌計分）
/// - Pareidolia (#172): 所有牌視為人頭牌
/// - Smeared (#129): 紅黑花色合併（Hearts=Diamonds, Spades=Clubs）
/// - OopsAll6s (#253): 所有 6 算作每種花色
#[derive(Clone, Debug, Default)]
pub struct JokerRules {
    /// FourFingers (#126): 順子/同花只需 4 張牌
    pub four_fingers: bool,
    /// Shortcut (#127): 順子可跳過 1 個點數
    pub shortcut: bool,
    /// Splash (#128): 所有牌計入所有牌型
    pub splash: bool,
    /// Pareidolia (#172): 所有牌視為人頭牌
    pub pareidolia: bool,
    /// Smeared (#129): 紅黑花色合併
    pub smeared: bool,
    /// OopsAll6s (#253): 所有 6 算作每種花色
    pub oops_all_6s: bool,
}

impl JokerRules {
    pub fn new() -> Self {
        Self::default()
    }

    /// 從 Joker 列表構建規則
    pub fn from_jokers(jokers: &[super::joker::JokerSlot]) -> Self {
        let mut rules = Self::new();
        for joker in jokers.iter().filter(|j| j.enabled) {
            match joker.id {
                JokerId::FourFingers => rules.four_fingers = true,
                JokerId::Shortcut => rules.shortcut = true,
                JokerId::Splash => rules.splash = true,
                JokerId::Pareidolia => rules.pareidolia = true,
                JokerId::Smeared => rules.smeared = true,
                JokerId::OopsAll6s => rules.oops_all_6s = true,
                _ => {}
            }
        }
        rules
    }

    /// 獲取順子/同花所需的最少張數
    pub fn min_cards_for_straight_flush(&self) -> usize {
        if self.four_fingers {
            4
        } else {
            5
        }
    }
}

/// 判定手牌的牌型並計算基礎分數
pub fn score_hand(hand: &[Card]) -> HandScore {
    score_hand_with_rules(hand, &JokerRules::default())
}

/// 判定手牌的牌型（考慮 Joker 規則修改）
pub fn score_hand_with_rules(hand: &[Card], rules: &JokerRules) -> HandScore {
    if hand.is_empty() {
        return HandScore::new(HandId::HighCard);
    }

    // 過濾 Stone 牌（不參與牌型判定）和面朝下的牌
    let scoring_cards: Vec<&Card> = hand
        .iter()
        .take(MAX_SELECTED)
        .filter(|c| c.counts_for_hand() && !c.face_down)
        .collect();

    if scoring_cards.is_empty() {
        return HandScore::new(HandId::HighCard);
    }

    let mut rank_counts = [0u8; 13];
    let mut suit_counts = [0u8; 4];
    let mut wild_count = 0u8;

    for card in &scoring_cards {
        rank_counts[(card.rank - 1) as usize] += 1;

        // OopsAll6s: 所有 6 算作每種花色（如同 Wild）
        let is_wild_6 = rules.oops_all_6s && card.rank == 6;

        if card.enhancement == Enhancement::Wild || is_wild_6 {
            wild_count += 1;
        } else if rules.smeared {
            // Smeared (#129): 紅黑花色合併
            // Hearts (2) + Diamonds (1) = 紅色花色 -> 使用 1
            // Spades (0) + Clubs (3) = 黑色花色 -> 使用 0
            let effective_suit = match card.suit {
                1 | 2 => 1, // 紅色
                0 | 3 => 0, // 黑色
                _ => card.suit,
            };
            suit_counts[effective_suit as usize] += 1;
        } else {
            suit_counts[card.suit as usize] += 1;
        }
    }

    // Wild 牌可以加入任何花色來湊同花
    let max_suit_count = *suit_counts.iter().max().unwrap_or(&0);
    let effective_suit_count = max_suit_count + wild_count;

    let min_for_flush = rules.min_cards_for_straight_flush() as u8;
    let is_flush = effective_suit_count >= min_for_flush;
    let is_straight = check_straight_with_rules(&rank_counts, rules);

    let mut count_values: Vec<u8> = rank_counts
        .iter()
        .cloned()
        .filter(|&count| count > 0)
        .collect();
    count_values.sort_unstable_by(|a, b| b.cmp(a));

    let id = if is_flush && is_straight {
        if is_royal(&rank_counts) {
            HandId::RoyalFlush
        } else {
            HandId::StraightFlush
        }
    } else if count_values.first() == Some(&4) {
        HandId::FourKind
    } else if count_values.first() == Some(&3) && count_values.get(1) == Some(&2) {
        HandId::FullHouse
    } else if is_flush {
        HandId::Flush
    } else if is_straight {
        HandId::Straight
    } else if count_values.first() == Some(&3) {
        HandId::ThreeKind
    } else if count_values.first() == Some(&2) && count_values.get(1) == Some(&2) {
        HandId::TwoPair
    } else if count_values.first() == Some(&2) {
        HandId::Pair
    } else {
        HandId::HighCard
    };

    HandScore::new(id)
}

/// 檢查是否為順子
pub fn check_straight(rank_counts: &[u8; 13]) -> bool {
    check_straight_with_rules(rank_counts, &JokerRules::default())
}

/// 檢查是否為順子（考慮 Joker 規則）
///
/// - FourFingers: 只需 4 張連續牌
/// - Shortcut: 可跳過 1 個點數
pub fn check_straight_with_rules(rank_counts: &[u8; 13], rules: &JokerRules) -> bool {
    let min_cards = rules.min_cards_for_straight_flush();

    if rules.shortcut {
        // Shortcut: 允許跳過 1 個點數
        // 使用滑動窗口，允許最多 1 個 gap
        check_straight_with_gap(rank_counts, min_cards, 1)
    } else {
        // 標準順子檢查
        check_straight_normal(rank_counts, min_cards)
    }
}

/// 標準順子檢查
fn check_straight_normal(rank_counts: &[u8; 13], min_cards: usize) -> bool {
    // 檢查連續牌
    let mut consecutive = 0;
    for count in rank_counts.iter() {
        if *count > 0 {
            consecutive += 1;
            if consecutive >= min_cards {
                return true;
            }
        } else {
            consecutive = 0;
        }
    }

    // 檢查 A-low straights (wheel variants)
    // For 5-card: A-2-3-4-5
    // For 4-card: A-2-3-4
    if rank_counts[0] > 0 {
        // Ace
        let low_cards: Vec<bool> = (1..=4).map(|i| rank_counts[i] > 0).collect();
        let count = low_cards.iter().filter(|&&x| x).count() + 1; // +1 for Ace
        if count >= min_cards {
            return true;
        }
    }

    // 檢查 A-high straights (broadway variants)
    // For 5-card: 10-J-Q-K-A
    // For 4-card: J-Q-K-A or 10-J-Q-K-A with 4 required
    if rank_counts[0] > 0 {
        // Ace
        let high_cards: Vec<bool> = (9..=12).map(|i| rank_counts[i] > 0).collect();
        let count = high_cards.iter().filter(|&&x| x).count() + 1; // +1 for Ace
        if count >= min_cards {
            return true;
        }
    }

    false
}

/// 允許 gap 的順子檢查（Shortcut Joker）
fn check_straight_with_gap(rank_counts: &[u8; 13], min_cards: usize, max_gaps: usize) -> bool {
    // 使用窗口滑動檢查
    // 對於 5 張順子允許 1 gap，窗口大小為 6（5 張 + 1 gap）
    // 對於 4 張順子允許 1 gap，窗口大小為 5（4 張 + 1 gap）
    let window_size = min_cards + max_gaps;

    // 檢查主要範圍（2-K，索引 1-12）
    for start in 0..=(13 - window_size) {
        let cards_in_window: usize = rank_counts[start..start + window_size]
            .iter()
            .filter(|&&c| c > 0)
            .count();
        let gaps = window_size - cards_in_window;
        if cards_in_window >= min_cards && gaps <= max_gaps {
            return true;
        }
    }

    // 檢查 Ace-low straights with gap
    // A-2-3-4-5 範圍，允許 1 gap
    if rank_counts[0] > 0 {
        // Ace
        let low_range: Vec<bool> = (1..1 + window_size - 1)
            .filter(|&i| i < 13)
            .map(|i| rank_counts[i] > 0)
            .collect();
        let cards = low_range.iter().filter(|&&x| x).count() + 1; // +1 for Ace
        if cards >= min_cards {
            return true;
        }
    }

    // 檢查 Ace-high straights with gap
    // 10-J-Q-K-A 範圍，允許 1 gap
    if rank_counts[0] > 0 {
        // Ace
        let high_start = 13 - window_size;
        let high_range: Vec<bool> = (high_start..13)
            .filter(|&i| i > 0)
            .map(|i| rank_counts[i] > 0)
            .collect();
        let cards = high_range.iter().filter(|&&x| x).count() + 1; // +1 for Ace
        if cards >= min_cards {
            return true;
        }
    }

    false
}

/// 檢查是否為皇家同花順的點數組合
pub fn is_royal(rank_counts: &[u8; 13]) -> bool {
    rank_counts[0] > 0      // A
        && rank_counts[9] > 0   // 10
        && rank_counts[10] > 0  // J
        && rank_counts[11] > 0  // Q
        && rank_counts[12] > 0 // K
}

/// 計算手牌潛力分數（用於 observation）
pub fn hand_potential(hand: &[Card]) -> f32 {
    let score = score_hand(hand);
    let raw_score = score.raw_score();
    (raw_score as f32 / 200.0).min(1.0)
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::cards::Card;

    fn make_cards(ranks_suits: &[(u8, u8)]) -> Vec<Card> {
        ranks_suits.iter().map(|&(r, s)| Card::new(r, s)).collect()
    }

    #[test]
    fn test_high_card() {
        let cards = make_cards(&[(2, 0), (4, 1), (6, 2), (8, 3), (10, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::HighCard);
        assert_eq!(score.base_chips, 5);
        assert_eq!(score.base_mult, 1);
    }

    #[test]
    fn test_pair() {
        let cards = make_cards(&[(2, 0), (2, 1), (6, 2), (8, 3), (10, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::Pair);
        assert_eq!(score.base_chips, 10);
        assert_eq!(score.base_mult, 2);
    }

    #[test]
    fn test_two_pair() {
        let cards = make_cards(&[(2, 0), (2, 1), (8, 2), (8, 3), (10, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::TwoPair);
        assert_eq!(score.base_chips, 20);
        assert_eq!(score.base_mult, 2);
    }

    #[test]
    fn test_three_of_a_kind() {
        let cards = make_cards(&[(5, 0), (5, 1), (5, 2), (8, 3), (10, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::ThreeKind);
        assert_eq!(score.base_chips, 30);
        assert_eq!(score.base_mult, 3);
    }

    #[test]
    fn test_straight() {
        let cards = make_cards(&[(5, 0), (6, 1), (7, 2), (8, 3), (9, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::Straight);
        assert_eq!(score.base_chips, 30);
        assert_eq!(score.base_mult, 4);
    }

    #[test]
    fn test_straight_wheel() {
        // A-2-3-4-5
        let cards = make_cards(&[(1, 0), (2, 1), (3, 2), (4, 3), (5, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::Straight);
    }

    #[test]
    fn test_straight_broadway() {
        // 10-J-Q-K-A
        let cards = make_cards(&[(10, 0), (11, 1), (12, 2), (13, 3), (1, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::Straight);
    }

    #[test]
    fn test_flush() {
        let cards = make_cards(&[(2, 0), (4, 0), (6, 0), (8, 0), (10, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::Flush);
        assert_eq!(score.base_chips, 35);
        assert_eq!(score.base_mult, 4);
    }

    #[test]
    fn test_full_house() {
        let cards = make_cards(&[(5, 0), (5, 1), (5, 2), (8, 0), (8, 1)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::FullHouse);
        assert_eq!(score.base_chips, 40);
        assert_eq!(score.base_mult, 4);
    }

    #[test]
    fn test_four_of_a_kind() {
        let cards = make_cards(&[(7, 0), (7, 1), (7, 2), (7, 3), (10, 0)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::FourKind);
        assert_eq!(score.base_chips, 60);
        assert_eq!(score.base_mult, 7);
    }

    #[test]
    fn test_straight_flush() {
        let cards = make_cards(&[(5, 2), (6, 2), (7, 2), (8, 2), (9, 2)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::StraightFlush);
        assert_eq!(score.base_chips, 100);
        assert_eq!(score.base_mult, 8);
    }

    #[test]
    fn test_royal_flush() {
        let cards = make_cards(&[(10, 3), (11, 3), (12, 3), (13, 3), (1, 3)]);
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::RoyalFlush);
        assert_eq!(score.base_chips, 100);
        assert_eq!(score.base_mult, 8);
    }

    #[test]
    fn test_empty_hand() {
        let cards: Vec<Card> = vec![];
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::HighCard);
    }

    #[test]
    fn test_wild_card_flush() {
        // 4 hearts + 1 wild = flush
        let mut cards = make_cards(&[(2, 2), (4, 2), (6, 2), (8, 2), (10, 0)]);
        cards[4].enhancement = Enhancement::Wild;
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::Flush);
    }

    #[test]
    fn test_stone_card_excluded() {
        // Stone 牌不參與牌型判定
        let mut cards = make_cards(&[(5, 0), (5, 1), (5, 2), (8, 3), (10, 0)]);
        // 如果 Stone 牌也算，這會是三條，但 Stone 不算
        cards[2].enhancement = Enhancement::Stone;
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::Pair); // 只剩兩張 5
    }

    #[test]
    fn test_hand_potential() {
        let cards = make_cards(&[(5, 0), (5, 1), (5, 2), (8, 3), (10, 0)]);
        let potential = hand_potential(&cards);
        // Three of a kind: 30 * 3 = 90, 90/200 = 0.45
        assert!((potential - 0.45).abs() < 0.01);
    }

    // ========================================================================
    // JokerRules 測試 - FourFingers (#126)
    // ========================================================================

    #[test]
    fn test_four_fingers_flush_with_4_cards() {
        // 4 張同花 - 正常情況不是 Flush，但 FourFingers 啟用時是
        let cards = make_cards(&[(2, 2), (4, 2), (6, 2), (8, 2)]);

        // 無 FourFingers
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::HighCard);

        // 有 FourFingers
        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Flush);
    }

    #[test]
    fn test_four_fingers_straight_with_4_cards() {
        // 4 張順子 - 正常情況不是 Straight，但 FourFingers 啟用時是
        let cards = make_cards(&[(5, 0), (6, 1), (7, 2), (8, 3)]);

        // 無 FourFingers
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::HighCard);

        // 有 FourFingers
        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Straight);
    }

    #[test]
    fn test_four_fingers_straight_flush_with_4_cards() {
        // 4 張同花順 - FourFingers 啟用時應識別
        let cards = make_cards(&[(5, 2), (6, 2), (7, 2), (8, 2)]);

        // 無 FourFingers
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::HighCard);

        // 有 FourFingers
        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::StraightFlush);
    }

    #[test]
    fn test_four_fingers_wheel_straight() {
        // A-2-3-4 (4-card wheel straight)
        let cards = make_cards(&[(1, 0), (2, 1), (3, 2), (4, 3)]);

        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Straight);
    }

    #[test]
    fn test_four_fingers_broadway_straight() {
        // J-Q-K-A (4-card broadway)
        let cards = make_cards(&[(11, 0), (12, 1), (13, 2), (1, 3)]);

        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Straight);
    }

    #[test]
    fn test_four_fingers_still_requires_4_cards() {
        // Only 3 cards - not enough for FourFingers flush
        let cards = make_cards(&[(2, 2), (4, 2), (6, 2)]);

        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::HighCard);
    }

    // ========================================================================
    // JokerRules 測試 - Shortcut (#127)
    // ========================================================================

    #[test]
    fn test_shortcut_straight_with_gap() {
        // 有間隔的順子：2-3-5-6-7（跳過 4）
        let cards = make_cards(&[(2, 0), (3, 1), (5, 2), (6, 3), (7, 0)]);

        // 無 Shortcut
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::HighCard);

        // 有 Shortcut
        let mut rules = JokerRules::new();
        rules.shortcut = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Straight);
    }

    #[test]
    fn test_shortcut_with_two_gaps_fails() {
        // 有兩個間隔：2-3-5-7-8（跳過 4 和 6）- 應該失敗
        let cards = make_cards(&[(2, 0), (3, 1), (5, 2), (7, 3), (8, 0)]);

        let mut rules = JokerRules::new();
        rules.shortcut = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::HighCard); // 兩個間隔不是順子
    }

    #[test]
    fn test_shortcut_wheel_with_gap() {
        // A-2-4-5-6 (wheel with gap at 3)
        let cards = make_cards(&[(1, 0), (2, 1), (4, 2), (5, 3), (6, 0)]);

        let mut rules = JokerRules::new();
        rules.shortcut = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Straight);
    }

    #[test]
    fn test_shortcut_gap_at_end() {
        // 3-4-5-6-8 (gap at 7)
        let cards = make_cards(&[(3, 0), (4, 1), (5, 2), (6, 3), (8, 0)]);

        let mut rules = JokerRules::new();
        rules.shortcut = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Straight);
    }

    // ========================================================================
    // JokerRules 測試 - Smeared (#129)
    // ========================================================================

    #[test]
    fn test_smeared_suit_merging() {
        // 3 Hearts + 2 Diamonds - 正常情況不是 Flush
        // Smeared: Hearts 和 Diamonds 視為同色
        let cards = make_cards(&[(2, 2), (4, 2), (6, 2), (8, 1), (10, 1)]);

        // 無 Smeared
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::HighCard);

        // 有 Smeared
        let mut rules = JokerRules::new();
        rules.smeared = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Flush);
    }

    #[test]
    fn test_smeared_black_suits() {
        // 3 Spades + 2 Clubs - Smeared 應視為同色
        let cards = make_cards(&[(2, 0), (4, 0), (6, 0), (8, 3), (10, 3)]);

        // 無 Smeared
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::HighCard);

        // 有 Smeared
        let mut rules = JokerRules::new();
        rules.smeared = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Flush);
    }

    #[test]
    fn test_smeared_mixed_red_black() {
        // 3 red (2H, 4H, 6D) + 2 black (8S, 10C) - NOT a flush even with Smeared
        let cards = make_cards(&[(2, 2), (4, 2), (6, 1), (8, 0), (10, 3)]);

        let mut rules = JokerRules::new();
        rules.smeared = true;
        let score = score_hand_with_rules(&cards, &rules);
        // 3 red cards, 2 black cards - neither group has 5
        assert_eq!(score.id, HandId::HighCard);
    }

    #[test]
    fn test_smeared_straight_flush() {
        // 5-6-7-8-9 with 3 Hearts + 2 Diamonds - Smeared makes it a Straight Flush
        let cards = make_cards(&[(5, 2), (6, 2), (7, 2), (8, 1), (9, 1)]);

        // Without Smeared - only Straight
        let score = score_hand(&cards);
        assert_eq!(score.id, HandId::Straight);

        // With Smeared - becomes Straight Flush
        let mut rules = JokerRules::new();
        rules.smeared = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::StraightFlush);
    }

    // ========================================================================
    // JokerRules 測試 - Combined Rules
    // ========================================================================

    #[test]
    fn test_four_fingers_and_shortcut_combined() {
        // 4 張帶間隔順子：3-4-6-7（跳過 5）
        let cards = make_cards(&[(3, 0), (4, 1), (6, 2), (7, 3)]);

        // 只有 FourFingers
        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::HighCard); // 有間隔，不是順子

        // FourFingers + Shortcut
        rules.shortcut = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Straight); // 4 張允許 1 gap
    }

    #[test]
    fn test_smeared_four_fingers_combo() {
        // 4 cards: 3 Hearts + 1 Diamond - with FourFingers + Smeared = Flush
        let cards = make_cards(&[(2, 2), (4, 2), (6, 2), (8, 1)]);

        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        rules.smeared = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::Flush);
    }

    #[test]
    fn test_all_three_straight_rules() {
        // 3 cards consecutive + 1 gap, same "smeared" suit
        // 4-5-6-8 all red (2 Hearts, 2 Diamonds)
        let cards = make_cards(&[(4, 2), (5, 2), (6, 1), (8, 1)]);

        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        rules.shortcut = true;
        rules.smeared = true;

        let score = score_hand_with_rules(&cards, &rules);
        // 4 cards, 1 gap allowed, all same "red" suit = Straight Flush
        assert_eq!(score.id, HandId::StraightFlush);
    }

    #[test]
    fn test_shortcut_four_fingers_straight_flush() {
        // 4 cards with gap, same suit: 5-6-8-9 all Hearts
        let cards = make_cards(&[(5, 2), (6, 2), (8, 2), (9, 2)]);

        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        rules.shortcut = true;
        let score = score_hand_with_rules(&cards, &rules);
        assert_eq!(score.id, HandId::StraightFlush);
    }

    // ========================================================================
    // JokerRules 結構測試
    // ========================================================================

    #[test]
    fn test_joker_rules_from_jokers() {
        use crate::game::JokerSlot;

        // Create jokers with rule-modifying effects
        let jokers = vec![
            JokerSlot::new(JokerId::FourFingers),
            JokerSlot::new(JokerId::Shortcut),
            JokerSlot::new(JokerId::Splash),
            JokerSlot::new(JokerId::Pareidolia),
            JokerSlot::new(JokerId::Smeared),
        ];

        let rules = JokerRules::from_jokers(&jokers);

        assert!(rules.four_fingers);
        assert!(rules.shortcut);
        assert!(rules.splash);
        assert!(rules.pareidolia);
        assert!(rules.smeared);
    }

    #[test]
    fn test_joker_rules_disabled_jokers_ignored() {
        use crate::game::JokerSlot;

        // Create disabled jokers
        let mut joker = JokerSlot::new(JokerId::FourFingers);
        joker.enabled = false;

        let jokers = vec![joker];
        let rules = JokerRules::from_jokers(&jokers);

        // Disabled Joker should not activate the rule
        assert!(!rules.four_fingers);
    }

    #[test]
    fn test_min_cards_for_straight_flush() {
        // Default rules - need 5 cards
        let rules = JokerRules::new();
        assert_eq!(rules.min_cards_for_straight_flush(), 5);

        // With FourFingers - need only 4 cards
        let mut rules = JokerRules::new();
        rules.four_fingers = true;
        assert_eq!(rules.min_cards_for_straight_flush(), 4);
    }
}
