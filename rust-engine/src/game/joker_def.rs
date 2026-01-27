//! Joker 定義系統 - 聲明式效果模板
//!
//! 本模組提供 Joker 效果的聲明式定義系統，取代原有的巨型 match 語句。
//!
//! # 架構
//!
//! ```text
//! JokerDef = 元數據 + 效果定義 + 初始狀態 + 觸發器
//!
//! 效果定義 = 基礎效果類型 × 觸發條件 × 作用目標
//! ```
//!
//! # 添加新 Joker
//!
//! 只需在 `JOKER_DEFINITIONS` 添加一個條目：
//!
//! ```rust,ignore
//! JokerDef {
//!     id: JokerId::NewJoker,
//!     cost: 6,
//!     rarity: Rarity::Uncommon,
//!     effect: EffectDef::CountBonus {
//!         filter: CardFilter::Suit(DIAMOND),
//!         per_card: BonusDef::Money(3),
//!     },
//!     initial_state: JokerState::None,
//!     triggers: &[],
//! }
//! ```

use super::hand_types::HandId;

// ============================================================================
// 花色常量
// ============================================================================

pub const SPADE: u8 = 0;
pub const DIAMOND: u8 = 1;
pub const HEART: u8 = 2;
pub const CLUB: u8 = 3;

// ============================================================================
// Joker 狀態系統
// ============================================================================

/// Joker 統一狀態類型
///
/// 取代原有 JokerSlot 中 30+ 個專屬狀態欄位，精簡為 4 種通用狀態。
/// 記憶體佔用從 ~200 bytes 降至 ~16 bytes。
#[derive(Clone, Debug, Default, PartialEq)]
pub enum JokerState {
    /// 無狀態（大多數 Joker）
    #[default]
    None,

    /// 累加器狀態
    ///
    /// 用於：Vampire, Canio, GlassJoker, Hologram, Constellation,
    /// Lucky_Cat, Madness, Hit_The_Road, Campfire, Wee, Merry,
    /// GreenJoker, RideTheBus, IceCream, Popcorn, Ramen, Rocket
    Accumulator {
        chips: i32,
        mult: i32,
        x_mult: f32,
    },

    /// 計數觸發器狀態
    ///
    /// 用於：Yorick, Obelisk, Selzer, TurtleBean, LoyaltyCard,
    /// ChaosTheClown, TradingCard, InvisibleJoker
    Counter {
        current: i32,
        threshold: i32,
        bonus_mult: f32,
    },

    /// 目標狀態（花色/點數目標）
    ///
    /// 用於：AncientJoker, Castle, TheIdol, ToDoList
    Target {
        suit: u8,
        rank: u8,
        value: i32,
    },
}

impl JokerState {
    /// 創建累加器狀態
    pub fn accumulator(chips: i32, mult: i32, x_mult: f32) -> Self {
        Self::Accumulator { chips, mult, x_mult }
    }

    /// 創建計數器狀態
    pub fn counter(current: i32, threshold: i32, bonus_mult: f32) -> Self {
        Self::Counter { current, threshold, bonus_mult }
    }

    /// 創建目標狀態
    pub fn target(suit: u8, rank: u8, value: i32) -> Self {
        Self::Target { suit, rank, value }
    }

    /// 獲取 x_mult 值（如果適用）
    pub fn get_x_mult(&self) -> f32 {
        match self {
            Self::Accumulator { x_mult, .. } => *x_mult,
            Self::Counter { bonus_mult, .. } => *bonus_mult,
            _ => 1.0,
        }
    }

    /// 獲取 mult 值（如果適用）
    pub fn get_mult(&self) -> i32 {
        match self {
            Self::Accumulator { mult, .. } => *mult,
            _ => 0,
        }
    }

    /// 獲取 chips 值（如果適用）
    pub fn get_chips(&self) -> i32 {
        match self {
            Self::Accumulator { chips, .. } => *chips,
            _ => 0,
        }
    }

    /// 獲取計數器當前值（如果適用）
    pub fn get_counter(&self) -> i32 {
        match self {
            Self::Counter { current, .. } => *current,
            _ => 0,
        }
    }

    /// 增加 x_mult
    pub fn add_x_mult(&mut self, amount: f32) {
        match self {
            Self::Accumulator { x_mult, .. } => *x_mult += amount,
            Self::Counter { bonus_mult, .. } => *bonus_mult += amount,
            _ => {}
        }
    }

    /// 增加 mult
    pub fn add_mult(&mut self, amount: i32) {
        if let Self::Accumulator { mult, .. } = self {
            *mult += amount;
        }
    }

