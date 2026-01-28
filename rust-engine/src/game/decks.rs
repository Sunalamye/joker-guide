//! 起始牌組系統
//!
//! 定義不同類型的起始牌組，每種都有獨特的效果
//!
//! # 架構
//!
//! 使用聲明式 `DECK_DEFS` 表定義所有牌組的元數據。

use rand::prelude::*;
use rand::rngs::StdRng;

use super::cards::{Card, standard_deck};
use super::constants::{DISCARDS_PER_BLIND, HAND_SIZE, JOKER_SLOTS, PLAYS_PER_BLIND, STARTING_MONEY};

// ============================================================================
// Deck 定義系統
// ============================================================================

/// Deck 定義結構
#[derive(Clone, Copy)]
pub struct DeckDef {
    pub name: &'static str,
    pub starting_money_mod: i64,
    pub plays_mod: i32,
    pub discards_mod: i32,
    pub joker_slots_mod: i32,
    pub hand_size_mod: i32,
    pub consumable_slots_mod: i32,
    pub disables_interest: bool,
    pub money_per_remaining_hand: i64,
    pub uses_plasma_scoring: bool,
    pub spectral_rate_mult: f32,
    pub gives_double_tag_after_boss: bool,
}

/// Deck 定義表（順序與 DeckType 枚舉一致）
pub static DECK_DEFS: [DeckDef; 16] = [
    // 0: Standard
    DeckDef { name: "Standard Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 1: Red - +1 discard
    DeckDef { name: "Red Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 1, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 2: Blue - +1 play
    DeckDef { name: "Blue Deck", starting_money_mod: 0, plays_mod: 1, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 3: Yellow - +$10
    DeckDef { name: "Yellow Deck", starting_money_mod: 10, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 4: Green - no interest, $2 per remaining hand
    DeckDef { name: "Green Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: true, money_per_remaining_hand: 2, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 5: Black - +1 joker slot, -1 play
    DeckDef { name: "Black Deck", starting_money_mod: 0, plays_mod: -1, discards_mod: 0, joker_slots_mod: 1, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 6: Ghost - x2 spectral rate
    DeckDef { name: "Ghost Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 2.0, gives_double_tag_after_boss: false },
    // 7: Abandoned - no face cards (handled in create_deck)
    DeckDef { name: "Abandoned Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 8: Checkered - 26 spades + 26 hearts (handled in create_deck)
    DeckDef { name: "Checkered Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 9: Painted - +2 hand size, -1 joker slot
    DeckDef { name: "Painted Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: -1, hand_size_mod: 2, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 10: Plasma - merged chips/mult scoring
    DeckDef { name: "Plasma Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: true, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 11: Erratic - random cards (handled in create_deck)
    DeckDef { name: "Erratic Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 12: Magic - Crystal Ball voucher + 2 Fool tarots (handled in starting_vouchers/consumables)
    DeckDef { name: "Magic Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 13: Nebula - Telescope voucher, -1 consumable slot
    DeckDef { name: "Nebula Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: -1, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 14: Zodiac - 3 merchant vouchers (handled in starting_vouchers)
    DeckDef { name: "Zodiac Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: false },
    // 15: Anaglyph - Double Tag after boss
    DeckDef { name: "Anaglyph Deck", starting_money_mod: 0, plays_mod: 0, discards_mod: 0, joker_slots_mod: 0, hand_size_mod: 0, consumable_slots_mod: 0, disables_interest: false, money_per_remaining_hand: 1, uses_plasma_scoring: false, spectral_rate_mult: 1.0, gives_double_tag_after_boss: true },
];

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

    /// 魔法牌組：起始 Crystal Ball Voucher，2 張 Fool Tarot
    Magic,

    /// 星雲牌組：起始 Telescope Voucher，-1 consumable slot
    Nebula,

    /// 黃道牌組：起始 3 個商人 Voucher (Tarot/Planet/Card Merchant)
    Zodiac,

    /// 立體牌組：每次 Boss 過關後獲得 Double Tag
    Anaglyph,
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
            DeckType::Magic,
            DeckType::Nebula,
            DeckType::Zodiac,
            DeckType::Anaglyph,
        ]
    }

    /// 牌組名稱
    pub fn name(&self) -> &'static str {
        DECK_DEFS[self.to_index()].name
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
        DECK_DEFS[self.to_index()].starting_money_mod
    }

    /// 每回合出牌次數修正
    pub fn plays_per_blind_modifier(&self) -> i32 {
        DECK_DEFS[self.to_index()].plays_mod
    }

    /// 每回合棄牌次數修正
    pub fn discards_per_blind_modifier(&self) -> i32 {
        DECK_DEFS[self.to_index()].discards_mod
    }

    /// Joker 欄位修正
    pub fn joker_slots_modifier(&self) -> i32 {
        DECK_DEFS[self.to_index()].joker_slots_mod
    }

    /// 手牌大小修正
    pub fn hand_size_modifier(&self) -> i32 {
        DECK_DEFS[self.to_index()].hand_size_mod
    }

    /// 是否禁用利息（Green Deck）
    pub fn disables_interest(&self) -> bool {
        DECK_DEFS[self.to_index()].disables_interest
    }

    /// 每剩餘手牌的額外金幣（Green Deck）
    pub fn money_per_remaining_hand(&self) -> i64 {
        DECK_DEFS[self.to_index()].money_per_remaining_hand
    }

    /// 是否使用 Plasma 計分模式
    pub fn uses_plasma_scoring(&self) -> bool {
        DECK_DEFS[self.to_index()].uses_plasma_scoring
    }

    /// Spectral 出現率倍數（Ghost Deck: x2）
    pub fn spectral_rate_mult(&self) -> f32 {
        DECK_DEFS[self.to_index()].spectral_rate_mult
    }

    /// to_index 用於 observation
    pub fn to_index(&self) -> usize {
        Self::all().iter().position(|d| d == self).unwrap_or(0)
    }

    /// Consumable slot 修正
    pub fn consumable_slots_modifier(&self) -> i32 {
        DECK_DEFS[self.to_index()].consumable_slots_mod
    }

    /// 起始 Voucher 列表
    pub fn starting_vouchers(&self) -> Vec<super::VoucherId> {
        match self {
            DeckType::Magic => vec![super::VoucherId::CrystalBall],
            DeckType::Nebula => vec![super::VoucherId::Telescope],
            DeckType::Zodiac => vec![
                super::VoucherId::Tarot_Merchant,
                super::VoucherId::Planet_Merchant,
                super::VoucherId::Overstock,
            ],
            _ => vec![],
        }
    }

    /// 是否在 Boss 過關後獲得 Double Tag
    pub fn gives_double_tag_after_boss(&self) -> bool {
        DECK_DEFS[self.to_index()].gives_double_tag_after_boss
    }

    /// 起始消耗品 (Tarot ID 列表)
    pub fn starting_consumables(&self) -> Vec<super::TarotId> {
        match self {
            DeckType::Magic => vec![
                super::TarotId::TheFool,
                super::TarotId::TheFool,
            ],
            _ => vec![],
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
            assert!(deck_type.to_index() < DeckType::all().len());
        }
    }
}
