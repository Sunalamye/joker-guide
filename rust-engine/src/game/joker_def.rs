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
use super::cards::Card;

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
// 效果計算上下文
// ============================================================================

/// 計算效果所需的上下文（簡化版）
///
/// 這個結構只包含 `compute_effect` 函數所需的字段，
/// 可以從 `joker::ScoringContext` 輕鬆轉換。
#[derive(Clone, Debug)]
pub struct ComputeContext<'a> {
    /// 打出的牌
    pub played_cards: &'a [Card],
    /// 手中持有的牌
    pub hand: &'a [Card],
    /// 牌型
    pub hand_id: HandId,
    /// 是否是第一手
    pub is_first_hand: bool,
    /// 是否是最後一手
    pub is_final_hand: bool,
}

impl<'a> ComputeContext<'a> {
    pub fn new(played_cards: &'a [Card], hand: &'a [Card], hand_id: HandId) -> Self {
        Self {
            played_cards,
            hand,
            hand_id,
            is_first_hand: false,
            is_final_hand: false,
        }
    }
}

// ============================================================================
// 效果計算函數
// ============================================================================

/// 根據效果定義計算 Joker 獎勵
///
/// 這個函數處理非 Stateful 的效果定義，返回計算後的 JokerBonus。
/// Stateful 效果需要額外的狀態信息，應該在調用者處單獨處理。
///
/// # 範例
/// ```ignore
/// let effect = get_effect_def(0); // Joker: +4 Mult
/// let ctx = ComputeContext::new(&played_cards, &hand, hand_id);
/// let bonus = compute_effect(&effect, &ctx);
/// ```
pub fn compute_effect(effect: &EffectDef, ctx: &ComputeContext) -> JokerBonus {
    let mut bonus = JokerBonus::new();

    match effect {
        // 固定加成 - 直接返回獎勵
        EffectDef::Fixed { chips, mult, x_mult, money } => {
            bonus.chip_bonus = *chips;
            bonus.add_mult = *mult;
            bonus.mul_mult = *x_mult;
            bonus.money_bonus = *money;
        }

        // 計數加成 - 計算符合條件的牌數量
        EffectDef::CountBonus { filter, scope, per_card } => {
            let cards = match scope {
                CardScope::PlayedCards => ctx.played_cards,
                CardScope::HandCards => ctx.hand,
                CardScope::DeckCards => &[], // 需要額外信息，暫不支持
            };

            let count = cards
                .iter()
                .filter(|card| filter.matches(card.suit, card.rank))
                .count() as i64;

            if count > 0 {
                let scaled = per_card.scale(count);
                bonus.merge(&scaled);
            }
        }

        // 條件觸發 - 檢查條件是否滿足
        EffectDef::Conditional { condition, bonus: bonus_def } => {
            let should_trigger = match condition {
                Condition::Always => true,
                Condition::HandTypeIn(hand_types) => hand_types.contains(&ctx.hand_id),
                Condition::PlayedCardCount { min, max } => {
                    let count = ctx.played_cards.len();
                    min.map_or(true, |m| count >= m) && max.map_or(true, |m| count <= m)
                }
                Condition::Timing { first_hand, final_hand } => {
                    (*first_hand && ctx.is_first_hand) || (*final_hand && ctx.is_final_hand)
                }
                Condition::StateThreshold { .. } => false, // 需要額外狀態
            };

            if should_trigger {
                let scaled = bonus_def.scale(1);
                bonus.merge(&scaled);
            }
        }

        // 指數乘法 - 計算 base ^ count
        EffectDef::PowerMultiply { filter, scope, base } => {
            let cards = match scope {
                CardScope::PlayedCards => ctx.played_cards,
                CardScope::HandCards => ctx.hand,
                CardScope::DeckCards => &[],
            };

            let count = cards
                .iter()
                .filter(|card| filter.matches(card.suit, card.rank))
                .count();

            if count > 0 {
                bonus.mul_mult = base.powi(count as i32);
            }
        }

        // 重觸發 - 計算重觸發次數
        EffectDef::Retrigger { filter, count } => {
            let retrigger_count = ctx.played_cards
                .iter()
                .filter(|card| filter.matches(card.suit, card.rank))
                .count() as i32;

            bonus.retriggers = retrigger_count * count;
        }

        // 規則修改 - 不直接提供獎勵
        EffectDef::RuleModifier => {
            // 規則修改在其他地方處理，這裡不返回獎勵
        }

        // 狀態相關 - 需要額外狀態信息
        EffectDef::Stateful => {
            // Stateful 效果需要在調用者處單獨處理
            // 返回空獎勵
        }
    }

    bonus
}

// ============================================================================
// V2 效果計算（整合狀態處理）
// ============================================================================

/// 擴展計算上下文，包含完整的遊戲狀態
///
/// 這個結構從 `joker::ScoringContext` 轉換，包含計算 Stateful 效果所需的所有信息。
#[derive(Clone, Debug)]
pub struct ComputeContextV2<'a> {
    /// 打出的牌
    pub played_cards: &'a [Card],
    /// 手中持有的牌
    pub hand: &'a [Card],
    /// 牌型
    pub hand_id: HandId,
    /// 是否是第一手
    pub is_first_hand: bool,
    /// 是否是最後一手
    pub is_final_hand: bool,
    /// 持有金幣數量
    pub money_held: i64,
    /// Joker 數量
    pub joker_count: usize,
    /// Joker 槽位上限
    pub joker_slot_limit: usize,
    /// 剩餘棄牌數
    pub discards_remaining: i32,
    /// 本輪已打手數
    pub hands_played_this_round: i32,
    /// 本局已打手數
    pub hands_played_this_run: i32,
    /// 牌組大小
    pub deck_size: i32,
    /// 牌組中增強牌數量
    pub enhanced_cards_in_deck: i32,
    /// Uncommon Joker 數量
    pub uncommon_joker_count: usize,
}