    /// 增加 chips
    pub fn add_chips(&mut self, amount: i32) {
        if let Self::Accumulator { chips, .. } = self {
            *chips += amount;
        }
    }

    /// 增加計數器
    pub fn increment_counter(&mut self) -> bool {
        if let Self::Counter { current, threshold, bonus_mult } = self {
            *current += 1;
            if *current >= *threshold && *threshold > 0 {
                *current = 0;
                *bonus_mult += 1.0;
                return true; // 達到閾值
            }
        }
        false
    }

    /// 重置計數器
    pub fn reset_counter(&mut self) {
        if let Self::Counter { current, .. } = self {
            *current = 0;
        }
    }

    /// 設置目標花色
    pub fn set_target_suit(&mut self, suit: u8) {
        if let Self::Target { suit: s, .. } = self {
            *s = suit;
        }
    }

    /// 設置目標點數
    pub fn set_target_rank(&mut self, rank: u8) {
        if let Self::Target { rank: r, .. } = self {
            *r = rank;
        }
    }

    /// 獲取目標花色（如果適用）
    pub fn get_target_suit(&self) -> u8 {
        match self {
            Self::Target { suit, .. } => *suit,
            _ => 0,
        }
    }

    /// 獲取目標點數（如果適用）
    pub fn get_target_rank(&self) -> u8 {
        match self {
            Self::Target { rank, .. } => *rank,
            _ => 0,
        }
    }

    /// 獲取目標值（如果適用）
    pub fn get_target_value(&self) -> i32 {
        match self {
            Self::Target { value, .. } => *value,
            _ => 0,
        }
    }

    /// 增加目標值
    pub fn add_target_value(&mut self, amount: i32) {
        if let Self::Target { value, .. } = self {
            *value += amount;
        }
    }

    /// 設置目標值
    pub fn set_target_value(&mut self, new_value: i32) {
        if let Self::Target { value, .. } = self {
            *value = new_value;
        }
    }
}

// ============================================================================
// 卡牌過濾器
// ============================================================================

/// 卡牌過濾條件
#[derive(Clone, Debug, PartialEq)]
pub enum CardFilter {
    /// 特定花色
    Suit(u8),

    /// 點數範圍（包含）
    RankRange { min: u8, max: u8 },

    /// 點數集合
    RankSet(&'static [u8]),

    /// 人頭牌（J, Q, K）
    FaceCard,

    /// 偶數牌（2, 4, 6, 8, 10）
    Even,

    /// 奇數牌（A, 3, 5, 7, 9）
    Odd,

    /// 任意卡牌
    Any,

    /// 低數字（2, 3, 4, 5）
    LowNumber,
}

impl CardFilter {
    /// 檢查卡牌是否匹配過濾條件
    pub fn matches(&self, suit: u8, rank: u8) -> bool {
        match self {
            Self::Suit(s) => suit == *s,
            Self::RankRange { min, max } => rank >= *min && rank <= *max,
            Self::RankSet(ranks) => ranks.contains(&rank),
            Self::FaceCard => rank >= 11 && rank <= 13, // J, Q, K
            Self::Even => matches!(rank, 2 | 4 | 6 | 8 | 10),
            Self::Odd => matches!(rank, 1 | 3 | 5 | 7 | 9 | 14), // A=1 or 14
            Self::Any => true,
            Self::LowNumber => rank >= 2 && rank <= 5,
        }
    }
}

// ============================================================================
// 條件系統
// ============================================================================

/// 效果觸發條件
#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    /// 無條件觸發
    Always,

    /// 牌型匹配
    HandTypeIn(&'static [HandId]),

    /// 出牌數量條件
    PlayedCardCount {
        min: Option<usize>,
        max: Option<usize>,
    },

    /// 時機條件
    Timing {
        first_hand: bool,
        final_hand: bool,
    },

    /// 狀態閾值
    StateThreshold {
        field: StateField,
        op: CompareOp,
        value: i64,
    },
}

/// 遊戲狀態欄位
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateField {
    DiscardsRemaining,
    Money,
    JokerCount,
    DeckSize,
    HandsPlayedThisRound,
    HandsPlayedThisRun,
    EnhancedCardsInDeck,
}

/// 比較運算符
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Gt,
    Lt,
    Gte,
    Lte,
}

// ============================================================================
// 獎勵定義
// ============================================================================

/// 獎勵類型定義
#[derive(Clone, Debug, PartialEq)]
pub enum BonusDef {
    /// 固定 Chips
    Chips(i64),

    /// 固定加法 Mult
    Mult(i64),

    /// 固定乘法 Mult
    XMult(f32),

    /// 固定金幣
    Money(i64),

