//! 卡牌和增強系統定義

/// 卡片增強類型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Enhancement {
    #[default]
    None,
    Bonus, // +30 chips
    Mult,  // +4 mult
    Wild,  // 可當任意花色
    Glass, // x2 Mult，1/4 機率破碎
    Steel, // x1.5 Mult（在手牌中時）
    Stone, // +50 chips，不計花色/點數
    Gold,  // 回合結束時 +$3
    Lucky, // 1/5 機率 +20 Mult，1/15 機率 +$20
}

impl Enhancement {
    /// 轉換為整數 ID (用於 observation)
    pub fn to_int(&self) -> u8 {
        match self {
            Enhancement::None => 0,
            Enhancement::Bonus => 1,
            Enhancement::Mult => 2,
            Enhancement::Wild => 3,
            Enhancement::Glass => 4,
            Enhancement::Steel => 5,
            Enhancement::Stone => 6,
            Enhancement::Gold => 7,
            Enhancement::Lucky => 8,
        }
    }

    /// 所有增強類型（用於隨機選擇）
    pub fn all() -> &'static [Enhancement] {
        &[
            Enhancement::Bonus,
            Enhancement::Mult,
            Enhancement::Wild,
            Enhancement::Glass,
            Enhancement::Steel,
            Enhancement::Stone,
            Enhancement::Gold,
            Enhancement::Lucky,
        ]
    }
}

/// 卡片封印類型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Seal {
    #[default]
    None,
    Gold,   // 打出時 +$3
    Red,    // 觸發 2 次
    Blue,   // 最後一手牌創建 Planet 卡
    Purple, // 棄掉時創建 Tarot 卡
}

impl Seal {
    /// 轉換為整數 ID (用於 observation)
    pub fn to_int(&self) -> u8 {
        match self {
            Seal::None => 0,
            Seal::Gold => 1,
            Seal::Red => 2,
            Seal::Blue => 3,
            Seal::Purple => 4,
        }
    }

    /// 所有封印類型（用於隨機選擇）
    pub fn all() -> &'static [Seal] {
        &[Seal::Gold, Seal::Red, Seal::Blue, Seal::Purple]
    }
}

/// 卡片版本類型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Edition {
    #[default]
    Base,
    Foil,        // +50 chips
    Holographic, // +10 mult
    Polychrome,  // x1.5 mult
    Negative,    // +1 Joker slot (特殊，通常用於 Joker)
}

impl Edition {
    /// 轉換為整數 ID (用於 observation)
    pub fn to_int(&self) -> u8 {
        match self {
            Edition::Base => 0,
            Edition::Foil => 1,
            Edition::Holographic => 2,
            Edition::Polychrome => 3,
            Edition::Negative => 4,
        }
    }

    /// 所有版本類型（用於隨機選擇，不含 Negative）
    pub fn all_common() -> &'static [Edition] {
        &[Edition::Foil, Edition::Holographic, Edition::Polychrome]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    pub rank: u8, // 1..=13 (Ace = 1)
    pub suit: u8, // 0..=3
    pub enhancement: Enhancement,
    pub seal: Seal,
    pub edition: Edition,
    pub face_down: bool,  // 是否面朝下（某些 Boss Blind 效果）
    pub bonus_chips: i64, // Hiker 等效果的永久 Chips 加成
}

impl Card {
    pub fn new(rank: u8, suit: u8) -> Self {
        Self {
            rank,
            suit,
            enhancement: Enhancement::None,
            seal: Seal::None,
            edition: Edition::Base,
            face_down: false,
            bonus_chips: 0,
        }
    }

    /// 基礎 chips（不含增強效果）
    pub fn base_chips(&self) -> i64 {
        match self.rank {
            1 => 11,            // Ace
            11 | 12 | 13 => 10, // J, Q, K
            n => n as i64,
        }
    }

    /// 總 chips（含增強、版本效果和永久加成）
    pub fn chips(&self) -> i64 {
        let base = self.base_chips();
        let enhancement_bonus = match self.enhancement {
            Enhancement::Bonus => 30,
            Enhancement::Stone => 50,
            _ => 0,
        };
        let edition_bonus = match self.edition {
            Edition::Foil => 50,
            _ => 0,
        };
        base + enhancement_bonus + edition_bonus + self.bonus_chips
    }

    /// 加法 mult 加成
    pub fn add_mult(&self) -> i64 {
        let enhancement_mult = match self.enhancement {
            Enhancement::Mult => 4,
            _ => 0,
        };
        let edition_mult = match self.edition {
            Edition::Holographic => 10,
            _ => 0,
        };
        enhancement_mult + edition_mult
    }

