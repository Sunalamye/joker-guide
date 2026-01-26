//! 起始牌組系統
//!
//! 定義不同類型的起始牌組，每種都有獨特的效果

use rand::prelude::*;
use rand::rngs::StdRng;

use super::cards::{Card, standard_deck};
use super::constants::{DISCARDS_PER_BLIND, HAND_SIZE, JOKER_SLOTS, PLAYS_PER_BLIND, STARTING_MONEY};

/// 起始牌組類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeckType {
    /// 標準牌組（無特殊效果）
    Standard,

    /// 紅色牌組：每回合 +1 棄牌
    Red,

    /// 藍色牌組：每回合 +1 出牌
    Blue,

    /// 黃色牌組：起始 +$10
    Yellow,

    /// 綠色牌組：無利息，每剩餘手牌 +$2
    Green,

    /// 黑色牌組：+1 Joker 欄位，-1 出牌
    Black,

    /// 幽靈牌組：所有牌有特殊版本機率
    Ghost,

    /// 廢棄牌組：起始沒有人頭牌
    Abandoned,

    /// 棋盤牌組：26 張黑桃 + 26 張紅心
    Checkered,

    /// 繪畫牌組：+2 手牌大小，-1 Joker 欄位
    Painted,

    /// 電漿牌組：Chips 和 Mult 合併計算
    Plasma,

    /// 不穩定牌組：點數和花色隨機化
    Erratic,
}

impl DeckType {
    /// 所有牌組類型
    pub fn all() -> &'static [DeckType] {
        &[
            DeckType::Standard,
            DeckType::Red,
            DeckType::Blue,
            DeckType::Yellow,
            DeckType::Green,
            DeckType::Black,
            DeckType::Ghost,
            DeckType::Abandoned,
            DeckType::Checkered,
            DeckType::Painted,
            DeckType::Plasma,
            DeckType::Erratic,
        ]
    }

    /// 牌組名稱
    pub fn name(&self) -> &'static str {
        match self {
            DeckType::Standard => "Standard Deck",
            DeckType::Red => "Red Deck",
            DeckType::Blue => "Blue Deck",
            DeckType::Yellow => "Yellow Deck",
            DeckType::Green => "Green Deck",
            DeckType::Black => "Black Deck",
            DeckType::Ghost => "Ghost Deck",
            DeckType::Abandoned => "Abandoned Deck",
            DeckType::Checkered => "Checkered Deck",
            DeckType::Painted => "Painted Deck",
            DeckType::Plasma => "Plasma Deck",
            DeckType::Erratic => "Erratic Deck",
        }
    }

    /// 生成該牌組的初始牌
    pub fn create_deck(&self, rng: &mut StdRng) -> Vec<Card> {
        match self {
            DeckType::Abandoned => create_abandoned_deck(),
            DeckType::Checkered => create_checkered_deck(),
            DeckType::Erratic => create_erratic_deck(rng),
            _ => standard_deck(),
        }
    }

    /// 起始金幣修正
    pub fn starting_money_modifier(&self) -> i64 {
        match self {
            DeckType::Yellow => 10,
            _ => 0,
        }
    }

    /// 每回合出牌次數修正
    pub fn plays_per_blind_modifier(&self) -> i32 {
        match self {
            DeckType::Blue => 1,
            DeckType::Black => -1,
            _ => 0,
        }
    }

    /// 每回合棄牌次數修正
    pub fn discards_per_blind_modifier(&self) -> i32 {
        match self {
            DeckType::Red => 1,
            _ => 0,
        }
    }

    /// Joker 欄位修正
    pub fn joker_slots_modifier(&self) -> i32 {
        match self {
            DeckType::Black => 1,
            DeckType::Painted => -1,
            _ => 0,
        }
    }

    /// 手牌大小修正
    pub fn hand_size_modifier(&self) -> i32 {
        match self {
            DeckType::Painted => 2,
            _ => 0,
        }
    }

    /// 是否禁用利息（Green Deck）
    pub fn disables_interest(&self) -> bool {
        matches!(self, DeckType::Green)
    }

    /// 每剩餘手牌的額外金幣（Green Deck）
    pub fn money_per_remaining_hand(&self) -> i64 {
        match self {
            DeckType::Green => 2,
            _ => 1, // 預設 $1
        }
    }

    /// 是否使用 Plasma 計分模式
    pub fn uses_plasma_scoring(&self) -> bool {
        matches!(self, DeckType::Plasma)
    }

    /// to_index 用於 observation
    pub fn to_index(&self) -> usize {
        match self {
            DeckType::Standard => 0,
            DeckType::Red => 1,
            DeckType::Blue => 2,
            DeckType::Yellow => 3,
            DeckType::Green => 4,
            DeckType::Black => 5,
            DeckType::Ghost => 6,
            DeckType::Abandoned => 7,
            DeckType::Checkered => 8,
            DeckType::Painted => 9,
            DeckType::Plasma => 10,
            DeckType::Erratic => 11,
        }
    }
}