    /// 複合獎勵
    Composite {
        chips: i64,
        mult: i64,
        x_mult: f32,
        money: i64,
    },
}

impl BonusDef {
    /// 縮放獎勵值
    pub fn scale(&self, multiplier: i64) -> JokerBonus {
        match self {
            Self::Chips(v) => JokerBonus {
                chip_bonus: v * multiplier,
                ..Default::default()
            },
            Self::Mult(v) => JokerBonus {
                add_mult: v * multiplier,
                ..Default::default()
            },
            Self::XMult(v) => JokerBonus {
                mul_mult: *v,
                ..Default::default()
            },
            Self::Money(v) => JokerBonus {
                money_bonus: v * multiplier,
                ..Default::default()
            },
            Self::Composite { chips, mult, x_mult, money } => JokerBonus {
                chip_bonus: chips * multiplier,
                add_mult: mult * multiplier,
                mul_mult: *x_mult,
                money_bonus: money * multiplier,
                retriggers: 0,
            },
        }
    }

    /// 轉換為 JokerBonus
    pub fn to_bonus(&self) -> JokerBonus {
        self.scale(1)
    }
}

// ============================================================================
// 效果定義
// ============================================================================

/// Joker 效果定義
#[derive(Clone, Debug)]
pub enum EffectDef {
    /// 固定加成（無條件）
    Fixed {
        chips: i64,
        mult: i64,
        x_mult: f32,
        money: i64,
    },

    /// 計數加成（每張符合條件的牌）
    CountBonus {
        filter: CardFilter,
        scope: CardScope,
        per_card: BonusDef,
    },

    /// 條件觸發
    Conditional {
        condition: Condition,
        bonus: BonusDef,
    },

    /// 狀態相關（需要讀取 JokerState 或 ScoringContext）
    Stateful,

    /// 重觸發
    Retrigger {
        filter: CardFilter,
        count: i32,
    },

    /// 規則修改（不直接加分）
    RuleModifier,

    /// 指數乘法（base ^ count）
    PowerMultiply {
        filter: CardFilter,
        scope: CardScope,
        base: f32,
    },
}

impl Default for EffectDef {
    fn default() -> Self {
        Self::Fixed {
            chips: 0,
            mult: 0,
            x_mult: 1.0,
            money: 0,
        }
    }
}

/// 卡牌作用域
#[derive(Clone, Debug, PartialEq, Default)]
pub enum CardScope {
    /// 出牌中的卡牌
    #[default]
    PlayedCards,

    /// 手牌（未出）
    HandCards,

    /// 全牌組
    DeckCards,
}

// ============================================================================
// 遊戲事件（用於觸發器）
// ============================================================================

/// 遊戲事件類型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameEvent {
    // 回合開始
    BlindSelected,
    BlindSkipped,

    // 出牌/棄牌
    HandPlayed,
    CardDiscarded,

    // 計分相關
    EnhancementAbsorbed,
    FaceCardDestroyed,
    GlassCardBroken,
    CardAddedToDeck,
    LuckyTriggered,

    // 消耗品
    TarotUsed,
    PlanetUsed,
    SpectralUsed,

    // 回合結束
    BlindCleared,
    BossBlindCleared,
    RoundEnded,

    // 商店
    JokerSold,
    CardSold,
    Rerolled,
    JokerPurchased,
    PackOpened,
}

// ============================================================================
// Joker Bonus 輸出
// ============================================================================

/// Joker 效果輸出
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JokerBonus {
    pub chip_bonus: i64,
    pub add_mult: i64,
    pub mul_mult: f32,
    pub money_bonus: i64,
    pub retriggers: i32,
}

impl JokerBonus {
    /// 創建新的空獎勵
    pub fn new() -> Self {
        Self {
            chip_bonus: 0,
            add_mult: 0,
            mul_mult: 1.0,
            money_bonus: 0,
            retriggers: 0,
        }
    }

    /// 合併兩個獎勵
    pub fn merge(&mut self, other: &JokerBonus) {
        self.chip_bonus += other.chip_bonus;
        self.add_mult += other.add_mult;
        self.mul_mult *= other.mul_mult;
        self.money_bonus += other.money_bonus;
        self.retriggers += other.retriggers;
    }

    /// 檢查是否為空獎勵
    pub fn is_empty(&self) -> bool {
        self.chip_bonus == 0
            && self.add_mult == 0
            && (self.mul_mult - 1.0).abs() < f32::EPSILON
            && self.money_bonus == 0
            && self.retriggers == 0
    }
}

// ============================================================================
// 稀有度
// ============================================================================

