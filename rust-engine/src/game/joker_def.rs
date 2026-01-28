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
use super::joker::JOKER_COUNT;

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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
// Joker 定義表
// ============================================================================

/// Joker 靜態定義（元數據）
#[derive(Clone, Copy, Debug)]
pub struct JokerDef {
    /// 基礎購買成本
    pub cost: i64,
    /// 稀有度
    pub rarity: Rarity,
    /// 初始狀態
    pub initial_state: JokerState,
}

// 稀有度縮寫常量
const C: Rarity = Rarity::Common;
const U: Rarity = Rarity::Uncommon;
const R: Rarity = Rarity::Rare;
const L: Rarity = Rarity::Legendary;

// 狀態構造輔助常量
const NONE: JokerState = JokerState::None;
const fn acc(chips: i32, mult: i32, x_mult: f32) -> JokerState {
    JokerState::Accumulator { chips, mult, x_mult }
}
const fn cnt(current: i32, threshold: i32, bonus_mult: f32) -> JokerState {
    JokerState::Counter { current, threshold, bonus_mult }
}
const fn tgt(suit: u8, rank: u8, value: i32) -> JokerState {
    JokerState::Target { suit, rank, value }
}

// JokerDef 構造輔助函數
const fn def(cost: i64, rarity: Rarity, state: JokerState) -> JokerDef {
    JokerDef { cost, rarity, initial_state: state }
}
const fn meta(cost: i64, rarity: Rarity) -> JokerDef {
    JokerDef { cost, rarity, initial_state: NONE }
}

