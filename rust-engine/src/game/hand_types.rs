//! 牌型定義

/// 牌型 ID
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandId {
    HighCard,
    Pair,
    TwoPair,
    ThreeKind,
    Straight,
    Flush,
    FullHouse,
    FourKind,
    StraightFlush,
    RoyalFlush,
    // 進階牌型（需要特殊條件解鎖）
    FiveKind,    // 5 張相同點數
    FlushHouse,  // Flush + Full House
    FlushFive,   // Flush + Five of a Kind
}

impl HandId {
    /// 獲取牌型的基礎 chips 和 mult
    pub fn base_values(&self) -> (i64, i64) {
        match self {
            HandId::HighCard => (5, 1),
            HandId::Pair => (10, 2),
            HandId::TwoPair => (20, 2),
            HandId::ThreeKind => (30, 3),
            HandId::Straight => (30, 4),
            HandId::Flush => (35, 4),
            HandId::FullHouse => (40, 4),
            HandId::FourKind => (60, 7),
            HandId::StraightFlush => (100, 8),
            HandId::RoyalFlush => (100, 8),
            HandId::FiveKind => (120, 12),
            HandId::FlushHouse => (140, 14),
            HandId::FlushFive => (160, 16),
        }
    }

    /// 獲取牌型在 observation 中的索引
    pub fn to_index(&self) -> usize {
        match self {
            HandId::HighCard => 0,
            HandId::Pair => 1,
            HandId::TwoPair => 2,
            HandId::ThreeKind => 3,
            HandId::Straight => 4,
            HandId::Flush => 5,
            HandId::FullHouse => 6,
            HandId::FourKind => 7,
            HandId::StraightFlush => 8,
            HandId::RoyalFlush => 9,
            HandId::FiveKind => 10,
            HandId::FlushHouse => 11,
            HandId::FlushFive => 12,
        }
    }
}

/// 手牌計分結果
#[derive(Clone, Copy, Debug)]
pub struct HandScore {
    pub base_chips: i64,
    pub base_mult: i64,
    pub id: HandId,
}

impl HandScore {
    pub fn new(id: HandId) -> Self {
        let (base_chips, base_mult) = id.base_values();
        Self {
            base_chips,
            base_mult,
            id,
        }
    }

    /// 計算原始分數（不含 Joker 效果）
    pub fn raw_score(&self) -> i64 {
        self.base_chips * self.base_mult
    }
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hand_base_values() {
        assert_eq!(HandId::HighCard.base_values(), (5, 1));
        assert_eq!(HandId::Flush.base_values(), (35, 4));
        assert_eq!(HandId::RoyalFlush.base_values(), (100, 8));
        assert_eq!(HandId::FlushFive.base_values(), (160, 16));
    }

    #[test]
    fn test_hand_to_index() {
        assert_eq!(HandId::HighCard.to_index(), 0);
        assert_eq!(HandId::Straight.to_index(), 4);
        assert_eq!(HandId::RoyalFlush.to_index(), 9);
        assert_eq!(HandId::FlushFive.to_index(), 12);
    }

    #[test]
    fn test_hand_raw_score() {
        let score = HandScore::new(HandId::TwoPair);
        assert_eq!(score.raw_score(), 40);
    }
}