impl<'a> ComputeContextV2<'a> {
    /// 從簡化上下文創建
    pub fn from_basic(ctx: &ComputeContext<'a>) -> Self {
        Self {
            played_cards: ctx.played_cards,
            hand: ctx.hand,
            hand_id: ctx.hand_id,
            is_first_hand: ctx.is_first_hand,
            is_final_hand: ctx.is_final_hand,
            money_held: 0,
            joker_count: 0,
            joker_slot_limit: 5,
            discards_remaining: 0,
            hands_played_this_round: 0,
            hands_played_this_run: 0,
            deck_size: 52,
            enhanced_cards_in_deck: 0,
            uncommon_joker_count: 0,
        }
    }

    /// 轉換為基本上下文
    pub fn to_basic(&self) -> ComputeContext<'a> {
        ComputeContext {
            played_cards: self.played_cards,
            hand: self.hand,
            hand_id: self.hand_id,
            is_first_hand: self.is_first_hand,
            is_final_hand: self.is_final_hand,
        }
    }
}

/// 使用新模板系統計算 Joker 效果 (V2)
///
/// 這個函數整合了效果定義和狀態處理，逐步替代 `compute_core_joker_effect`。
///
/// # 參數
/// - `joker_index`: Joker 在 JOKER_DEFINITIONS 中的索引
/// - `state`: Joker 的當前狀態
/// - `ctx`: 計算上下文
/// - `_rng_value`: 隨機數值（供隨機效果使用）
///
/// # 範例
/// ```ignore
/// let bonus = compute_joker_effect_v2(0, &JokerState::None, &ctx, 0);
/// assert_eq!(bonus.add_mult, 4); // Joker: +4 Mult
/// ```
pub fn compute_joker_effect_v2(
    joker_index: usize,
    state: &JokerState,
    ctx: &ComputeContextV2,
    _rng_value: u8,
) -> JokerBonus {
    let effect = get_effect_def(joker_index);
    let basic_ctx = ctx.to_basic();

    // 非 Stateful 效果直接使用 compute_effect
    match &effect {
        EffectDef::Stateful => {
            // Stateful 效果需要根據 joker_index 和 state 單獨處理
            compute_stateful_effect(joker_index, state, ctx)
        }
        _ => compute_effect(&effect, &basic_ctx),
    }
}