/// Joker 稀有度
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rarity {
    Common = 1,
    Uncommon = 2,
    Rare = 3,
    Legendary = 4,
}

impl From<u8> for Rarity {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Common,
            2 => Self::Uncommon,
            3 => Self::Rare,
            4 => Self::Legendary,
            _ => Self::Common,
        }
    }
}

impl From<Rarity> for u8 {
    fn from(value: Rarity) -> Self {
        value as u8
    }
}

// ============================================================================
// 觸發器定義
// ============================================================================

/// 觸發器上下文（傳遞給觸發器處理函數）
#[derive(Debug)]
pub struct TriggerContext<'a> {
    /// 隨機數值（0-255）
    pub rng_value: u8,

    /// 當前金幣
    pub money: i64,

    /// 棄牌中的人頭牌數量
    pub discarded_face_count: i32,

    /// 棄牌中特定花色的數量
    pub discarded_suit_count: [i32; 4],

    /// 棄牌總數
    pub discarded_count: i32,

    /// 是否是 Boss Blind
    pub is_boss_blind: bool,

    /// 額外數據（可選）
    pub extra: Option<&'a dyn std::any::Any>,
}

impl Default for TriggerContext<'_> {
    fn default() -> Self {
        Self {
            rng_value: 0,
            money: 0,
            discarded_face_count: 0,
            discarded_suit_count: [0; 4],
            discarded_count: 0,
            is_boss_blind: false,
            extra: None,
        }
    }
}

// ============================================================================
// 測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_joker_state_accumulator() {
        let mut state = JokerState::accumulator(0, 0, 1.0);
        assert_eq!(state.get_x_mult(), 1.0);

        state.add_x_mult(0.1);
        assert!((state.get_x_mult() - 1.1).abs() < 0.001);

        state.add_mult(5);
        assert_eq!(state.get_mult(), 5);

        state.add_chips(100);
        assert_eq!(state.get_chips(), 100);
    }

    #[test]
    fn test_joker_state_counter() {
        let mut state = JokerState::counter(0, 5, 1.0);
        assert_eq!(state.get_counter(), 0);

        // 增加到閾值
        for _ in 0..4 {
            assert!(!state.increment_counter());
        }
        assert!(state.increment_counter()); // 第 5 次達到閾值
        assert_eq!(state.get_counter(), 0); // 重置
        assert!((state.get_x_mult() - 2.0).abs() < 0.001); // bonus_mult 增加
    }

    #[test]
    fn test_card_filter() {
        // 測試花色過濾
        assert!(CardFilter::Suit(HEART).matches(HEART, 5));
        assert!(!CardFilter::Suit(HEART).matches(SPADE, 5));

        // 測試人頭牌
        assert!(CardFilter::FaceCard.matches(HEART, 11)); // J
        assert!(CardFilter::FaceCard.matches(HEART, 12)); // Q
        assert!(CardFilter::FaceCard.matches(HEART, 13)); // K
        assert!(!CardFilter::FaceCard.matches(HEART, 10));

        // 測試偶數
        assert!(CardFilter::Even.matches(HEART, 2));
        assert!(CardFilter::Even.matches(HEART, 10));
        assert!(!CardFilter::Even.matches(HEART, 3));

        // 測試奇數
        assert!(CardFilter::Odd.matches(HEART, 1)); // A
        assert!(CardFilter::Odd.matches(HEART, 9));
        assert!(!CardFilter::Odd.matches(HEART, 2));
    }

    #[test]
    fn test_bonus_def_scale() {
        let bonus = BonusDef::Chips(10).scale(5);
        assert_eq!(bonus.chip_bonus, 50);

        let bonus = BonusDef::Mult(3).scale(4);
        assert_eq!(bonus.add_mult, 12);

        let bonus = BonusDef::Money(2).scale(3);
        assert_eq!(bonus.money_bonus, 6);
    }

    #[test]
    fn test_joker_bonus_merge() {
        let mut bonus1 = JokerBonus {
            chip_bonus: 100,
            add_mult: 10,
            mul_mult: 2.0,
            money_bonus: 5,
            retriggers: 1,
        };

        let bonus2 = JokerBonus {
            chip_bonus: 50,
            add_mult: 5,
            mul_mult: 1.5,
            money_bonus: 3,
            retriggers: 2,
        };

        bonus1.merge(&bonus2);

        assert_eq!(bonus1.chip_bonus, 150);
        assert_eq!(bonus1.add_mult, 15);
        assert!((bonus1.mul_mult - 3.0).abs() < 0.001);
        assert_eq!(bonus1.money_bonus, 8);
        assert_eq!(bonus1.retriggers, 3);
    }
}