/// 所有 Joker 的靜態定義表
///
/// 索引對應 JokerId 的數值。此表集中管理所有 Joker 的元數據，
/// 用於替代分散在各處的 base_cost() 和 rarity() match 語句。
pub const JOKER_DEFINITIONS: [JokerDef; JOKER_COUNT] = [
    // ========================================================================
    // 0-19: Basic Mult/Chip Jokers
    // ========================================================================
    meta(2, C),     // 0: Joker - +4 Mult
    meta(5, C),     // 1: GreedyJoker - +$3 per Diamond
    meta(5, C),     // 2: LustyJoker - +$3 per Heart
    meta(5, C),     // 3: WrathfulJoker - +$3 per Spade
    meta(5, C),     // 4: GluttonousJoker - +$3 per Club
    meta(4, C),     // 5: JollyJoker - +8 Mult (Pair)
    meta(5, C),     // 6: ZanyJoker - +12 Mult (Three of a Kind)
    meta(5, C),     // 7: MadJoker - +10 Mult (Two Pair)
    meta(5, C),     // 8: CrazyJoker - +12 Mult (Straight)
    meta(5, C),     // 9: DrollJoker - +10 Mult (Flush)
    meta(4, C),     // 10: SlyJoker - +50 Chips (Pair)
    meta(5, C),     // 11: WilyJoker - +100 Chips (Three of a Kind)
    meta(5, C),     // 12: CleverJoker - +80 Chips (Two Pair)
    meta(5, C),     // 13: DeviousJoker - +100 Chips (Straight)
    meta(5, C),     // 14: CraftyJoker - +80 Chips (Flush)
    meta(5, C),     // 15: HalfJoker - +20 Mult if <=3 cards
    meta(5, C),     // 16: Banner - +30 Chips per discard remaining
    meta(6, U),     // 17: MysticSummit - +15 Mult if 0 discards
    meta(4, C),     // 18: Misprint - +?? Mult (0-23)
    meta(6, U),     // 19: AbstractJoker - +3 Mult per Joker

    // ========================================================================
    // 20-39: Multiplier Jokers
    // ========================================================================
    def(6, U, acc(0, 0, 1.0)),  // 20: RideTheBus - +1 Mult per non-face hand
    meta(7, R),                  // 21: SteelJoker - X0.2 per Steel card
    def(6, R, acc(0, 0, 1.0)),  // 22: GlassJoker - X0.75 per Glass broken
    def(5, U, acc(0, 0, 1.0)),  // 23: Hologram - X0.25 per card added
    meta(7, R),                  // 24: FourFingers - 4-card Straights/Flushes
    meta(6, R),                  // 25: Shortcut - Straights skip 1 rank
    meta(7, R),                  // 26: Splash - All cards count
    meta(7, R),                  // 27: Photograph - X2 first Face card
    def(5, C, acc(0, 0, 1.0)),  // 28: GreenJoker - +1 Mult per hand (resets)
    meta(6, U),                  // 29: SuperPosition - X2 if Straight+Flush
    meta(6, U),                  // 30: DuskJoker - X2 on last hand
    meta(6, C),                  // 31: Fibonacci - A/2/3/5/8 +8 Mult
    meta(5, C),                  // 32: ScaryFace - Face +30 Chips
    meta(5, C),                  // 33: EvenSteven - Even +4 Mult
    meta(5, C),                  // 34: OddTodd - Odd +31 Chips
    meta(5, C),                  // 35: Scholar - Ace +20 Chips +4 Mult
    meta(5, U),                  // 36: BusinessCard - Face 1/2 +$2
    meta(6, U),                  // 37: Supernova - +Mult = hands this run
    meta(6, U),                  // 38: Erosion - +4 Mult per card below 52
    meta(6, U),                  // 39: ToTheMoon - +$1 per $5 held

    // ========================================================================
    // 40-59: Economy Jokers
    // ========================================================================
    meta(6, U),                  // 40: GoldenJoker - +$4 end of round
    meta(6, U),                  // 41: Bull - +2 Chips per $1 held
    meta(4, C),                  // 42: Egg - +$3 sell value per round
    meta(5, C),                  // 43: Cartomancer - Tarot on skip
    meta(5, C),                  // 44: Astronomer - Planet on skip
    def(6, U, acc(0, 1, 1.0)),  // 45: Rocket - +$1 per round (scaling)
    meta(5, U),                  // 46: FortuneTeller - +1 Mult per Tarot
    meta(5, C),                  // 47: Faceless - +$5 if 3+ Face discarded
    meta(5, U),                  // 48: SpaceJoker - 1/4 upgrade hand level
    meta(5, C),                  // 49: Vagabond - Tarot if <=4 cards played
    meta(5, C),                  // 50: Stuntman - +250 Chips, -2 hand size
    meta(10, R),                 // 51: Brainstorm - Copy leftmost Joker
    meta(5, C),                  // 52: Satellite - +$1 per unique Planet
    meta(5, C),                  // 53: ShootTheMoon - +13 Mult per Queen
    meta(7, R),                  // 54: Bloodstone - 1/2 X1.5 for Hearts
    meta(7, R),                  // 55: Arrowhead - Spade +50 Chips
    meta(8, R),                  // 56: Onyx - Club +80 Mult
    meta(8, R),                  // 57: Opal - Diamond X1.5
    meta(5, C),                  // 58: Drunkard - +1 discard
    meta(5, R),                  // 59: SteakJoker - X2, -$1 sell per round

    // ========================================================================
    // 60-79: Conditional/Complex Jokers (Part 1)
    // ========================================================================
    def(5, U, acc(100, 0, 1.0)), // 60: IceCream - +100 Chips, -5 per hand
    meta(8, R),                   // 61: DNA - First hand triggers twice
    meta(5, U),                   // 62: BlueJoker - +2 Chips per deck card
    meta(5, R),                   // 63: Sixth - 6 played/discarded = Spectral
    def(5, U, acc(0, 0, 1.0)),   // 64: Constellation - X0.1 per Planet
    meta(5, U),                   // 65: Hiker - +2 Chips per card played
    meta(5, U),                   // 66: CloudNine - +$1 per 9 in deck
    def(5, U, acc(0, 20, 1.0)),  // 67: Popcorn - +20 Mult, -4 per round
    def(5, U, tgt(0, 0, 0)),     // 68: AncientJoker - X1.5 for suit
    def(5, U, acc(0, 0, 2.0)),   // 69: Ramen - X2, -0.01 per discard
    meta(5, U),                   // 70: Walkie - +10 Mult if 10 or 4
    def(5, U, cnt(10, 0, 1.0)),  // 71: Selzer - 10 cards retrigger
    def(5, U, tgt(0, 0, 0)),     // 72: Castle - +3 Chips per suit discard
    meta(5, U),                   // 73: Smiley - Face +5 Mult
    def(5, U, acc(0, 0, 1.0)),   // 74: Campfire - X+0.25 per card sold
    meta(5, R),                   // 75: Ticket - +$1 per Gold card
    meta(5, R),                   // 76: MrBones - Prevent death >25%
    meta(8, R),                   // 77: Acrobat - X3 final hand
    meta(8, R),                   // 78: SockAndBuskin - Retrigger Face
    meta(5, R),                   // 79: Swashbuckler - +2 Mult per low card

    // ========================================================================
    // 80-99: Conditional/Complex Jokers (Part 2)
    // ========================================================================
    meta(5, R),                   // 80: Troubadour - +2 hand size, -1 hand
    meta(5, R),                   // 81: Certificate - +$1 per Gold Seal
    meta(5, R),                   // 82: Smeared - Red/Black same suit
    meta(5, R),                   // 83: Throwback - X0.25 per blind skipped
    meta(5, R),                   // 84: HangingChad - Retrigger first card
    meta(5, R),                   // 85: RoughGem - Diamond +$1
    meta(8, R),                   // 86: Mime - Retrigger held cards
    meta(5, R),                   // 87: CreditCard - -$20 debt allowed
    meta(5, C),                   // 88: Ceremonial - Destroy rightmost Joker
    meta(10, R),                  // 89: Blueprint - Copy right Joker
    def(5, C, acc(0, 0, 1.0)),   // 90: Wee - +8 Chips per round
    def(5, C, acc(0, 0, 1.0)),   // 91: Merry - +3 Mult per round
    meta(5, C),                   // 92: RedCard - +3 Mult per reroll
    def(5, C, acc(0, 0, 0.5)),   // 93: Madness - X0.5, +0.5 per Joker destroyed
    meta(5, C),                   // 94: Square - +4 Chips if 4 cards
    meta(5, C),                   // 95: Seance - Straight Flush = Spectral
    meta(5, C),                   // 96: RiffRaff - 2 Common Jokers on select
    def(8, R, acc(0, 0, 1.0)),   // 97: Vampire - X1, +0.1 per enhancement
    meta(5, C),                   // 98: InvisibleJoker - Sell to duplicate
    meta(8, R),                   // 99: Baron - King held X1.5

    // ========================================================================
    // 100-119: More Complex Jokers (Part 1)
    // ========================================================================
    meta(7, R),                   // 100: Cavendish - X3, 1/1000 self-destruct
    meta(7, R),                   // 101: Card_Sharp - X3 if hand repeated
    meta(5, C),                   // 102: Delayed - +$2 if no discards
    meta(5, C),                   // 103: Hack - Retrigger 2/3/4/5
    meta(5, C),                   // 104: Pareidolia - All cards are Face
    meta(5, C),                   // 105: Gros_Michel - +15 Mult, 1/15 destruct
    meta(5, C),                   // 106: Even_Steven - X2 if only evens
    meta(5, C),                   // 107: Odd_Todd_2 - X2 if only odds
    meta(5, C),                   // 108: Juggler - +1 hand size
    meta(5, C),                   // 109: DriversLicense - X3 if 16+ enhanced
    def(5, C, acc(0, 0, 1.0)),   // 110: Hit_The_Road - X0.5 per Jack discarded
    meta(7, R),                   // 111: The_Duo - X2 if Pair
    meta(7, R),                   // 112: The_Trio - X3 if Three of a Kind
    meta(8, R),                   // 113: The_Family - X4 if Four of a Kind
    meta(7, R),                   // 114: The_Order - X3 if Straight
    meta(7, R),                   // 115: The_Tribe - X2 if Flush
    meta(5, R),                   // 116: Stencil - X1 per empty Joker slot
    meta(5, L),                   // 117: Perkeo - Negative consumable copy
    meta(5, C),                   // 118: Flower_Pot - X3 if all 4 suits
    meta(5, C),                   // 119: BluePrint (reserved)

    // ========================================================================
    // 120-139: More Complex Jokers (Part 2)
    // ========================================================================
    def(5, L, acc(0, 0, 1.0)),   // 120: Canio - X1 per face destroyed
    meta(5, L),                   // 121: Triboulet - K/Q X2
    def(5, L, acc(0, 0, 1.0)),   // 122: Yorick - X1 per 23 discards
    meta(5, L),                   // 123: Chicot - Disable Boss Blind
    meta(5, C),                   // 124: Perkeo_2 (reserved)
    meta(5, C),                   // 125: Seeing_Double - X2 if Club + other
    meta(5, C),                   // 126: Matador - +$8 on Boss trigger
    meta(5, C),                   // 127: Stuntman_2 (reserved)
    meta(5, C),                   // 128: Stone - +25 Chips per Stone card
    def(5, C, acc(0, 0, 1.0)),   // 129: Lucky_Cat - X0.25 per Lucky trigger
    def(5, C, cnt(0, 0, 1.0)),   // 130: Obelisk - X0.2 per non-most-played
    meta(5, C),                   // 131: Runner - +15 Chips if Straight
    meta(5, C),                   // 132: Courier - +25 Chips per low card
    meta(5, C),                   // 133: Cloud9 (reserved)
    meta(5, C),                   // 134: Spare_Trousers - +2 Mult if Two Pair
    meta(5, C),                   // 135: Ring_Master - Jokers can repeat
    meta(5, C),                   // 136: Golden_Ticket - Gold +$3 at round end
    meta(5, C),                   // 137: Rough_Gem_2 (reserved)
    meta(5, C),                   // 138: Bootstraps - +2 Mult per $5
    meta(5, L),                   // 139: Caino - X0.1 per face destroyed

    // ========================================================================
    // 140-163: Utility and Special Jokers
    // ========================================================================
    meta(5, C),                   // 140: Flash - +2 Mult per reroll
    meta(5, C),                   // 141: Trousers - +4 Mult if Two Pair
    meta(5, C),                   // 142: LoyaltyCard - X4 every 6 hands
    meta(5, C),                   // 143: Blackboard - X3 if all black
    def(5, C, cnt(5, 0, 1.0)),   // 144: TurtleBean - +5 hand size, -1/round
    meta(5, C),                   // 145: Burglar - +3 hands, no discards
    meta(5, C),                   // 146: GiftCard - +$1 sell value per round
    meta(5, C),                   // 147: Luchador - Sell to disable Boss
    meta(5, C),                   // 148: ReservedParking - Face 1/2 +$1
    def(5, C, cnt(0, 1, 1.0)),   // 149: TradingCard - First face discard = Tarot
    meta(5, C),                   // 150: MarbleJoker - Add Stone on select
    meta(5, C),                   // 151: MailInRebate - +$5 per K discarded
    meta(5, C),                   // 152: BaseballCard - X1.5 per Uncommon
    meta(5, C),                   // 153: RaisedFist - Lowest card 2X rank Mult
    meta(5, C),                   // 154: EightBall - 8 played = Tarot
    def(5, C, tgt(0, 0, 4)),     // 155: ToDoList - +$4 for target hand type
    meta(5, C),                   // 156: BurntJoker - Upgrade discarded hand
    meta(5, C),                   // 157: MidasMask - Face becomes Gold
    meta(5, C),                   // 158: OopsAll6s - 6s count as all suits
    def(5, C, tgt(0, 1, 0)),     // 159: TheIdol - Target card X2
    meta(5, C),                   // 160: SquareJoker - +4 Mult per card if 52
    meta(5, C),                   // 161: DietCola - Sell for +$100
    def(5, C, cnt(0, 1, 1.0)),   // 162: ChaosTheClown - Free reroll per round
    meta(5, C),                   // 163: Hallucination - Pack = Tarot chance
];