/// 計算 Stateful 效果
///
/// 這些效果依賴於 JokerState 或 ComputeContextV2 中的擴展信息。
fn compute_stateful_effect(
    joker_index: usize,
    state: &JokerState,
    ctx: &ComputeContextV2,
) -> JokerBonus {
    let mut bonus = JokerBonus::new();

    match joker_index {
        // === 累加器效果 ===
        // RideTheBus (20): 使用累積的 mult
        20 => bonus.add_mult = state.get_mult() as i64,

        // GreenJoker (28): 使用累積的 mult
        28 => bonus.add_mult = state.get_mult() as i64,

        // IceCream (60): 使用累積的 chips
        60 => bonus.chip_bonus = state.get_chips() as i64,

        // Popcorn (67): 使用累積的 mult
        67 => bonus.add_mult = state.get_mult() as i64,

        // Ramen (69): 使用累積的 x_mult
        69 => bonus.mul_mult = state.get_x_mult(),

        // Campfire (74): 使用累積的 x_mult
        74 => bonus.mul_mult = state.get_x_mult(),

        // Wee (90): 使用累積的 chips
        90 => bonus.chip_bonus = state.get_chips() as i64,

        // Merry (91): 使用累積的 mult
        91 => bonus.add_mult = state.get_mult() as i64,

        // Vampire (97): 使用累積的 x_mult
        97 => bonus.mul_mult = state.get_x_mult(),

        // GlassJoker (22): 使用累積的 x_mult
        22 => bonus.mul_mult = state.get_x_mult(),

        // Hologram (23): 使用累積的 x_mult
        23 => bonus.mul_mult = state.get_x_mult(),

        // Constellation (64): 使用累積的 x_mult
        64 => bonus.mul_mult = state.get_x_mult(),

        // Yorick (122): 使用累積的 x_mult
        122 => bonus.mul_mult = state.get_x_mult(),

        // Hit_The_Road (110): 使用累積的 x_mult
        110 => bonus.mul_mult = state.get_x_mult(),

        // Lucky_Cat (129): 使用累積的 x_mult
        129 => bonus.mul_mult = state.get_x_mult(),

        // Obelisk (130): 使用累積的 x_mult
        130 => bonus.mul_mult = state.get_x_mult(),

        // Canio (120): 使用累積的 x_mult
        120 => bonus.mul_mult = state.get_x_mult(),

        // Caino (139): 使用累積的 x_mult
        139 => bonus.mul_mult = state.get_x_mult(),

        // Madness (93): 使用累積的 x_mult
        93 => bonus.mul_mult = state.get_x_mult(),

        // Castle (72): 使用目標值作為 chips
        72 => bonus.chip_bonus = state.get_target_value() as i64,

        // Rocket (96): 使用累積的金幣加成
        96 => bonus.money_bonus = state.get_chips() as i64, // 借用 chips 欄位存金幣

        // === 目標效果 ===
        // AncientJoker (68): 如果手牌包含指定花色，X1.5 Mult
        68 => {
            let target_suit = state.get_target_suit();
            if ctx.played_cards.iter().any(|c| c.suit == target_suit) {
                bonus.mul_mult = 1.5;
            }
        }

        // TheIdol (159): 如果打出目標牌，X2 Mult
        159 => {
            let target_suit = state.get_target_suit();
            let target_rank = state.get_target_rank();
            if ctx.played_cards.iter().any(|c| c.suit == target_suit && c.rank == target_rank) {
                bonus.mul_mult = 2.0;
            }
        }

        // ToDoList (155): 如果打出目標牌型，+$4
        155 => {
            // 需要額外判斷是否打出目標牌型
            // 目標牌型存在 target_rank 中
            let target_hand = state.get_target_rank();
            if ctx.hand_id as u8 == target_hand {
                bonus.money_bonus = 4;
            }
        }

        // === 計數效果 ===
        // Selzer (71): 10 次用完後消失，每次重觸發所有牌
        71 => {
            if state.get_counter() > 0 {
                bonus.retriggers = ctx.played_cards.len() as i32;
            }
        }

        // LoyaltyCard (142): 每 6 手 X4 Mult
        142 => {
            // 當計數器達到 6 時觸發
            if let JokerState::Counter { current, threshold, .. } = state {
                if *current >= *threshold {
                    bonus.mul_mult = 4.0;
                }
            }
        }

        // === 上下文相關效果 ===
        // AbstractJoker (19): +3 Mult per Joker
        19 => bonus.add_mult = 3 * ctx.joker_count as i64,

        // Bull (41): +2 Chips per $1 held
        41 => bonus.chip_bonus = 2 * ctx.money_held,

        // Banner (16): +30 Chips per discard left
        16 => bonus.chip_bonus = 30 * ctx.discards_remaining as i64,

        // BlueJoker (62): +2 Chips per deck card
        62 => bonus.chip_bonus = 2 * ctx.deck_size as i64,

        // Erosion (38): +4 Mult per card below 52
        38 => {
            if ctx.deck_size < 52 {
                bonus.add_mult = 4 * (52 - ctx.deck_size) as i64;
            }
        }

        // Stencil (116): X1 per empty slot
        116 => {
            let empty_slots = ctx.joker_slot_limit.saturating_sub(ctx.joker_count);
            if empty_slots > 0 {
                bonus.mul_mult = empty_slots as f32;
            }
        }

        // DriversLicense (109): X3 if 16+ enhanced
        109 => {
            if ctx.enhanced_cards_in_deck >= 16 {
                bonus.mul_mult = 3.0;
            }
        }

        // BaseballCard (152): X1.5 per Uncommon Joker
        152 => {
            if ctx.uncommon_joker_count > 0 {
                bonus.mul_mult = 1.5_f32.powi(ctx.uncommon_joker_count as i32);
            }
        }

        // 預設情況
        _ => {}
    }

    bonus
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

        // ====================================================================
        // 2.4.5 狀態相關類 Joker (25 個) - 需要 JokerState 或 ScoringContext
        // ====================================================================

        // #117: RideTheBus (20): +Mult per consecutive non-face hand
        20 => EffectDef::Stateful,

        // #118: GreenJoker (28): +Mult per hand played this round
        28 => EffectDef::Stateful,

        // #119: IceCream (60): +100 Chips -5 per hand
        60 => EffectDef::Stateful,

        // #120: Popcorn (67): +20 Mult -4 per round
        67 => EffectDef::Stateful,

        // #121: Ramen (69): X2 -0.01 per discard
        69 => EffectDef::Stateful,

        // #122: Campfire (74): X+0.25 per card sold
        74 => EffectDef::Stateful,

        // #123: Wee (90): +8 Chips per round
        90 => EffectDef::Stateful,

        // #124: Merry (91): +3 Mult per round
        91 => EffectDef::Stateful,

        // #125: Vampire (97): X+0.1 per enhancement absorbed
        97 => EffectDef::Stateful,

        // #126: GlassJoker (22): X+0.75 per Glass broken
        22 => EffectDef::Stateful,

        // #127: Hologram (23): X+0.25 per card added to deck
        23 => EffectDef::Stateful,

        // #128: Constellation (64): X+0.1 per Planet used
        64 => EffectDef::Stateful,

        // #129: Yorick (122): X1 per 23 discards
        122 => EffectDef::Stateful,

        // #130: Hit_The_Road (110): X+0.5 per Jack discarded
        110 => EffectDef::Stateful,

        // #131: Lucky_Cat (129): X+0.25 per Lucky triggered
        129 => EffectDef::Stateful,

        // #132: Obelisk (130): X+0.2 per non-most-played hand streak
        130 => EffectDef::Stateful,

        // #133: Canio (120): X+1 per face card destroyed
        120 => EffectDef::Stateful,

        // #134: Caino (139): X+0.1 per face card destroyed
        139 => EffectDef::Stateful,

        // #135: Madness (93): X+0.5 per Joker destroyed
        93 => EffectDef::Stateful,

        // #136: Castle (72): +Chips per suit discarded (Target state)
        72 => EffectDef::Stateful,

        // #137: AncientJoker (68): X1.5 for target suit (Target state)
        68 => EffectDef::Stateful,

        // #138: TheIdol (159): Target card X2 (Target state)
        159 => EffectDef::Stateful,

        // #139: Selzer (71): 10 cards retrigger (Counter state)
        71 => EffectDef::Stateful,

        // #140: LoyaltyCard (142): X4 every 6 hands (Counter state)
        142 => EffectDef::Stateful,

        // #141: ToDoList (155): +$4 for target hand type (Target state)
        155 => EffectDef::Stateful,

        // ====================================================================
        // 2.4.6 手牌計數類 Joker (6 個)
        // ====================================================================

        // #142: HalfJoker (15): +20 Mult if <=3 cards played
        15 => EffectDef::Conditional {
            condition: Condition::PlayedCardCount { min: None, max: Some(3) },
            bonus: BonusDef::Mult(20),
        },

        // #143: Banner (16): +30 Chips per discard remaining (Stateful)
        16 => EffectDef::Stateful,

        // #144: MysticSummit (17): +15 Mult if 0 discards remaining (Stateful)
        17 => EffectDef::Stateful,

        // #145: Square (94): +4 Chips if exactly 4 cards played
        94 => EffectDef::Conditional {
            condition: Condition::PlayedCardCount { min: Some(4), max: Some(4) },
            bonus: BonusDef::Chips(4),
        },

        // #146: BlueJoker (62): +2 Chips per deck card (Stateful)
        62 => EffectDef::Stateful,

        // #147: Erosion (38): +4 Mult per card below 52 (Stateful)
        38 => EffectDef::Stateful,

        // ====================================================================
        // 2.4.7 重觸發類 Joker (3 個)
        // ====================================================================

        // #148: SockAndBuskin (78): Retrigger Face cards
        78 => EffectDef::Retrigger {
            filter: CardFilter::FaceCard,
            count: 1,
        },

        // #149: HangingChad (84): Retrigger first card (Stateful)
        84 => EffectDef::Stateful,

        // #150: Hack (103): Retrigger 2/3/4/5 cards
        103 => EffectDef::Retrigger {
            filter: CardFilter::LowNumber,
            count: 1,
        },

        // ====================================================================
        // 2.4.8 條件加成類 Joker (6 個)
        // ====================================================================

        // #151: DuskJoker (30): X2 final hand
        30 => EffectDef::Conditional {
            condition: Condition::Timing { first_hand: false, final_hand: true },
            bonus: BonusDef::XMult(2.0),
        },

        // #152: Acrobat (77): X3 final hand
        77 => EffectDef::Conditional {
            condition: Condition::Timing { first_hand: false, final_hand: true },
            bonus: BonusDef::XMult(3.0),
        },

        // #153: DNA (61): X2 first hand
        61 => EffectDef::Conditional {
            condition: Condition::Timing { first_hand: true, final_hand: false },
            bonus: BonusDef::XMult(2.0),
        },

        // #154: Photograph (27): X2 first Face card (Stateful)
        27 => EffectDef::Stateful,

        // #155: Stencil (116): X1 per empty Joker slot (Stateful)
        116 => EffectDef::Stateful,

        // #156: DriversLicense (109): X3 if 16+ enhanced cards (Stateful)
        109 => EffectDef::Stateful,

        // ====================================================================
        // 2.4.9 規則修改類 Joker (6 個)
        // ====================================================================

        // #157: FourFingers (24): 4-card Straights/Flushes
        24 => EffectDef::RuleModifier,

        // #158: Shortcut (25): Straights can skip 1 rank
        25 => EffectDef::RuleModifier,

        // #159: Splash (26): All cards count for scoring
        26 => EffectDef::RuleModifier,

        // #160: Pareidolia (104): All cards are Face cards
        104 => EffectDef::RuleModifier,

        // #161: Smeared (82): Red/Black count as same suit
        82 => EffectDef::RuleModifier,

        // #162: OopsAll6s (158): 6s count as all suits
        158 => EffectDef::RuleModifier,

        // ====================================================================
        // 2.4.10 特殊效果類 Joker (8 個)
        // ====================================================================

        // #163: Misprint (18): random 0-23 Mult (Stateful - RNG)
        18 => EffectDef::Stateful,

        // #164: Cavendish (100): X3 + self-destruct chance (Stateful)
        100 => EffectDef::Stateful,

        // #165: Gros_Michel (105): +15 Mult + self-destruct (Stateful)
        105 => EffectDef::Stateful,

        // #166: MrBones (76): Prevent death (Stateful)
        76 => EffectDef::Stateful,

        // #167: Brainstorm (51): Copy leftmost Joker (Stateful)
        51 => EffectDef::Stateful,

        // #168: Blueprint (89): Copy right Joker (Stateful)
        89 => EffectDef::Stateful,

        // #169: Perkeo (117): Negative consumable copy (Stateful)
        117 => EffectDef::Stateful,

        // #170: Chicot (123): Disable Boss Blind (Stateful)
        123 => EffectDef::Stateful,

        // 其他 Joker 暫時返回默認效果（待實現）
        _ => EffectDef::default(),
    }
}

