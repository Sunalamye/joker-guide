//! 計分引擎
//!
//! 處理牌型判定和基礎計分邏輯

use super::cards::{Card, Enhancement};
use super::constants::MAX_SELECTED;
use super::hand_types::{HandId, HandScore};

/// 判定手牌的牌型並計算基礎分數
pub fn score_hand(hand: &[Card]) -> HandScore {
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

        if card.enhancement == Enhancement::Wild {
            wild_count += 1;
        } else {
            suit_counts[card.suit as usize] += 1;
        }
    }

    // Wild 牌可以加入任何花色來湊同花
    let max_suit_count = *suit_counts.iter().max().unwrap_or(&0);
    let effective_suit_count = max_suit_count + wild_count;

    let is_flush = effective_suit_count >= 5;
    let is_straight = check_straight(&rank_counts);

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
    // 檢查連續 5 張
    let mut consecutive = 0;
    for count in rank_counts.iter() {
        if *count > 0 {
            consecutive += 1;
            if consecutive >= 5 {
                return true;
            }
        } else {
            consecutive = 0;
        }
    }

    // 檢查 A-2-3-4-5 (wheel)
    if rank_counts[0] > 0
        && rank_counts[1] > 0
        && rank_counts[2] > 0
        && rank_counts[3] > 0
        && rank_counts[4] > 0
    {
        return true;
    }

    // 檢查 10-J-Q-K-A (broadway)
    if rank_counts[0] > 0
        && rank_counts[9] > 0
        && rank_counts[10] > 0
        && rank_counts[11] > 0
        && rank_counts[12] > 0
    {
        return true;
    }

    false
}

/// 檢查是否為皇家同花順的點數組合
pub fn is_royal(rank_counts: &[u8; 13]) -> bool {
    rank_counts[0] > 0      // A
        && rank_counts[9] > 0   // 10
        && rank_counts[10] > 0  // J
        && rank_counts[11] > 0  // Q
        && rank_counts[12] > 0  // K
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
}