/// 根據 JokerId 索引獲取定義
///
/// # 範例
/// ```
/// use joker_guide::game::joker_def::get_joker_def;
/// let def = get_joker_def(0); // Joker
/// assert_eq!(def.cost, 2);
/// ```
pub const fn get_joker_def(id_index: usize) -> &'static JokerDef {
    &JOKER_DEFINITIONS[id_index]
}

// ============================================================================
// 效果定義表
// ============================================================================

// ============================================================================
// 牌型常量（用於 Conditional 效果）
// ============================================================================

/// 包含 Pair 的牌型（用於 JollyJoker, SlyJoker）
const HANDS_WITH_PAIR: &[HandId] = &[
    HandId::Pair,
    HandId::TwoPair,
    HandId::FullHouse,
    HandId::FlushHouse,
];

/// 包含 Three of a Kind 的牌型（用於 ZanyJoker, WilyJoker）
const HANDS_WITH_THREE_KIND: &[HandId] = &[
    HandId::ThreeKind,
    HandId::FullHouse,
    HandId::FourKind,
    HandId::FiveKind,
    HandId::FlushHouse,
    HandId::FlushFive,
];

/// Two Pair 牌型（用於 MadJoker, CleverJoker）
const HANDS_TWO_PAIR: &[HandId] = &[HandId::TwoPair];