// ============================================================================
// 觸發器定義
// ============================================================================

/// 觸發器效果類型
#[derive(Clone, Debug, PartialEq)]
pub enum TriggerEffect {
    /// 無動作（純觸發，由調用者處理）
    None,

    /// 累加狀態
    AddToState {
        chips: i32,
        mult: i32,
        x_mult: f32,
    },

    /// 重置狀態
    ResetState,

    /// 增加計數器
    IncrementCounter,

    /// 獲得金幣
    GainMoney(i64),

    /// 自我銷毀（如 Cavendish, Gros Michel）
    SelfDestruct {
        chance: u8, // 1/chance 機率
    },

    /// 創建負片複製品（Perkeo）
    CreateNegativeCopy,

    /// 銷毀隨機 Joker（Madness）
    DestroyRandomJoker,

    /// 禁用 Boss Blind（Chicot）
    DisableBossBlind,

    /// 自訂效果（需要在 trigger_joker_events 中特殊處理）
    Custom,
}

/// 觸發器定義
#[derive(Clone, Debug)]
pub struct TriggerDef {
    /// 觸發事件
    pub event: GameEvent,
    /// 觸發效果
    pub effect: TriggerEffect,
}

/// 根據 JokerId 索引獲取觸發器定義
///
/// 返回該 Joker 響應的事件列表。
/// 大多數 Joker 只在計分時生效（無觸發器）。
/// 只有狀態更新類 Joker 需要觸發器定義。
pub fn get_triggers(id_index: usize) -> &'static [TriggerDef] {
    // 使用 lazy_static 或 const 來定義觸發器
    // 由於 Rust 的限制，這裡使用 match 返回靜態引用

    match id_index {
        // ====================================================================
        // 回合結束觸發器
        // ====================================================================

        // GoldenJoker (40): 回合結束 +$4
        40 => &[TriggerDef {
            event: GameEvent::RoundEnded,
            effect: TriggerEffect::GainMoney(4),
        }],

        // IceCream (60): 每手 -5 Chips
        60 => &[TriggerDef {
            event: GameEvent::HandPlayed,
            effect: TriggerEffect::AddToState { chips: -5, mult: 0, x_mult: 0.0 },
        }],

        // Popcorn (67): 每輪 -4 Mult
        67 => &[TriggerDef {
            event: GameEvent::RoundEnded,
            effect: TriggerEffect::AddToState { chips: 0, mult: -4, x_mult: 0.0 },
        }],

        // Ramen (69): 每次棄牌 -0.01 X Mult
        69 => &[TriggerDef {
            event: GameEvent::CardDiscarded,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: -0.01 },
        }],

        // Campfire (74): 每次賣出 +0.25 X Mult
        74 => &[TriggerDef {
            event: GameEvent::JokerSold,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: 0.25 },
        }],

        // Wee (90): 每輪 +8 Chips（打出 2 時）
        90 => &[TriggerDef {
            event: GameEvent::HandPlayed,
            effect: TriggerEffect::Custom, // 需要檢查是否打出 2
        }],

        // Merry (91): 每輪 +3 Mult（打出 K 時）
        91 => &[TriggerDef {
            event: GameEvent::HandPlayed,
            effect: TriggerEffect::Custom, // 需要檢查是否打出 K
        }],

        // ====================================================================
        // 狀態累加觸發器
        // ====================================================================

        // Vampire (97): 吸收增強 +0.1 X Mult
        97 => &[TriggerDef {
            event: GameEvent::EnhancementAbsorbed,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: 0.1 },
        }],

        // GlassJoker (22): Glass 牌破碎 +0.75 X Mult
        22 => &[TriggerDef {
            event: GameEvent::GlassCardBroken,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: 0.75 },
        }],

        // Hologram (23): 牌加入牌組 +0.25 X Mult
        23 => &[TriggerDef {
            event: GameEvent::CardAddedToDeck,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: 0.25 },
        }],

        // Constellation (64): 使用 Planet +0.1 X Mult
        64 => &[TriggerDef {
            event: GameEvent::PlanetUsed,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: 0.1 },
        }],

        // Lucky_Cat (129): Lucky 觸發 +0.25 X Mult
        129 => &[TriggerDef {
            event: GameEvent::LuckyTriggered,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: 0.25 },
        }],

        // Canio (120): 人頭牌銷毀 +1.0 X Mult
        120 => &[TriggerDef {
            event: GameEvent::FaceCardDestroyed,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: 1.0 },
        }],

        // Caino (139): 人頭牌銷毀 +0.1 X Mult
        139 => &[TriggerDef {
            event: GameEvent::FaceCardDestroyed,
            effect: TriggerEffect::AddToState { chips: 0, mult: 0, x_mult: 0.1 },
        }],

        // ====================================================================
        // 計數器觸發器
        // ====================================================================

        // Yorick (122): 每次棄牌增加計數器
        122 => &[TriggerDef {
            event: GameEvent::CardDiscarded,
            effect: TriggerEffect::IncrementCounter,
        }],

        // Selzer (71): 每手減少 charges
        71 => &[TriggerDef {
            event: GameEvent::HandPlayed,
            effect: TriggerEffect::Custom, // 減少 counter
        }],

        // LoyaltyCard (142): 每手增加計數器
        142 => &[TriggerDef {
            event: GameEvent::HandPlayed,
            effect: TriggerEffect::IncrementCounter,
        }],

        // Obelisk (130): 每手更新連勝
        130 => &[TriggerDef {
            event: GameEvent::HandPlayed,
            effect: TriggerEffect::Custom, // 需要檢查牌型
        }],

        // ====================================================================
        // 特殊效果觸發器
        // ====================================================================

        // Madness (93): 選擇 Blind 時銷毀隨機 Joker +0.5 X Mult
        93 => &[TriggerDef {
            event: GameEvent::BlindSelected,
            effect: TriggerEffect::DestroyRandomJoker,
        }],

        // Perkeo (117): 商店結束時創建負片消耗品
        117 => &[TriggerDef {
            event: GameEvent::RoundEnded,
            effect: TriggerEffect::CreateNegativeCopy,
        }],

        // Chicot (123): Boss Blind 開始時禁用
        123 => &[TriggerDef {
            event: GameEvent::BlindSelected,
            effect: TriggerEffect::DisableBossBlind,
        }],

        // Cavendish (100): 回合結束 1/1000 機率自毀
        100 => &[TriggerDef {
            event: GameEvent::RoundEnded,
            effect: TriggerEffect::SelfDestruct { chance: 250 }, // 1/250 per round (approx 1/1000 total)
        }],

        // Gros Michel (105): 回合結束 1/6 機率自毀
        105 => &[TriggerDef {
            event: GameEvent::RoundEnded,
            effect: TriggerEffect::SelfDestruct { chance: 6 },
        }],

        // GreenJoker (28): 每手 +1 Mult，每輪重置
        28 => &[
            TriggerDef {
                event: GameEvent::HandPlayed,
                effect: TriggerEffect::AddToState { chips: 0, mult: 1, x_mult: 0.0 },
            },
            TriggerDef {
                event: GameEvent::RoundEnded,
                effect: TriggerEffect::ResetState,
            },
        ],

        // RideTheBus (20): 連續非人頭牌手 +1 Mult
        20 => &[TriggerDef {
            event: GameEvent::HandPlayed,
            effect: TriggerEffect::Custom, // 需要檢查是否為人頭牌
        }],

        // Hit_The_Road (110): 棄掉 Jack +0.5 X Mult
        110 => &[TriggerDef {
            event: GameEvent::CardDiscarded,
            effect: TriggerEffect::Custom, // 需要檢查是否為 Jack
        }],

        // Castle (72): 棄掉特定花色牌 +3 Chips
        72 => &[TriggerDef {
            event: GameEvent::CardDiscarded,
            effect: TriggerEffect::Custom, // 需要檢查花色
        }],

        // 其他 Joker 沒有事件觸發器
        _ => &[],
    }
}