    /// 乘法 mult 加成
    pub fn x_mult(&self) -> f32 {
        let mut x = 1.0;

        // Enhancement x mult
        if self.enhancement == Enhancement::Glass {
            x *= 2.0;
        }

        // Edition x mult
        if self.edition == Edition::Polychrome {
            x *= 1.5;
        }

        x
    }

    /// 是否為面牌 (J, Q, K)
    pub fn is_face(&self) -> bool {
        self.rank >= 11 && self.rank <= 13
    }

    /// 是否為面牌（考慮 Pareidolia 規則）
    ///
    /// Pareidolia (#172): 所有牌視為人頭牌
    pub fn is_face_with_pareidolia(&self, pareidolia: bool) -> bool {
        if pareidolia {
            true
        } else {
            self.is_face()
        }
    }

    /// Wild 牌是否可匹配指定花色
    pub fn matches_suit(&self, target_suit: u8) -> bool {
        if self.enhancement == Enhancement::Wild {
            true // Wild 可匹配任意花色
        } else {
            self.suit == target_suit
        }
    }

    /// Stone 牌不參與牌型判定
    pub fn counts_for_hand(&self) -> bool {
        self.enhancement != Enhancement::Stone
    }

    /// 獲取有效花色（用於計分）
    pub fn effective_suit(&self) -> u8 {
        if self.enhancement == Enhancement::Stone {
            255 // 無效花色
        } else {
            self.suit
        }
    }

    /// 獲取 Smeared 規則下的有效花色
    ///
    /// Smeared (#129):
    /// - Hearts (2) 和 Diamonds (1) 視為同一花色（紅色）
    /// - Spades (0) 和 Clubs (3) 視為同一花色（黑色）
    pub fn effective_suit_smeared(&self) -> u8 {
        match self.suit {
            1 | 2 => 1, // 紅色花色 -> 統一為 1
            0 | 3 => 0, // 黑色花色 -> 統一為 0
            _ => self.suit,
        }
    }
}

/// 創建標準 52 張牌組
pub fn standard_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);
    for suit in 0..4 {
        for rank in 1..=13 {
            deck.push(Card::new(rank, suit));
        }
    }
    deck
}

/// 獲取卡片在 52 張牌組中的索引
pub fn card_index(card: Card) -> usize {
    (card.suit as usize * 13) + (card.rank as usize - 1)
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_card_base_chips() {
        assert_eq!(Card::new(1, 0).base_chips(), 11);
        assert_eq!(Card::new(12, 1).base_chips(), 10);
        assert_eq!(Card::new(7, 2).base_chips(), 7);
    }

    #[test]
    fn test_card_chips_with_enhancement_and_edition() {
        let mut card = Card::new(9, 3);
        card.enhancement = Enhancement::Bonus;
        card.edition = Edition::Foil;
        card.bonus_chips = 5;
        assert_eq!(card.chips(), 9 + 30 + 50 + 5);
    }

    #[test]
    fn test_card_mults() {
        let mut card = Card::new(4, 0);
        card.enhancement = Enhancement::Mult;
        card.edition = Edition::Holographic;
        assert_eq!(card.add_mult(), 14);

        card.enhancement = Enhancement::Glass;
        card.edition = Edition::Polychrome;
        assert!((card.x_mult() - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_face_and_pareidolia() {
        let face = Card::new(12, 0);
        let not_face = Card::new(9, 1);
        assert!(face.is_face());
        assert!(!not_face.is_face());
        assert!(not_face.is_face_with_pareidolia(true));
        assert!(!not_face.is_face_with_pareidolia(false));
    }

    #[test]
    fn test_suit_matching_and_counts() {
        let mut card = Card::new(3, 2);
        assert!(card.matches_suit(2));
        assert!(!card.matches_suit(1));

        card.enhancement = Enhancement::Wild;
        assert!(card.matches_suit(1));

        card.enhancement = Enhancement::Stone;
        assert!(!card.counts_for_hand());
    }

    #[test]
    fn test_effective_suits() {
        let mut card = Card::new(3, 2);
        assert_eq!(card.effective_suit(), 2);

        card.enhancement = Enhancement::Stone;
        assert_eq!(card.effective_suit(), 255);

        let hearts = Card::new(2, 2);
        let diamonds = Card::new(2, 1);
        let spades = Card::new(2, 0);
        let clubs = Card::new(2, 3);
        assert_eq!(hearts.effective_suit_smeared(), 1);
        assert_eq!(diamonds.effective_suit_smeared(), 1);
        assert_eq!(spades.effective_suit_smeared(), 0);
        assert_eq!(clubs.effective_suit_smeared(), 0);
    }

    #[test]
    fn test_standard_deck_and_index() {
        let deck = standard_deck();
        assert_eq!(deck.len(), 52);

        let mut seen = HashSet::new();
        for card in deck {
            let idx = card_index(card);
            assert!(seen.insert(idx));
        }
        assert_eq!(seen.len(), 52);
    }
}