/// 包含 Straight 的牌型（用於 CrazyJoker, DeviousJoker, The_Order）
const HANDS_WITH_STRAIGHT: &[HandId] = &[
    HandId::Straight,
    HandId::StraightFlush,
    HandId::RoyalFlush,
];

/// 包含 Flush 的牌型（用於 DrollJoker, CraftyJoker, The_Tribe）
const HANDS_WITH_FLUSH: &[HandId] = &[
    HandId::Flush,
    HandId::StraightFlush,
    HandId::RoyalFlush,
    HandId::FlushHouse,
    HandId::FlushFive,
];

/// Straight + Flush 牌型（用於 SuperPosition）
const HANDS_STRAIGHT_FLUSH: &[HandId] = &[
    HandId::StraightFlush,
    HandId::RoyalFlush,
];

/// 包含 Four of a Kind 的牌型（用於 The_Family）
const HANDS_WITH_FOUR_KIND: &[HandId] = &[
    HandId::FourKind,
    HandId::FiveKind,
];

/// 包含 Pair 的所有牌型（用於 The_Duo）
const HANDS_WITH_ANY_PAIR: &[HandId] = &[
    HandId::Pair,
    HandId::TwoPair,
    HandId::FullHouse,
    HandId::ThreeKind,
    HandId::FourKind,
    HandId::FiveKind,
];