/// 牌組配置（應用於遊戲狀態）
#[derive(Clone, Debug)]
pub struct DeckConfig {
    pub deck_type: DeckType,
    pub starting_money: i64,
    pub plays_per_blind: i32,
    pub discards_per_blind: i32,
    pub joker_slots: usize,
    pub hand_size: usize,
}

impl DeckConfig {
    pub fn from_deck_type(deck_type: DeckType) -> Self {
        Self {
            deck_type,
            starting_money: STARTING_MONEY + deck_type.starting_money_modifier(),
            plays_per_blind: PLAYS_PER_BLIND + deck_type.plays_per_blind_modifier(),
            discards_per_blind: DISCARDS_PER_BLIND + deck_type.discards_per_blind_modifier(),
            joker_slots: (JOKER_SLOTS as i32 + deck_type.joker_slots_modifier()) as usize,
            hand_size: (HAND_SIZE as i32 + deck_type.hand_size_modifier()) as usize,
        }
    }
}

impl Default for DeckConfig {
    fn default() -> Self {
        Self::from_deck_type(DeckType::Standard)
    }
}

// ============================================================================
// 特殊牌組生成函數
// ============================================================================

/// 創建廢棄牌組（無人頭牌）
fn create_abandoned_deck() -> Vec<Card> {
    let mut deck = Vec::new();
    for suit in 0..4u8 {
        for rank in 1..=10u8 {
            // 跳過 J(11), Q(12), K(13)
            deck.push(Card::new(rank, suit));
        }
    }
    deck
}

/// 創建棋盤牌組（26 黑桃 + 26 紅心）
fn create_checkered_deck() -> Vec<Card> {
    let mut deck = Vec::new();
    // 黑桃 (suit = 0)
    for rank in 1..=13u8 {
        deck.push(Card::new(rank, 0));
        deck.push(Card::new(rank, 0));
    }
    // 紅心 (suit = 1)
    for rank in 1..=13u8 {
        deck.push(Card::new(rank, 1));
        deck.push(Card::new(rank, 1));
    }
    deck
}

/// 創建不穩定牌組（隨機點數和花色）
fn create_erratic_deck(rng: &mut StdRng) -> Vec<Card> {
    let mut deck = Vec::new();
    for _ in 0..52 {
        let suit = rng.gen_range(0..4);
        let rank = rng.gen_range(1..=13);
        deck.push(Card::new(rank, suit));
    }
    deck
}

/// 牌組類型總數
pub const DECK_TYPE_COUNT: usize = 12;

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_deck_config() {
        let config = DeckConfig::from_deck_type(DeckType::Standard);
        assert_eq!(config.starting_money, STARTING_MONEY);
        assert_eq!(config.plays_per_blind, PLAYS_PER_BLIND);
        assert_eq!(config.discards_per_blind, DISCARDS_PER_BLIND);
    }

    #[test]
    fn test_yellow_deck_money() {
        let config = DeckConfig::from_deck_type(DeckType::Yellow);
        assert_eq!(config.starting_money, STARTING_MONEY + 10);
    }

    #[test]
    fn test_black_deck_modifiers() {
        let config = DeckConfig::from_deck_type(DeckType::Black);
        assert_eq!(config.joker_slots, JOKER_SLOTS + 1);
        assert_eq!(config.plays_per_blind, PLAYS_PER_BLIND - 1);
    }

    #[test]
    fn test_abandoned_deck() {
        let deck = create_abandoned_deck();
        assert_eq!(deck.len(), 40); // 4 suits * 10 ranks
        for card in &deck {
            assert!(card.rank <= 10);
        }
    }

    #[test]
    fn test_checkered_deck() {
        let deck = create_checkered_deck();
        assert_eq!(deck.len(), 52); // 26 spades + 26 hearts
        let spades = deck.iter().filter(|c| c.suit == 0).count();
        let hearts = deck.iter().filter(|c| c.suit == 1).count();
        assert_eq!(spades, 26);
        assert_eq!(hearts, 26);
    }

    #[test]
    fn test_deck_type_index() {
        for deck_type in DeckType::all() {
            assert!(deck_type.to_index() < DECK_TYPE_COUNT);
        }
    }
}