// ============================================================================
// 觸發器執行
// ============================================================================

/// 觸發器執行結果
#[derive(Clone, Debug, Default)]
pub struct TriggerResult {
    /// 金幣變化
    pub money_delta: i64,
    /// 需要銷毀的 Joker 索引
    pub jokers_to_destroy: Vec<usize>,
    /// 是否禁用 Boss Blind
    pub disable_boss_blind: bool,
    /// 是否創建負片消耗品
    pub create_negative_copy: bool,
}

impl TriggerResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn merge(&mut self, other: &TriggerResult) {
        self.money_delta += other.money_delta;
        self.jokers_to_destroy.extend(other.jokers_to_destroy.iter().copied());
        self.disable_boss_blind |= other.disable_boss_blind;
        self.create_negative_copy |= other.create_negative_copy;
    }
}

/// 處理遊戲事件，更新所有 Joker 狀態並收集結果
///
/// # 參數
/// - `event`: 發生的遊戲事件
/// - `joker_states`: 所有 Joker 的狀態（可變引用）
/// - `joker_indices`: 對應的 Joker ID 索引列表
/// - `ctx`: 觸發器上下文（包含額外信息）
///
/// # 返回
/// 觸發器執行結果（金幣變化、銷毀列表等）
pub fn trigger_joker_events(
    event: GameEvent,
    joker_states: &mut [JokerState],
    joker_indices: &[usize],
    ctx: &TriggerContext,
) -> TriggerResult {
    let mut result = TriggerResult::new();

    for (slot_idx, &joker_idx) in joker_indices.iter().enumerate() {
        let triggers = get_triggers(joker_idx);

        for trigger in triggers {
            if trigger.event != event {
                continue;
            }

            // 獲取當前狀態的可變引用
            let state = &mut joker_states[slot_idx];

            match &trigger.effect {
                TriggerEffect::None => {}

                TriggerEffect::AddToState { chips, mult, x_mult } => {
                    state.add_chips(*chips);
                    state.add_mult(*mult);
                    if *x_mult != 0.0 {
                        state.add_x_mult(*x_mult);
                    }
                }

                TriggerEffect::ResetState => {
                    // 重置為初始狀態
                    *state = get_joker_def(joker_idx).initial_state;
                }

                TriggerEffect::IncrementCounter => {
                    state.increment_counter();
                }

                TriggerEffect::GainMoney(amount) => {
                    result.money_delta += amount;
                }

                TriggerEffect::SelfDestruct { chance } => {
                    // 使用 rng_value 決定是否自毀
                    // rng_value 範圍 0-255
                    let threshold = 256 / (*chance as u16);
                    if (ctx.rng_value as u16) < threshold {
                        result.jokers_to_destroy.push(slot_idx);
                    }
                }

                TriggerEffect::CreateNegativeCopy => {
                    result.create_negative_copy = true;
                }

                TriggerEffect::DestroyRandomJoker => {
                    // 標記需要銷毀隨機 Joker（調用者處理）
                    // 同時增加 Madness 的 X Mult
                    state.add_x_mult(0.5);
                }

                TriggerEffect::DisableBossBlind => {
                    if ctx.is_boss_blind {
                        result.disable_boss_blind = true;
                    }
                }

                TriggerEffect::Custom => {
                    // 自訂效果需要在調用者處理
                    // 這裡只標記需要特殊處理
                    process_custom_trigger(joker_idx, event, state, ctx, &mut result);
                }
            }
        }
    }

    result
}