/// 根據 JokerId 索引獲取效果定義
///
/// 返回該 Joker 的效果模板，用於計算計分效果。
/// 這個函數將逐步替代 `compute_core_joker_effect` 中的 match 語句。
///
/// # 範例
/// ```
/// use joker_guide::game::joker_def::get_effect_def;
/// let effect = get_effect_def(0); // Joker: +4 Mult
/// ```
pub fn get_effect_def(id_index: usize) -> EffectDef {
    match id_index {
        // ====================================================================
        // 2.4.1 固定加成類 Joker (5 個)
        // ====================================================================

        // #71: Joker (0): +4 Mult
        0 => EffectDef::Fixed {
            chips: 0,
            mult: 4,
            x_mult: 1.0,
            money: 0,
        },

        // #72: AbstractJoker (19): +3 Mult per Joker
        // 需要 ScoringContext::joker_count，標記為 Stateful
        19 => EffectDef::Stateful,

        // #73: Bull (41): +2 Chips per $1 held
        // 需要 ScoringContext::money_held，標記為 Stateful
        41 => EffectDef::Stateful,

        // #74: Stuntman (50): +250 Chips
        50 => EffectDef::Fixed {
            chips: 250,
            mult: 0,
            x_mult: 1.0,
            money: 0,
        },

        // #75: GoldenJoker (40): +$4 end of round
        40 => EffectDef::Fixed {
            chips: 0,
            mult: 0,
            x_mult: 1.0,
            money: 4,
        },

        // ====================================================================
        // 2.4.2 牌型條件類 Joker (18 個)
        // ====================================================================

        // #76: JollyJoker (5): +8 Mult (Pair)
        5 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_PAIR),
            bonus: BonusDef::Mult(8),
        },

        // #77: ZanyJoker (6): +12 Mult (Three of a Kind)
        6 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_THREE_KIND),
            bonus: BonusDef::Mult(12),
        },

        // #78: MadJoker (7): +10 Mult (Two Pair)
        7 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_TWO_PAIR),
            bonus: BonusDef::Mult(10),
        },

        // #79: CrazyJoker (8): +12 Mult (Straight)
        8 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_STRAIGHT),
            bonus: BonusDef::Mult(12),
        },

        // #80: DrollJoker (9): +10 Mult (Flush)
        9 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_FLUSH),
            bonus: BonusDef::Mult(10),
        },

        // #81: SlyJoker (10): +50 Chips (Pair)
        10 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_PAIR),
            bonus: BonusDef::Chips(50),
        },

        // #82: WilyJoker (11): +100 Chips (Three of a Kind)
        11 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_THREE_KIND),
            bonus: BonusDef::Chips(100),
        },

        // #83: CleverJoker (12): +80 Chips (Two Pair)
        12 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_TWO_PAIR),
            bonus: BonusDef::Chips(80),
        },

        // #84: DeviousJoker (13): +100 Chips (Straight)
        13 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_STRAIGHT),
            bonus: BonusDef::Chips(100),
        },

        // #85: CraftyJoker (14): +80 Chips (Flush)
        14 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_FLUSH),
            bonus: BonusDef::Chips(80),
        },

        // #86: The_Duo (111): X2 Mult (Pair)
        111 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_ANY_PAIR),
            bonus: BonusDef::XMult(2.0),
        },

        // #87: The_Trio (112): X3 Mult (Three of a Kind)
        112 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_THREE_KIND),
            bonus: BonusDef::XMult(3.0),
        },

        // #88: The_Family (113): X4 Mult (Four of a Kind)
        113 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_FOUR_KIND),
            bonus: BonusDef::XMult(4.0),
        },

        // #89: The_Order (114): X3 Mult (Straight)
        114 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_STRAIGHT),
            bonus: BonusDef::XMult(3.0),
        },

        // #90: The_Tribe (115): X2 Mult (Flush)
        115 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_WITH_FLUSH),
            bonus: BonusDef::XMult(2.0),
        },

        // #91: SuperPosition (29): X2 Mult (Straight+Flush)
        29 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_STRAIGHT_FLUSH),
            bonus: BonusDef::XMult(2.0),
        },

        // #92: Spare_Trousers (134): +2 Mult (Two Pair)
        134 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_TWO_PAIR),
            bonus: BonusDef::Mult(2),
        },

        // #93: Trousers (141): +4 Mult (Two Pair)
        141 => EffectDef::Conditional {
            condition: Condition::HandTypeIn(HANDS_TWO_PAIR),
            bonus: BonusDef::Mult(4),
        },

        // ====================================================================
        // 2.4.3 花色計數類 Joker (11 個)
        // ====================================================================

        // #94: GreedyJoker (1): +$3 per Diamond
        1 => EffectDef::CountBonus {
            filter: CardFilter::Suit(DIAMOND),
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Money(3),
        },

        // #95: LustyJoker (2): +$3 per Heart
        2 => EffectDef::CountBonus {
            filter: CardFilter::Suit(HEART),
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Money(3),
        },

        // #96: WrathfulJoker (3): +$3 per Spade
        3 => EffectDef::CountBonus {
            filter: CardFilter::Suit(SPADE),
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Money(3),
        },

        // #97: GluttonousJoker (4): +$3 per Club
        4 => EffectDef::CountBonus {
            filter: CardFilter::Suit(CLUB),
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Money(3),
        },

        // #98: Arrowhead (55): Spade +50 Chips
        55 => EffectDef::CountBonus {
            filter: CardFilter::Suit(SPADE),
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Chips(50),
        },

        // #99: Onyx (56): Club +80 Mult
        56 => EffectDef::CountBonus {
            filter: CardFilter::Suit(CLUB),
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Mult(80),
        },

        // #100: Opal (57): Diamond X1.5 (uses PowerMultiply)
        57 => EffectDef::PowerMultiply {
            filter: CardFilter::Suit(DIAMOND),
            scope: CardScope::PlayedCards,
            base: 1.5,
        },

        // #101: Bloodstone (54): Heart 1/2 chance X1.5 (Stateful due to RNG)
        54 => EffectDef::Stateful,

        // #102: RoughGem (85): Diamond +$1
        85 => EffectDef::CountBonus {
            filter: CardFilter::Suit(DIAMOND),
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Money(1),
        },

        // #103: Seeing_Double (125): X2 if Club + other suit (Stateful)
        125 => EffectDef::Stateful,

        // #104: Flower_Pot (118): X3 if all 4 suits (Stateful)
        118 => EffectDef::Stateful,

        // ====================================================================
        // 2.4.4 點數計數類 Joker (12 個)
        // ====================================================================

        // #105: Fibonacci (31): A/2/3/5/8 +8 Mult each
        31 => EffectDef::CountBonus {
            filter: CardFilter::RankSet(&[1, 2, 3, 5, 8, 14]), // A=1 or 14, 2, 3, 5, 8
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Mult(8),
        },

        // #106: ScaryFace (32): Face cards +30 Chips each
        32 => EffectDef::CountBonus {
            filter: CardFilter::FaceCard,
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Chips(30),
        },

        // #107: EvenSteven (33): Even cards +4 Mult each
        33 => EffectDef::CountBonus {
            filter: CardFilter::Even,
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Mult(4),
        },

        // #108: OddTodd (34): Odd cards +31 Chips each
        34 => EffectDef::CountBonus {
            filter: CardFilter::Odd,
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Chips(31),
        },

        // #109: Scholar (35): Ace +20 Chips +4 Mult (Stateful - composite)
        35 => EffectDef::Stateful,

        // #110: Smiley (73): Face cards +5 Mult each
        73 => EffectDef::CountBonus {
            filter: CardFilter::FaceCard,
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Mult(5),
        },

        // #111: Walkie (70): 10 or 4 in hand +10 Mult (Stateful)
        70 => EffectDef::Stateful,

        // #112: ShootTheMoon (53): +13 Mult per Queen in hand (Stateful)
        53 => EffectDef::Stateful,

        // #113: Baron (99): King held X1.5 (Stateful - hand cards)
        99 => EffectDef::Stateful,

        // #114: Triboulet (121): K/Q X2 Mult each (Stateful)
        121 => EffectDef::Stateful,

        // #115: Swashbuckler (79): +2 Mult per card below 8 (Stateful)
        79 => EffectDef::Stateful,

        // #116: Courier (132): +25 Chips per low card
        132 => EffectDef::CountBonus {
            filter: CardFilter::LowNumber, // 2, 3, 4, 5
            scope: CardScope::PlayedCards,
            per_card: BonusDef::Chips(25),
        },

        // 其他 Joker 暫時返回默認效果（待實現）
        _ => EffectDef::default(),
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

    #[test]
    fn test_joker_definitions_table() {
        use super::JOKER_COUNT;

        // 驗證表大小
        assert_eq!(JOKER_DEFINITIONS.len(), JOKER_COUNT);

        // 測試一些關鍵 Joker 的定義
        // 0: Joker - cost 2, Common
        let joker = get_joker_def(0);
        assert_eq!(joker.cost, 2);
        assert_eq!(joker.rarity, Rarity::Common);
        assert_eq!(joker.initial_state, JokerState::None);

        // 20: RideTheBus - cost 6, Uncommon, Accumulator
        let ride_the_bus = get_joker_def(20);
        assert_eq!(ride_the_bus.cost, 6);
        assert_eq!(ride_the_bus.rarity, Rarity::Uncommon);
        assert!(matches!(ride_the_bus.initial_state, JokerState::Accumulator { .. }));

        // 69: Ramen - cost 5, Uncommon, Accumulator with x_mult 2.0
        let ramen = get_joker_def(69);
        assert_eq!(ramen.cost, 5);
        assert_eq!(ramen.rarity, Rarity::Uncommon);
        if let JokerState::Accumulator { x_mult, .. } = ramen.initial_state {
            assert!((x_mult - 2.0).abs() < 0.001);
        } else {
            panic!("Ramen should have Accumulator state");
        }

        // 71: Selzer - cost 5, Uncommon, Counter with current=10
        let selzer = get_joker_def(71);
        assert_eq!(selzer.cost, 5);
        assert_eq!(selzer.rarity, Rarity::Uncommon);
        if let JokerState::Counter { current, .. } = selzer.initial_state {
            assert_eq!(current, 10);
        } else {
            panic!("Selzer should have Counter state");
        }

        // 117: Perkeo - Legendary
        let perkeo = get_joker_def(117);
        assert_eq!(perkeo.rarity, Rarity::Legendary);

        // 122: Yorick - Legendary, Accumulator
        let yorick = get_joker_def(122);
        assert_eq!(yorick.rarity, Rarity::Legendary);
        assert!(matches!(yorick.initial_state, JokerState::Accumulator { .. }));
    }

    #[test]
    fn test_effect_def_fixed_joker() {
        use super::get_effect_def;

        // 0: Joker - +4 Mult (固定加成)
        let effect = get_effect_def(0);
        if let EffectDef::Fixed { chips, mult, x_mult, money } = effect {
            assert_eq!(chips, 0);
            assert_eq!(mult, 4);
            assert!((x_mult - 1.0).abs() < 0.001);
            assert_eq!(money, 0);
        } else {
            panic!("Joker should have Fixed effect");
        }
    }

    #[test]
    fn test_effect_def_abstract_joker() {
        use super::get_effect_def;

        // 19: AbstractJoker - +3 Mult per Joker (Stateful)
        let effect = get_effect_def(19);
        assert!(matches!(effect, EffectDef::Stateful));
    }

    #[test]
    fn test_effect_def_bull() {
        use super::get_effect_def;

        // 41: Bull - +2 Chips per $1 held (Stateful)
        let effect = get_effect_def(41);
        assert!(matches!(effect, EffectDef::Stateful));
    }

    #[test]
    fn test_effect_def_stuntman() {
        use super::get_effect_def;

        // 50: Stuntman - +250 Chips (Fixed)
        let effect = get_effect_def(50);
        if let EffectDef::Fixed { chips, mult, x_mult, money } = effect {
            assert_eq!(chips, 250);
            assert_eq!(mult, 0);
            assert!((x_mult - 1.0).abs() < 0.001);
            assert_eq!(money, 0);
        } else {
            panic!("Stuntman should have Fixed effect");
        }
    }

    #[test]
    fn test_effect_def_golden_joker() {
        use super::get_effect_def;

        // 40: GoldenJoker - +$4 end of round (Fixed)
        let effect = get_effect_def(40);
        if let EffectDef::Fixed { chips, mult, x_mult, money } = effect {
            assert_eq!(chips, 0);
            assert_eq!(mult, 0);
            assert!((x_mult - 1.0).abs() < 0.001);
            assert_eq!(money, 4);
        } else {
            panic!("GoldenJoker should have Fixed effect");
        }
    }

    #[test]
    fn test_effect_def_jolly_joker() {
        use super::get_effect_def;

        // 5: JollyJoker - +8 Mult on Pair hands (Conditional)
        let effect = get_effect_def(5);
        if let EffectDef::Conditional { condition, bonus } = effect {
            assert!(matches!(condition, Condition::HandTypeIn(_)));
            assert!(matches!(bonus, BonusDef::Mult(8)));
        } else {
            panic!("JollyJoker should have Conditional effect");
        }
    }
}