/// 處理自訂觸發效果
fn process_custom_trigger(
    joker_idx: usize,
    event: GameEvent,
    state: &mut JokerState,
    ctx: &TriggerContext,
    _result: &mut TriggerResult,
) {
    match (joker_idx, event) {
        // RideTheBus (20): 連續非人頭牌手 +1 Mult
        // 這個需要在調用者處理，因為需要檢查打出的牌
        (20, GameEvent::HandPlayed) => {
            // 由調用者決定是增加還是重置
        }

        // Hit_The_Road (110): 棄掉 Jack +0.5 X Mult
        (110, GameEvent::CardDiscarded) => {
            // 使用 discarded_face_count 作為 Jack 數量的近似
            // 實際實現需要調用者傳遞更精確的信息
        }

        // Castle (72): 棄掉特定花色牌 +3 Chips
        (72, GameEvent::CardDiscarded) => {
            let target_suit = state.get_target_suit() as usize;
            if target_suit < 4 {
                let count = ctx.discarded_suit_count[target_suit];
                if count > 0 {
                    // 每棄一張目標花色牌 +3 Chips
                    let target_value = state.get_target_value();
                    state.set_target_value(target_value + count * 3);
                }
            }
        }

        // Wee (90): 打出 2 時 +8 Chips
        (90, GameEvent::HandPlayed) => {
            // 需要調用者傳遞是否打出 2
        }

        // Merry (91): 打出 K 時 +3 Mult
        (91, GameEvent::HandPlayed) => {
            // 需要調用者傳遞是否打出 K
        }

        // Selzer (71): 每手減少 charges
        (71, GameEvent::HandPlayed) => {
            // 減少計數器
            if let JokerState::Counter { current, .. } = state {
                if *current > 0 {
                    *current -= 1;
                }
            }
        }

        // Obelisk (130): 每手更新連勝
        (130, GameEvent::HandPlayed) => {
            // 需要調用者傳遞牌型信息來決定是否增加連勝
        }

        _ => {}
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

    #[test]
    fn test_compute_effect_fixed() {
        use super::{compute_effect, get_effect_def, ComputeContext};

        // 0: Joker - +4 Mult (Fixed)
        let effect = get_effect_def(0);
        let ctx = ComputeContext::new(&[], &[], HandId::HighCard);

        let bonus = compute_effect(&effect, &ctx);
        assert_eq!(bonus.add_mult, 4);
        assert_eq!(bonus.chip_bonus, 0);
        assert!((bonus.mul_mult - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_effect_conditional_pass() {
        use super::{compute_effect, get_effect_def, ComputeContext};

        // 5: JollyJoker - +8 Mult on Pair hands
        let effect = get_effect_def(5);
        let ctx = ComputeContext::new(&[], &[], HandId::Pair);

        let bonus = compute_effect(&effect, &ctx);
        assert_eq!(bonus.add_mult, 8);
    }

    #[test]
    fn test_compute_effect_conditional_fail() {
        use super::{compute_effect, get_effect_def, ComputeContext};

        // 5: JollyJoker - +8 Mult on Pair hands (should NOT trigger on HighCard)
        let effect = get_effect_def(5);
        let ctx = ComputeContext::new(&[], &[], HandId::HighCard);

        let bonus = compute_effect(&effect, &ctx);
        assert_eq!(bonus.add_mult, 0); // 條件不滿足，不觸發
    }

    #[test]
    fn test_compute_effect_timing_final_hand() {
        use super::{compute_effect, get_effect_def, ComputeContext};

        // 30: DuskJoker - X2 on final hand
        let effect = get_effect_def(30);

        // 最後一手
        let mut ctx = ComputeContext::new(&[], &[], HandId::HighCard);
        ctx.is_final_hand = true;
        let bonus = compute_effect(&effect, &ctx);
        assert!((bonus.mul_mult - 2.0).abs() < 0.001);

        // 非最後一手
        let ctx2 = ComputeContext::new(&[], &[], HandId::HighCard);
        let bonus2 = compute_effect(&effect, &ctx2);
        assert!((bonus2.mul_mult - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_effect_stateful() {
        use super::{compute_effect, get_effect_def, ComputeContext};

        // 19: AbstractJoker - +3 Mult per Joker (Stateful)
        let effect = get_effect_def(19);
        let ctx = ComputeContext::new(&[], &[], HandId::HighCard);

        // Stateful 效果應返回空獎勵
        let bonus = compute_effect(&effect, &ctx);
        assert_eq!(bonus.add_mult, 0);
        assert_eq!(bonus.chip_bonus, 0);
        assert!((bonus.mul_mult - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_effect_rule_modifier() {
        use super::{compute_effect, get_effect_def, ComputeContext};

        // 24: FourFingers - 4-card Straights/Flushes (RuleModifier)
        let effect = get_effect_def(24);
        let ctx = ComputeContext::new(&[], &[], HandId::HighCard);

        // RuleModifier 效果不直接提供獎勵
        let bonus = compute_effect(&effect, &ctx);
        assert_eq!(bonus.add_mult, 0);
        assert_eq!(bonus.chip_bonus, 0);
        assert!((bonus.mul_mult - 1.0).abs() < 0.001);
    }

    // ========================================================================
    // compute_joker_effect_v2 測試
    // ========================================================================

    #[test]
    fn test_v2_fixed_joker() {
        use super::{compute_joker_effect_v2, ComputeContextV2, ComputeContext};

        // 0: Joker - +4 Mult (Fixed)
        let basic_ctx = ComputeContext::new(&[], &[], HandId::HighCard);
        let ctx = ComputeContextV2::from_basic(&basic_ctx);
        let bonus = compute_joker_effect_v2(0, &JokerState::None, &ctx, 0);
        assert_eq!(bonus.add_mult, 4);
    }

    #[test]
    fn test_v2_abstract_joker() {
        use super::{compute_joker_effect_v2, ComputeContextV2, ComputeContext};

        // 19: AbstractJoker - +3 Mult per Joker (Stateful)
        let basic_ctx = ComputeContext::new(&[], &[], HandId::HighCard);
        let mut ctx = ComputeContextV2::from_basic(&basic_ctx);
        ctx.joker_count = 5;

        let bonus = compute_joker_effect_v2(19, &JokerState::None, &ctx, 0);
        assert_eq!(bonus.add_mult, 15); // 5 * 3 = 15
    }

    #[test]
    fn test_v2_bull() {
        use super::{compute_joker_effect_v2, ComputeContextV2, ComputeContext};

        // 41: Bull - +2 Chips per $1 held (Stateful)
        let basic_ctx = ComputeContext::new(&[], &[], HandId::HighCard);
        let mut ctx = ComputeContextV2::from_basic(&basic_ctx);
        ctx.money_held = 25;

        let bonus = compute_joker_effect_v2(41, &JokerState::None, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 50); // 25 * 2 = 50
    }

    #[test]
    fn test_v2_vampire_with_state() {
        use super::{compute_joker_effect_v2, ComputeContextV2, ComputeContext};

        // 97: Vampire - uses accumulated x_mult (Stateful)
        let basic_ctx = ComputeContext::new(&[], &[], HandId::HighCard);
        let ctx = ComputeContextV2::from_basic(&basic_ctx);
        let state = JokerState::accumulator(0, 0, 2.5);

        let bonus = compute_joker_effect_v2(97, &state, &ctx, 0);
        assert!((bonus.mul_mult - 2.5).abs() < 0.001);
    }

    #[test]
    fn test_v2_ancient_joker_trigger() {
        use super::{compute_joker_effect_v2, ComputeContextV2};
        use super::super::cards::Card;

        // 68: AncientJoker - X1.5 if target suit in played cards
        // Card::new(rank, suit)
        let played_cards = vec![Card::new(5, HEART)];
        let ctx = ComputeContextV2 {
            played_cards: &played_cards,
            hand: &[],
            hand_id: HandId::HighCard,
            is_first_hand: false,
            is_final_hand: false,
            money_held: 0,
            joker_count: 1,
            joker_slot_limit: 5,
            discards_remaining: 3,
            hands_played_this_round: 0,
            hands_played_this_run: 0,
            deck_size: 52,
            enhanced_cards_in_deck: 0,
            uncommon_joker_count: 0,
        };

        // 目標花色是 HEART
        let state = JokerState::Target { suit: HEART, rank: 0, value: 0 };
        let bonus = compute_joker_effect_v2(68, &state, &ctx, 0);
        assert!((bonus.mul_mult - 1.5).abs() < 0.001);

        // 目標花色是 SPADE (不在 played_cards 中)
        let state2 = JokerState::Target { suit: SPADE, rank: 0, value: 0 };
        let bonus2 = compute_joker_effect_v2(68, &state2, &ctx, 0);
        assert!((bonus2.mul_mult - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_v2_stencil() {
        use super::{compute_joker_effect_v2, ComputeContextV2, ComputeContext};

        // 116: Stencil - X1 per empty slot (Stateful)
        let basic_ctx = ComputeContext::new(&[], &[], HandId::HighCard);
        let mut ctx = ComputeContextV2::from_basic(&basic_ctx);
        ctx.joker_slot_limit = 5;
        ctx.joker_count = 2; // 3 empty slots

        let bonus = compute_joker_effect_v2(116, &JokerState::None, &ctx, 0);
        assert!((bonus.mul_mult - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_v2_conditional_jolly_joker() {
        use super::{compute_joker_effect_v2, ComputeContextV2, ComputeContext};

        // 5: JollyJoker - +8 Mult on Pair hands (Conditional, not Stateful)
        let basic_ctx = ComputeContext::new(&[], &[], HandId::Pair);
        let ctx = ComputeContextV2::from_basic(&basic_ctx);

        let bonus = compute_joker_effect_v2(5, &JokerState::None, &ctx, 0);
        assert_eq!(bonus.add_mult, 8);

        // Non-pair hand should not trigger
        let basic_ctx2 = ComputeContext::new(&[], &[], HandId::HighCard);
        let ctx2 = ComputeContextV2::from_basic(&basic_ctx2);
        let bonus2 = compute_joker_effect_v2(5, &JokerState::None, &ctx2, 0);
        assert_eq!(bonus2.add_mult, 0);
    }
}
