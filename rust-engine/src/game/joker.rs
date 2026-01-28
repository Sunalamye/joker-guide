//! Joker 系統 - Tiered Architecture
//!
//! 使用分層架構實現 Joker 效果：
//! - Tier 1 (CoreJoker): 高頻 Joker，使用 enum 實現靜態分發
//! - Tier 2 (ConditionalJoker): 條件觸發類，使用 trait object
//! - Tier 3: 複雜/動態 Joker，未來擴展

use super::cards::{Card, Edition, Enhancement};
use super::hand_types::HandId;
use super::joker_def::{
    JokerState, get_joker_def, compute_joker_effect_v2, ComputeContextV2,
    JokerBonus as JokerBonusNew,
    GameEvent, TriggerContext, TriggerResult, trigger_joker_events,
};

// ============================================================================
// Joker ID 系統 - 消除字串比對
// ============================================================================

/// Joker 總數
pub const JOKER_COUNT: usize = 164;

/// Joker 唯一識別碼
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum JokerId {
    // ========================================================================
    // Tier 1: Basic Mult/Chip Jokers (索引 0-19)
    // ========================================================================
    Joker = 0,           // 基礎 Joker: +4 Mult
    GreedyJoker = 1,     // +$3 per Diamond
    LustyJoker = 2,      // +$3 per Heart
    WrathfulJoker = 3,   // +$3 per Spade
    GluttonousJoker = 4, // +$3 per Club
    JollyJoker = 5,      // +8 Mult (Pair)
    ZanyJoker = 6,       // +12 Mult (Three of a Kind)
    MadJoker = 7,        // +10 Mult (Two Pair)
    CrazyJoker = 8,      // +12 Mult (Straight)
    DrollJoker = 9,      // +10 Mult (Flush)
    SlyJoker = 10,       // +50 Chips (Pair)
    WilyJoker = 11,      // +100 Chips (Three of a Kind)
    CleverJoker = 12,    // +80 Chips (Two Pair)
    DeviousJoker = 13,   // +100 Chips (Straight)
    CraftyJoker = 14,    // +80 Chips (Flush)
    HalfJoker = 15,      // +20 Mult if <=3 cards
    Banner = 16,         // +30 Chips per remaining discard
    MysticSummit = 17,   // +15 Mult if 0 discards left
    Misprint = 18,       // +?? Mult (random 0-23)
    AbstractJoker = 19,  // +3 Mult per Joker

    // ========================================================================
    // Tier 1: Multiplier Jokers (索引 20-39)
    // ========================================================================
    RideTheBus = 20,    // +1 Mult per consecutive non-face hand
    SteelJoker = 21,    // X0.2 Mult per Steel card in full hand
    GlassJoker = 22,    // X0.75 Mult per Glass card broken
    Hologram = 23,      // X0.25 Mult per card added to deck
    FourFingers = 24,   // Straights/Flushes can be made with 4 cards
    Shortcut = 25,      // Straights can skip 1 rank
    Splash = 26,        // Every card counts in every hand type
    Photograph = 27,    // X2 Mult for first Face card played
    GreenJoker = 28,    // +1 Mult per hand played (resets per round)
    SuperPosition = 29, // Creates X2 Mult when hand is both Straight & Flush
    DuskJoker = 30,     // X2 Mult on last hand of round
    Fibonacci = 31,     // Ace/2/3/5/8 played give +8 Mult
    ScaryFace = 32,     // Face cards give +30 Chips
    EvenSteven = 33,    // Even cards give +4 Mult
    OddTodd = 34,       // Odd cards give +31 Chips
    Scholar = 35,       // Aces give +20 Chips and +4 Mult
    BusinessCard = 36,  // Face cards have 1/2 chance +$2
    Supernova = 37,     // +Mult equal to times hand played this run
    Erosion = 38,       // +4 Mult for each card below 52 in deck
    ToTheMoon = 39,     // +$1 per $5 held (replaces interest)

    // ========================================================================
    // Tier 1: Economy Jokers (索引 40-59)
    // ========================================================================
    GoldenJoker = 40,   // +$4 at end of round
    Bull = 41,          // +2 Chips for each $1 held
    Egg = 42,           // +$3 sell value per round
    Cartomancer = 43,   // Creates Tarot on Blind skip
    Astronomer = 44,    // Creates Planet on Blind skip
    Rocket = 45,        // +$1 at end of round (scaling)
    FortuneTeller = 46, // +1 Mult per Tarot used this run
    Faceless = 47,      // +$5 if 3+ Face cards discarded
    SpaceJoker = 48,    // 1/4 chance to upgrade hand level
    Vagabond = 49,      // Creates Tarot if hand played with <=4 cards
    Stuntman = 50,      // +250 Chips, -2 hand size
    Brainstorm = 51,    // Copies leftmost Joker ability
    Satellite = 52,     // +$1 per unique Planet used this run
    ShootTheMoon = 53,  // +13 Mult per Queen in hand
    Bloodstone = 54,    // 1/2 chance X1.5 Mult for Hearts scored
    Arrowhead = 55,     // Spades give +50 Chips
    Onyx = 56,          // Clubs give +80 Mult
    Opal = 57,          // Diamonds give +1.5 X Mult
    Drunkard = 58,      // +1 discard each round
    SteakJoker = 59,    // X2 Mult, loses $1 sell per round

    // ========================================================================
    // Tier 2: Conditional/Complex Jokers (索引 60-99)
    // ========================================================================
    IceCream = 60,       // +100 Chips, -5 per hand played
    DNA = 61,            // First hand each round triggers twice
    BlueJoker = 62,      // +2 Chips per card in deck
    Sixth = 63,          // Play or discard 6, destroy and gain 1 Spectral
    Constellation = 64,  // X0.1 Mult per Planet used this run
    Hiker = 65,          // +2 Chips per card played
    CloudNine = 66,      // +$1 per 9 in full deck
    Popcorn = 67,        // +20 Mult, -4 per round
    AncientJoker = 68,   // X1.5 Mult if hand contains specific suit
    Ramen = 69,          // X2 Mult, loses X0.01 per card discarded
    Walkie = 70,         // +10 Mult if hand contains 10 or 4
    Selzer = 71,         // Next 10 scored cards retrigger
    Castle = 72,         // +3 Chips per discarded card for specific suit
    Smiley = 73,         // Face cards give +5 Mult
    Campfire = 74,       // X Mult +0.25 for each card sold
    Ticket = 75,         // +$1 per Gold card played
    MrBones = 76,        // Prevents death if chips > 25% of requirement
    Acrobat = 77,        // X3 Mult on final hand of round
    SockAndBuskin = 78,  // Retrigger all played Face cards
    Swashbuckler = 79,   // +2 Mult per card in hand below 8
    Troubadour = 80,     // +2 hand size, -1 hand per round
    Certificate = 81,    // +$1 per Gold Seal card in hand
    Smeared = 82,        // Hearts/Diamonds same suit, Spades/Clubs same suit
    Throwback = 83,      // X0.25 Mult per blind skipped this run
    HangingChad = 84,    // Retrigger first played card
    RoughGem = 85,       // Diamonds give +$1
    Mime = 86,           // Retrigger abilities of cards held in hand
    CreditCard = 87,     // Allows going $20 into debt
    Ceremonial = 88,     // When Blind selected, destroy rightmost Joker
    Blueprint = 89,      // Copies ability of Joker to the right
    Wee = 90,            // +8 Chips per round (persistent)
    Merry = 91,          // +3 Mult per round (persistent)
    RedCard = 92,        // +3 Mult per reroll this run
    Madness = 93,        // X0.5 Mult, destroys random Joker on Blind select
    Square = 94,         // +4 Chips if hand has exactly 4 cards
    Seance = 95,         // If hand is Straight Flush, create Spectral
    RiffRaff = 96,       // On Blind select, create 2 Common Jokers
    Vampire = 97,        // X1 Mult, gains enhancements from played cards
    InvisibleJoker = 98, // After 2 rounds, sell to duplicate random Joker
    Baron = 99,          // Each King held gives X1.5 Mult

    // ========================================================================
    // Tier 2: More Complex Jokers (索引 100-149)
    // ========================================================================
    Cavendish = 100,       // X3 Mult, 1/1000 chance to self-destruct
    Card_Sharp = 101,      // X3 Mult if hand already played this round
    Delayed = 102,         // +$2 per round if no discards used
    Hack = 103,            // Retrigger 2/3/4/5 cards
    Pareidolia = 104,      // All cards count as Face cards
    Gros_Michel = 105,     // +15 Mult, 1/15 chance to self-destruct
    Even_Steven = 106,     // X2 Mult if hand only contains evens
    Odd_Todd_2 = 107,      // X2 Mult if hand only contains odds
    Juggler = 108,         // +1 hand size
    DriversLicense = 109,  // X3 Mult if you have 16+ enhanced cards
    Hit_The_Road = 110,    // X0.5 Mult, gains X0.5 for each Jack discarded
    The_Duo = 111,         // X2 Mult if hand contains Pair
    The_Trio = 112,        // X3 Mult if hand contains Three of a Kind
    The_Family = 113,      // X4 Mult if hand contains Four of a Kind
    The_Order = 114,       // X3 Mult if hand contains Straight
    The_Tribe = 115,       // X2 Mult if hand contains Flush
    Stencil = 116,         // X1 Mult for each empty Joker slot
    Perkeo = 117,          // Creates Negative copy of 1 consumable at end of shop
    Flower_Pot = 118,      // X3 Mult if hand has Diamond/Club/Heart/Spade
    BluePrint = 119,       // Reserved
    Canio = 120,           // X1 Mult, gains X1 for every face card destroyed
    Triboulet = 121,       // Kings and Queens give X2 Mult
    Yorick = 122,          // X1 Mult, gains X1 for every 23 cards discarded
    Chicot = 123,          // Disables Boss Blind effects
    Perkeo_2 = 124,        // Reserved
    Seeing_Double = 125,   // X2 Mult if hand has Club AND another suit
    Matador = 126,         // +$8 when Boss Blind ability triggers
    Stuntman_2 = 127,      // Reserved
    Stone = 128,           // +25 Chips for each Stone card in deck
    Lucky_Cat = 129,       // X0.25 Mult for each Lucky card triggered
    Obelisk = 130,         // X0.2 Mult per consecutive hand without most played type
    Runner = 131,          // +15 Chips if hand is Straight
    Courier = 132,         // +25 Chips per card below Ace in hand
    Cloud9 = 133,          // Reserved
    Spare_Trousers = 134,  // +2 Mult if hand has Two Pair
    Ring_Master = 135,     // Jokers can appear multiple times
    Golden_Ticket = 136,   // Gold cards give +$3 at round end
    Rough_Gem_2 = 137,     // Reserved
    Bootstraps = 138,      // +2 Mult for each $5 above $0
    Caino = 139,           // X1 Mult, gains X0.1 for every face card destroyed
    Flash = 140,           // +2 Mult per reroll used
    Trousers = 141,        // +4 Mult if hand has Two Pair
    LoyaltyCard = 142,     // 每 6 手 X4 Mult
    Blackboard = 143,      // 全黑牌 X3 Mult
    TurtleBean = 144,      // +5 手牌大小，每輪 -1
    Burglar = 145,         // 選 Blind +3 hands, 無 discards
    GiftCard = 146,        // 回合結束 Joker +$1 售價
    Luchador = 147,        // Sell to disable current Boss Blind effect
    ReservedParking = 148, // 手中人頭牌 1/2 機率 +$1 (回合結束)
    TradingCard = 149,     // 首次棄人頭牌時創建 Tarot
    MarbleJoker = 150,     // 選擇 Blind 時加 Stone 卡到牌組
    MailInRebate = 151,    // 棄 K 時 +$5
    BaseballCard = 152,    // X1.5 Mult for each Uncommon Joker held
    RaisedFist = 153,      // Lowest held card gives 2X its rank as Mult
    EightBall = 154,       // 打出 8 時創建 Tarot
    ToDoList = 155,        // 打出特定牌型時 +$4
    BurntJoker = 156,      // 棄牌時升級棄掉牌型的等級
    MidasMask = 157,       // 打出人頭牌時變為 Gold 增強
    OopsAll6s = 158,       // 所有 6 算作每種花色（用於 Flush）
    TheIdol = 159,         // 特定牌（每回合隨機選擇）X2 Mult
    SquareJoker = 160,     // 牌組正好 52 張時，每張打出的牌 +4 Mult
    DietCola = 161,        // 賣出時 +$100（實際獲得售價 + $100）
    ChaosTheClown = 162,   // 每回合 1 次免費 reroll
    Hallucination = 163,   // 開包時有機率生成 Tarot 卡
}

impl JokerId {
    /// 獲取 Joker 的基礎成本
    ///
    /// 從 JOKER_DEFINITIONS 靜態表查詢，集中管理所有 Joker 的成本數據。
    pub fn base_cost(&self) -> i64 {
        get_joker_def(*self as usize).cost
    }

    /// 獲取稀有度 (1=Common, 2=Uncommon, 3=Rare, 4=Legendary)
    ///
    /// 從 JOKER_DEFINITIONS 靜態表查詢，集中管理所有 Joker 的稀有度數據。
    pub fn rarity(&self) -> u8 {
        get_joker_def(*self as usize).rarity as u8
    }

    /// 用於 observation 的整數索引
    pub fn to_index(&self) -> usize {
        *self as usize
    }

    /// 從索引創建 JokerId
    pub fn from_index(index: usize) -> Option<Self> {
        if index < JOKER_COUNT {
            // Safety: We verify the index is within bounds
            Some(unsafe { std::mem::transmute::<u8, JokerId>(index as u8) })
        } else {
            None
        }
    }

    /// 所有可用的 Joker 列表（排除保留位）
    pub fn all_available() -> Vec<JokerId> {
        (0..JOKER_COUNT)
            .filter_map(|i| JokerId::from_index(i))
            .filter(|j| {
                !matches!(
                    j,
                    // 保留位：這些是用於內部測試或尚未實現的 Joker
                    JokerId::BluePrint
                        | JokerId::Perkeo_2
                        | JokerId::Stuntman_2
                        | JokerId::Cloud9
                        | JokerId::Rough_Gem_2
                )
            })
            .collect()
    }

    /// 按稀有度獲取 Joker 列表
    pub fn by_rarity(rarity: u8) -> Vec<JokerId> {
        Self::all_available()
            .into_iter()
            .filter(|j| j.rarity() == rarity)
            .collect()
    }
}

/// 從舊的 type_key 遷移（向後兼容）
impl JokerId {
    pub fn from_legacy_key(key: &str) -> Option<Self> {
        match key {
            "+m" => Some(JokerId::Joker),
            "+c" => Some(JokerId::SlyJoker),
            "!!" => Some(JokerId::WilyJoker),
            "+$" => Some(JokerId::Banner),
            "++" => Some(JokerId::JollyJoker),
            "Xm" => Some(JokerId::RideTheBus),
            "X2" => Some(JokerId::GlassJoker),
            "..." => Some(JokerId::Misprint),
            _ => None,
        }
    }

    /// 隨機生成 Common Joker
    pub fn random_common<R: rand::Rng>(rng: &mut R) -> Self {
        const COMMON_JOKERS: &[JokerId] = &[
            JokerId::Joker,
            JokerId::Misprint,
            JokerId::GreedyJoker,
            JokerId::LustyJoker,
            JokerId::WrathfulJoker,
            JokerId::GluttonousJoker,
            JokerId::JollyJoker,
            JokerId::ZanyJoker,
            JokerId::MadJoker,
            JokerId::CrazyJoker,
            JokerId::DrollJoker,
            JokerId::SlyJoker,
            JokerId::WilyJoker,
            JokerId::CleverJoker,
            JokerId::DeviousJoker,
            JokerId::CraftyJoker,
            JokerId::HalfJoker,
            JokerId::Banner,
            JokerId::ScaryFace,
            JokerId::EvenSteven,
            JokerId::OddTodd,
            JokerId::Scholar,
            JokerId::Egg,
            JokerId::Drunkard,
            JokerId::GreenJoker,
            JokerId::Fibonacci,
        ];
        COMMON_JOKERS[rng.gen_range(0..COMMON_JOKERS.len())]
    }

    /// 隨機生成 Rare Joker
    pub fn random_rare<R: rand::Rng>(rng: &mut R) -> Self {
        const RARE_JOKERS: &[JokerId] = &[
            JokerId::DNA,
            JokerId::Selzer,
            JokerId::Mime,
            JokerId::Triboulet,
            JokerId::DriversLicense,
            JokerId::Hit_The_Road,
            JokerId::Vampire,
            JokerId::Lucky_Cat,
            JokerId::Canio,
            JokerId::Caino,
            JokerId::Yorick,
            JokerId::Madness,
        ];
        RARE_JOKERS[rng.gen_range(0..RARE_JOKERS.len())]
    }

    /// 隨機生成 Legendary Joker
    pub fn random_legendary<R: rand::Rng>(rng: &mut R) -> Self {
        const LEGENDARY_JOKERS: &[JokerId] = &[
            JokerId::Canio,
            JokerId::Triboulet,
            // 可以添加更多 Legendary Joker
        ];
        LEGENDARY_JOKERS[rng.gen_range(0..LEGENDARY_JOKERS.len())]
    }
}

// ============================================================================
// Joker Bonus 結構
// ============================================================================

/// Joker 計算的加成結果
#[derive(Clone, Debug, Default)]
pub struct JokerBonus {
    pub chip_bonus: i64,
    pub add_mult: i64,
    pub mul_mult: f32,
    pub money_bonus: i64,
    pub retriggers: i32,
}

impl JokerBonus {
    pub fn new() -> Self {
        Self {
            chip_bonus: 0,
            add_mult: 0,
            mul_mult: 1.0,
            money_bonus: 0,
            retriggers: 0,
        }
    }

    /// 合併另一個 bonus
    pub fn merge(&mut self, other: &JokerBonus) {
        self.chip_bonus += other.chip_bonus;
        self.add_mult += other.add_mult;
        self.mul_mult *= other.mul_mult;
        self.money_bonus += other.money_bonus;
        self.retriggers += other.retriggers;
    }
}

// ============================================================================
// Joker 計分上下文
// ============================================================================

/// 計算 Joker 效果時的上下文資訊
pub struct ScoringContext<'a> {
    pub played_cards: &'a [Card],
    /// 手中持有的牌（非打出的牌）- 用於 Baron, ShootTheMoon 等
    pub hand: &'a [Card],
    pub hand_id: HandId,
    pub discards_remaining: i32,
    pub hands_remaining: i32,
    pub joker_count: usize,
    pub consecutive_non_face: i32,
    pub steel_cards_in_hand: i32,
    pub glass_cards_broken: i32,
    pub cards_added_to_deck: i32,
    pub money_held: i64,
    pub deck_size: i32,
    pub hands_played_this_round: i32,
    pub hands_played_this_run: i32,
    pub tarots_used_this_run: i32,
    pub planets_used_this_run: i32,
    pub is_final_hand: bool,
    pub is_first_hand: bool,
    pub blinds_skipped: i32,
    pub cards_discarded_this_run: i32,
    pub rerolls_this_run: i32,
    /// Boss Blind 能力是否觸發（用於 Matador）
    pub boss_ability_triggered: bool,
    /// 牌組中的 Stone 卡數量 (Stone Joker)
    pub stone_cards_in_deck: i32,
    /// Joker 槽位上限 (Stencil)
    pub joker_slot_limit: usize,
    /// Uncommon Joker 數量 (BaseballCard)
    pub uncommon_joker_count: usize,
    /// 牌組中增強牌數量 (DriversLicense)
    pub enhanced_cards_in_deck: i32,
    /// Mime: 手中持有牌的能力是否重觸發
    pub has_mime: bool,
}

impl<'a> ScoringContext<'a> {
    pub fn new(played_cards: &'a [Card], hand_id: HandId) -> Self {
        Self::with_hand(played_cards, &[], hand_id)
    }

    pub fn with_hand(played_cards: &'a [Card], hand: &'a [Card], hand_id: HandId) -> Self {
        Self {
            played_cards,
            hand,
            hand_id,
            discards_remaining: 0,
            hands_remaining: 0,
            joker_count: 0,
            consecutive_non_face: 0,
            steel_cards_in_hand: 0,
            glass_cards_broken: 0,
            cards_added_to_deck: 0,
            money_held: 0,
            deck_size: 52,
            hands_played_this_round: 0,
            hands_played_this_run: 0,
            tarots_used_this_run: 0,
            planets_used_this_run: 0,
            is_final_hand: false,
            is_first_hand: false,
            blinds_skipped: 0,
            cards_discarded_this_run: 0,
            rerolls_this_run: 0,
            boss_ability_triggered: false,
            stone_cards_in_deck: 0,
            joker_slot_limit: 5, // 默認 5 個槽位
            uncommon_joker_count: 0,
            enhanced_cards_in_deck: 0,
            has_mime: false,
        }
    }
}

// ============================================================================
// Tier 1: Core Joker 效果計算 (靜態分發，最快)
// ============================================================================

/// 計算單個 Core Joker 的效果
///
/// # 已棄用
/// 此函數已被 `compute_joker_effect_v2` 取代。
/// 所有生產代碼路徑現在都使用 V2 模板系統。
/// 此函數僅保留以支持舊測試，將在 Phase 4 中完全移除。
///
/// 請使用 `compute_joker_effect_with_state_v2` 或 `compute_joker_bonus_v2` 替代。
#[deprecated(since = "0.2.0", note = "Use compute_joker_effect_v2 instead")]
pub fn compute_core_joker_effect(id: JokerId, ctx: &ScoringContext, rng_value: u8) -> JokerBonus {
    let mut bonus = JokerBonus::new();

    match id {
        // ====== 基礎 Mult Jokers ======
        JokerId::Joker => bonus.add_mult += 4,

        // ====== 花色相關 ======
        JokerId::GreedyJoker => {
            let diamonds = ctx.played_cards.iter().filter(|c| c.suit == 1).count();
            bonus.money_bonus += diamonds as i64 * 3;
        }
        JokerId::LustyJoker => {
            let hearts = ctx.played_cards.iter().filter(|c| c.suit == 2).count();
            bonus.money_bonus += hearts as i64 * 3;
        }
        JokerId::WrathfulJoker => {
            let spades = ctx.played_cards.iter().filter(|c| c.suit == 0).count();
            bonus.money_bonus += spades as i64 * 3;
        }
        JokerId::GluttonousJoker => {
            let clubs = ctx.played_cards.iter().filter(|c| c.suit == 3).count();
            bonus.money_bonus += clubs as i64 * 3;
        }

        // ====== Pair 牌型加成 ======
        JokerId::JollyJoker => {
            // 包含 Pair 的牌型: Pair, TwoPair, FullHouse, FlushHouse
            if matches!(
                ctx.hand_id,
                HandId::Pair | HandId::TwoPair | HandId::FullHouse | HandId::FlushHouse
            ) {
                bonus.add_mult += 8;
            }
        }
        JokerId::SlyJoker => {
            if matches!(
                ctx.hand_id,
                HandId::Pair | HandId::TwoPair | HandId::FullHouse | HandId::FlushHouse
            ) {
                bonus.chip_bonus += 50;
            }
        }

        // ====== Three of a Kind 加成 ======
        JokerId::ZanyJoker => {
            // 包含 Three of a Kind 的牌型: ThreeKind, FullHouse, FourKind, FiveKind, FlushHouse, FlushFive
            if matches!(
                ctx.hand_id,
                HandId::ThreeKind
                    | HandId::FullHouse
                    | HandId::FourKind
                    | HandId::FiveKind
                    | HandId::FlushHouse
                    | HandId::FlushFive
            ) {
                bonus.add_mult += 12;
            }
        }
        JokerId::WilyJoker => {
            if matches!(
                ctx.hand_id,
                HandId::ThreeKind
                    | HandId::FullHouse
                    | HandId::FourKind
                    | HandId::FiveKind
                    | HandId::FlushHouse
                    | HandId::FlushFive
            ) {
                bonus.chip_bonus += 100;
            }
        }

        // ====== Two Pair 加成 ======
        JokerId::MadJoker => {
            if ctx.hand_id == HandId::TwoPair {
                bonus.add_mult += 10;
            }
        }
        JokerId::CleverJoker => {
            if ctx.hand_id == HandId::TwoPair {
                bonus.chip_bonus += 80;
            }
        }

        // ====== Straight 加成 ======
        JokerId::CrazyJoker => {
            if matches!(
                ctx.hand_id,
                HandId::Straight | HandId::StraightFlush | HandId::RoyalFlush
            ) {
                bonus.add_mult += 12;
            }
        }
        JokerId::DeviousJoker => {
            if matches!(
                ctx.hand_id,
                HandId::Straight | HandId::StraightFlush | HandId::RoyalFlush
            ) {
                bonus.chip_bonus += 100;
            }
        }

        // ====== Flush 加成 ======
        JokerId::DrollJoker => {
            // 包含 Flush 的牌型: Flush, StraightFlush, RoyalFlush, FlushHouse, FlushFive
            if matches!(
                ctx.hand_id,
                HandId::Flush
                    | HandId::StraightFlush
                    | HandId::RoyalFlush
                    | HandId::FlushHouse
                    | HandId::FlushFive
            ) {
                bonus.add_mult += 10;
            }
        }
        JokerId::CraftyJoker => {
            if matches!(
                ctx.hand_id,
                HandId::Flush
                    | HandId::StraightFlush
                    | HandId::RoyalFlush
                    | HandId::FlushHouse
                    | HandId::FlushFive
            ) {
                bonus.chip_bonus += 80;
            }
        }

        // ====== 條件類 ======
        JokerId::HalfJoker => {
            if ctx.played_cards.len() <= 3 {
                bonus.add_mult += 20;
            }
        }
        JokerId::Banner => {
            bonus.chip_bonus += ctx.discards_remaining as i64 * 30;
        }
        JokerId::MysticSummit => {
            if ctx.discards_remaining == 0 {
                bonus.add_mult += 15;
            }
        }
        JokerId::Misprint => {
            bonus.add_mult += (rng_value % 24) as i64;
        }
        JokerId::AbstractJoker => {
            bonus.add_mult += ctx.joker_count as i64 * 3;
        }
        JokerId::BaseballCard => {
            // X1.5 Mult for each Uncommon Joker held
            if ctx.uncommon_joker_count > 0 {
                bonus.mul_mult *= (1.5_f32).powi(ctx.uncommon_joker_count as i32);
            }
        }

        // ====== X Mult Jokers ======
        JokerId::RideTheBus => {
            bonus.add_mult += ctx.consecutive_non_face as i64;
        }
        JokerId::SteelJoker => {
            bonus.mul_mult *= 1.0 + (ctx.steel_cards_in_hand as f32 * 0.2);
        }
        JokerId::GlassJoker => {
            bonus.mul_mult *= 1.0 + (ctx.glass_cards_broken as f32 * 0.75);
        }
        JokerId::Hologram => {
            bonus.mul_mult *= 1.0 + (ctx.cards_added_to_deck as f32 * 0.25);
        }

        // ====== 經濟類 ======
        JokerId::GoldenJoker => {
            bonus.money_bonus += 4;
        }
        JokerId::Bull => {
            bonus.chip_bonus += ctx.money_held * 2;
        }
        JokerId::ToTheMoon => {
            bonus.money_bonus += ctx.money_held / 5;
        }
        JokerId::Supernova => {
            bonus.add_mult += ctx.hands_played_this_run as i64;
        }
        JokerId::Erosion => {
            let cards_below_52 = (52 - ctx.deck_size).max(0);
            bonus.add_mult += cards_below_52 as i64 * 4;
        }
        JokerId::SquareJoker => {
            // SquareJoker: 牌組正好 52 張時，每張打出的牌 +4 Mult
            if ctx.deck_size == 52 {
                bonus.add_mult += ctx.played_cards.len() as i64 * 4;
            }
        }

        // ====== 卡牌計數類 ======
        JokerId::BlueJoker => {
            bonus.chip_bonus += ctx.deck_size as i64 * 2;
        }
        JokerId::Fibonacci => {
            let fib_cards = ctx
                .played_cards
                .iter()
                .filter(|c| matches!(c.rank, 1 | 2 | 3 | 5 | 8))
                .count();
            bonus.add_mult += fib_cards as i64 * 8;
        }
        JokerId::ScaryFace => {
            let face_count = ctx
                .played_cards
                .iter()
                .filter(|c| c.rank >= 11 && c.rank <= 13)
                .count();
            bonus.chip_bonus += face_count as i64 * 30;
        }
        JokerId::EvenSteven => {
            // 偶數牌: 2, 4, 6, 8, 10 (不包含 J=11, Q=12, K=13)
            let even_count = ctx
                .played_cards
                .iter()
                .filter(|c| c.rank <= 10 && c.rank % 2 == 0)
                .count();
            bonus.add_mult += even_count as i64 * 4;
        }
        JokerId::OddTodd => {
            // 奇數牌: A(1), 3, 5, 7, 9 (不包含 J=11, K=13)
            let odd_count = ctx
                .played_cards
                .iter()
                .filter(|c| c.rank == 1 || (c.rank <= 9 && c.rank % 2 == 1))
                .count();
            bonus.chip_bonus += odd_count as i64 * 31;
        }
        JokerId::Scholar => {
            let ace_count = ctx
                .played_cards
                .iter()
                .filter(|c| c.rank == 1 || c.rank == 14)
                .count();
            bonus.chip_bonus += ace_count as i64 * 20;
            bonus.add_mult += ace_count as i64 * 4;
        }
        JokerId::Smiley => {
            let face_count = ctx
                .played_cards
                .iter()
                .filter(|c| c.rank >= 11 && c.rank <= 13)
                .count();
            bonus.add_mult += face_count as i64 * 5;
        }
        JokerId::BusinessCard => {
            // 1/2 chance +$2 per face card played
            let face_count = ctx
                .played_cards
                .iter()
                .filter(|c| c.rank >= 11 && c.rank <= 13)
                .count();
            // Use rng_value for 50% chance per face card
            for i in 0..face_count {
                if ((rng_value >> (i % 8)) & 1) == 0 {
                    bonus.money_bonus += 2;
                }
            }
        }

        // ====== 乘法類 ======
        JokerId::DuskJoker => {
            if ctx.is_final_hand {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::Acrobat => {
            if ctx.is_final_hand {
                bonus.mul_mult *= 3.0;
            }
        }
        JokerId::DNA => {
            // DNA: 每回合第一手牌觸發兩次 (X2 分數效果)
            if ctx.is_first_hand {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::Photograph => {
            if ctx
                .played_cards
                .iter()
                .any(|c| c.rank >= 11 && c.rank <= 13)
            {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::Bloodstone => {
            let hearts = ctx.played_cards.iter().filter(|c| c.suit == 2).count();
            if hearts > 0 && (rng_value % 2) == 0 {
                bonus.mul_mult *= 1.5;
            }
        }
        JokerId::Arrowhead => {
            let spades = ctx.played_cards.iter().filter(|c| c.suit == 0).count();
            bonus.chip_bonus += spades as i64 * 50;
        }
        JokerId::Onyx => {
            let clubs = ctx.played_cards.iter().filter(|c| c.suit == 3).count();
            bonus.add_mult += clubs as i64 * 80;
        }
        JokerId::Opal => {
            let diamonds = ctx.played_cards.iter().filter(|c| c.suit == 1).count();
            if diamonds > 0 {
                bonus.mul_mult *= 1.5f32.powi(diamonds as i32);
            }
        }
        JokerId::RoughGem => {
            // +$1 per Diamond played
            let diamonds = ctx.played_cards.iter().filter(|c| c.suit == 1).count();
            bonus.money_bonus += diamonds as i64;
        }
        JokerId::Ticket => {
            // +$1 per Gold enhancement card played
            let gold_cards = ctx
                .played_cards
                .iter()
                .filter(|c| c.enhancement == Enhancement::Gold)
                .count();
            bonus.money_bonus += gold_cards as i64;
        }

        // ====== Walkie Talkie ======
        JokerId::Walkie => {
            // +10 Mult if hand contains a 10 or 4
            let has_10_or_4 = ctx.played_cards.iter().any(|c| c.rank == 10 || c.rank == 4);
            if has_10_or_4 {
                bonus.add_mult += 10;
            }
        }

        // ====== The X 系列 ======
        JokerId::The_Duo => {
            if matches!(
                ctx.hand_id,
                HandId::Pair
                    | HandId::TwoPair
                    | HandId::FullHouse
                    | HandId::ThreeKind
                    | HandId::FourKind
                    | HandId::FiveKind
            ) {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::The_Trio => {
            if matches!(
                ctx.hand_id,
                HandId::ThreeKind | HandId::FullHouse | HandId::FourKind | HandId::FiveKind
            ) {
                bonus.mul_mult *= 3.0;
            }
        }
        JokerId::The_Family => {
            if matches!(ctx.hand_id, HandId::FourKind | HandId::FiveKind) {
                bonus.mul_mult *= 4.0;
            }
        }
        JokerId::The_Order => {
            if matches!(
                ctx.hand_id,
                HandId::Straight | HandId::StraightFlush | HandId::RoyalFlush
            ) {
                bonus.mul_mult *= 3.0;
            }
        }
        JokerId::The_Tribe => {
            if matches!(
                ctx.hand_id,
                HandId::Flush
                    | HandId::StraightFlush
                    | HandId::RoyalFlush
                    | HandId::FlushHouse
                    | HandId::FlushFive
            ) {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::SuperPosition => {
            // X2 Mult when hand is both a Straight AND a Flush (Straight Flush or Royal Flush)
            if matches!(ctx.hand_id, HandId::StraightFlush | HandId::RoyalFlush) {
                bonus.mul_mult *= 2.0;
            }
        }

        // ====== 消耗品計數類 ======
        JokerId::FortuneTeller => {
            bonus.add_mult += ctx.tarots_used_this_run as i64;
        }
        JokerId::Constellation => {
            bonus.mul_mult *= 1.0 + (ctx.planets_used_this_run as f32 * 0.1);
        }

        // ====== 重觸發類 ======
        JokerId::SockAndBuskin => {
            let face_count = ctx
                .played_cards
                .iter()
                .filter(|c| c.rank >= 11 && c.rank <= 13)
                .count();
            bonus.retriggers += face_count as i32;
        }
        JokerId::HangingChad => {
            if !ctx.played_cards.is_empty() {
                bonus.retriggers += 1;
            }
        }
        JokerId::Hack => {
            let low_cards = ctx
                .played_cards
                .iter()
                .filter(|c| matches!(c.rank, 2 | 3 | 4 | 5))
                .count();
            bonus.retriggers += low_cards as i32;
        }

        // ====== 其他狀態類 ======
        JokerId::Throwback => {
            bonus.mul_mult *= 1.0 + (ctx.blinds_skipped as f32 * 0.25);
        }
        JokerId::RedCard => {
            // +3 Mult per skipped Blind this run
            bonus.add_mult += ctx.blinds_skipped as i64 * 3;
        }
        JokerId::Flash => {
            // +2 Mult per reroll this run
            bonus.add_mult += ctx.rerolls_this_run as i64 * 2;
        }
        JokerId::Baron => {
            // Each King held in hand (not played) gives X1.5 Mult
            // Mime: 效果觸發兩次
            let kings_in_hand = ctx.hand.iter().filter(|c| c.rank == 13).count();
            if kings_in_hand > 0 {
                let trigger_count = if ctx.has_mime { 2 } else { 1 };
                bonus.mul_mult *= 1.5f32.powi((kings_in_hand * trigger_count) as i32);
            }
        }
        JokerId::ShootTheMoon => {
            // Each Queen held in hand (not played) gives +13 Mult
            // Mime: 效果觸發兩次
            let queens_in_hand = ctx.hand.iter().filter(|c| c.rank == 12).count();
            let trigger_count = if ctx.has_mime { 2 } else { 1 };
            bonus.add_mult += queens_in_hand as i64 * 13 * trigger_count as i64;
        }
        JokerId::Swashbuckler => {
            // Each card below 8 held in hand gives +2 Mult (ranks 2-7)
            let below_8_count = ctx
                .hand
                .iter()
                .filter(|c| c.rank >= 2 && c.rank <= 7)
                .count();
            bonus.add_mult += below_8_count as i64 * 2;
        }
        JokerId::RaisedFist => {
            // Lowest held card's rank × 2 as Mult (Ace counts as 14)
            if let Some(lowest) = ctx.hand.iter().min_by_key(|c| c.rank) {
                bonus.add_mult += (lowest.rank as i64) * 2;
            }
        }
        JokerId::Courier => {
            // +25 Chips per card below Ace held in hand (ranks 2-13, i.e., not Ace)
            let below_ace_count = ctx.hand.iter().filter(|c| c.rank < 14).count();
            bonus.chip_bonus += below_ace_count as i64 * 25;
        }
        JokerId::Card_Sharp => {
            if ctx.hands_played_this_round > 0 {
                bonus.mul_mult *= 3.0;
            }
        }
        JokerId::Cavendish => {
            bonus.mul_mult *= 3.0;
            // 1/1000 chance to self-destruct handled elsewhere
        }
        JokerId::Trousers | JokerId::Spare_Trousers => {
            if ctx.hand_id == HandId::TwoPair {
                bonus.add_mult += 4;
            }
        }
        JokerId::Square => {
            if ctx.played_cards.len() == 4 {
                bonus.chip_bonus += 4;
            }
        }
        JokerId::Runner => {
            if matches!(
                ctx.hand_id,
                HandId::Straight | HandId::StraightFlush | HandId::RoyalFlush
            ) {
                bonus.chip_bonus += 15;
            }
        }
        JokerId::Stuntman => {
            // +250 Chips (hand size -2 handled in game state)
            bonus.chip_bonus += 250;
        }
        JokerId::Bootstraps => {
            if ctx.money_held > 0 {
                bonus.add_mult += (ctx.money_held / 5) * 2;
            }
        }

        // 規則修改類（不直接加分，影響其他系統）
        JokerId::FourFingers
        | JokerId::Shortcut
        | JokerId::Splash
        | JokerId::Smeared
        | JokerId::Pareidolia
        | JokerId::Chicot => {
            // 這些會影響計分規則，不在這裡處理
        }

        // ====== 條件觸發類 (X2 Mult) ======
        JokerId::Even_Steven => {
            // X2 Mult if ALL scoring cards are even (2, 4, 6, 8, 10)
            let all_even = ctx
                .played_cards
                .iter()
                .all(|c| c.rank <= 10 && c.rank % 2 == 0);
            if all_even && !ctx.played_cards.is_empty() {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::Odd_Todd_2 => {
            // X2 Mult if ALL scoring cards are odd (A, 3, 5, 7, 9)
            let all_odd = ctx
                .played_cards
                .iter()
                .all(|c| c.rank == 1 || (c.rank <= 9 && c.rank % 2 == 1));
            if all_odd && !ctx.played_cards.is_empty() {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::Seeing_Double => {
            // X2 Mult if hand contains a Club AND at least one other suit
            let has_club = ctx.played_cards.iter().any(|c| c.suit == 0); // Club = 0
            let has_other_suit = ctx.played_cards.iter().any(|c| c.suit != 0);
            if has_club && has_other_suit {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::Flower_Pot => {
            // X3 Mult if hand contains all 4 suits (Diamond, Club, Heart, Spade)
            let suits: std::collections::HashSet<u8> =
                ctx.played_cards.iter().map(|c| c.suit).collect();
            if suits.len() >= 4 {
                bonus.mul_mult *= 3.0;
            }
        }
        JokerId::Stone => {
            // +25 Chips for each Stone card in the full deck
            bonus.chip_bonus += ctx.stone_cards_in_deck as i64 * 25;
        }
        JokerId::DriversLicense => {
            // X3 Mult if you have 16+ enhanced cards in deck
            if ctx.enhanced_cards_in_deck >= 16 {
                bonus.mul_mult *= 3.0;
            }
        }
        JokerId::Stencil => {
            // X1 Mult for each empty Joker slot
            let empty_slots = ctx.joker_slot_limit.saturating_sub(ctx.joker_count);
            if empty_slots > 0 {
                bonus.mul_mult *= empty_slots as f32;
            }
        }
        JokerId::Triboulet => {
            // Kings and Queens each give X2 Mult
            let kq_count = ctx
                .played_cards
                .iter()
                .filter(|c| c.rank == 13 || c.rank == 12)
                .count();
            if kq_count > 0 {
                bonus.mul_mult *= (2.0_f32).powi(kq_count as i32);
            }
        }
        JokerId::Blackboard => {
            // X3 Mult if all held cards are Spades or Clubs (black suits)
            let all_black = ctx.hand.iter().all(|c| c.suit == 0 || c.suit == 3); // Club=0, Spade=3
            if all_black && !ctx.hand.is_empty() {
                bonus.mul_mult *= 3.0;
            }
        }

        // ====== 特殊效果類 ======
        JokerId::Gros_Michel => {
            // +15 Mult, 1/15 chance to self-destruct at round end (handled elsewhere)
            bonus.add_mult += 15;
        }
        JokerId::Matador => {
            // +$8 when Boss Blind ability triggers
            if ctx.boss_ability_triggered {
                bonus.money_bonus += 8;
            }
        }
        JokerId::MrBones => {
            // Prevents death if chips > 25% of requirement
            // Death prevention handled at game level, no scoring bonus
        }
        JokerId::Luchador => {
            // Sell to disable current Boss Blind effect
            // Sell effect handled at game level, no scoring bonus
        }
        JokerId::Ceremonial => {
            // On select_blind, destroy rightmost Joker, gain 2x sell value as Mult
            // The mult bonus is stored in joker.counter after destruction event
            // This is processed separately via compute_joker_bonus_with_state
        }
        JokerId::InvisibleJoker => {
            // After 2 rounds, sell to duplicate adjacent Joker
            // Round tracking and duplication handled at game level
        }

        // 未實作的保留位
        _ => {}
    }

    bonus
}

// ============================================================================
// Joker Slot 結構
// ============================================================================

/// Joker 欄位
#[derive(Clone, Debug)]
pub struct JokerSlot {
    pub id: JokerId,
    pub enabled: bool,
    pub sell_value: i64,
    pub counter: i32,
    pub is_eternal: bool,
    pub is_negative: bool,
    pub is_perishable: bool,
    pub perishable_rounds: i32, // 剩餘回合數，到 0 時銷毀
    pub is_rental: bool,        // 每回合 $3，付不起則銷毀
    pub edition: Edition,
    pub x_mult_accumulated: f32,

    /// 統一狀態系統
    pub state: JokerState,

    // 尚未遷移到 JokerState 的欄位
    pub flash_card_mult: i32, // Flash: 累積 Mult (+2 per reroll)
    pub red_card_mult: i32,   // RedCard: 累積 Mult (+3 per skip blind)
}

impl JokerSlot {
    pub fn new(id: JokerId) -> Self {
        Self {
            id,
            enabled: true,
            sell_value: id.base_cost() / 2,
            counter: 0,
            is_eternal: false,
            is_negative: false,
            is_perishable: false,
            perishable_rounds: 5, // Perishable 預設 5 回合後消失
            is_rental: false,
            edition: Edition::Base,
            x_mult_accumulated: 1.0,
            // 根據 Joker 類型初始化統一狀態
            state: match id {
                // X Mult Accumulators (起始 X1.0)
                JokerId::Vampire | JokerId::Canio | JokerId::Lucky_Cat |
                JokerId::Hologram | JokerId::Constellation |
                JokerId::Yorick | JokerId::GlassJoker | JokerId::Hit_The_Road |
                JokerId::Campfire | JokerId::Wee | JokerId::Merry |
                JokerId::GreenJoker | JokerId::RideTheBus => JokerState::Accumulator {
                    chips: 0,
                    mult: 0,
                    x_mult: 1.0,
                },
                // 特殊初始值的 Accumulators
                JokerId::Madness => JokerState::Accumulator {
                    chips: 0,
                    mult: 0,
                    x_mult: 0.5, // Madness 起始 X0.5 Mult
                },
                JokerId::Ramen => JokerState::Accumulator {
                    chips: 0,
                    mult: 0,
                    x_mult: 2.0, // Ramen 起始 X2.0 Mult
                },
                JokerId::IceCream => JokerState::Accumulator {
                    chips: 100, // IceCream 起始 100 Chips
                    mult: 0,
                    x_mult: 1.0,
                },
                JokerId::Popcorn => JokerState::Accumulator {
                    chips: 0,
                    mult: 20, // Popcorn 起始 20 Mult
                    x_mult: 1.0,
                },
                // Target 狀態 Jokers
                JokerId::AncientJoker => JokerState::Target {
                    suit: 0, // 初始為 Diamonds，每回合隨機變化
                    rank: 0,
                    value: 0,
                },
                JokerId::Castle => JokerState::Target {
                    suit: 0,  // 初始花色 Diamonds，每回合隨機變化
                    rank: 0,
                    value: 0, // 累積的 chips
                },
                JokerId::TheIdol => JokerState::Target {
                    suit: 0,  // 初始花色，每回合隨機變化
                    rank: 1,  // 初始點數 Ace，每回合隨機變化
                    value: 0,
                },
                // Counter 狀態 Jokers
                JokerId::Obelisk => JokerState::Counter {
                    current: 0,     // 連續非最常打牌型次數
                    threshold: 0,   // 不使用閾值
                    bonus_mult: 1.0,
                },
                JokerId::Selzer => JokerState::Counter {
                    current: 10,    // 起始 10 次重觸發
                    threshold: 0,   // 到 0 時自毀
                    bonus_mult: 1.0,
                },
                JokerId::TurtleBean => JokerState::Counter {
                    current: 5,     // 起始 +5 手牌大小
                    threshold: 0,   // 到 0 時自毀
                    bonus_mult: 1.0,
                },
                JokerId::ChaosTheClown => JokerState::Counter {
                    current: 0,     // 0 = 未使用免費 reroll, 1 = 已使用
                    threshold: 1,   // 閾值 1（每回合重置為 0）
                    bonus_mult: 1.0,
                },
                JokerId::TradingCard => JokerState::Counter {
                    current: 0,     // 0 = 未觸發, 1 = 已觸發
                    threshold: 1,   // 一次性觸發
                    bonus_mult: 1.0,
                },
                // Target 狀態 Jokers（額外）
                JokerId::ToDoList => JokerState::Target {
                    suit: 0,
                    rank: 0,    // 牌型索引，在購買時隨機設置
                    value: 4,   // +$4 獎勵
                },
                // Accumulator 狀態 Jokers（額外）
                JokerId::Rocket => JokerState::Accumulator {
                    chips: 0,
                    mult: 1,    // 起始每回合 +$1
                    x_mult: 1.0,
                },
                _ => JokerState::None,
            },
            flash_card_mult: 0,
            red_card_mult: 0,
        }
    }

    pub fn with_sell_value(mut self, value: i64) -> Self {
        self.sell_value = value;
        self
    }

    pub fn eternal(mut self) -> Self {
        self.is_eternal = true;
        self
    }

    pub fn negative(mut self) -> Self {
        self.is_negative = true;
        self
    }

    pub fn perishable(mut self) -> Self {
        self.is_perishable = true;
        self.perishable_rounds = 5; // 5 回合後消失
        self
    }

    pub fn rental(mut self) -> Self {
        self.is_rental = true;
        self
    }

    // ====== JokerState 狀態更新方法 ======

    /// Vampire: 吸收增強時調用 (+0.1 X Mult per enhancement)
    pub fn update_vampire_on_enhancement(&mut self, enhancements_absorbed: i32) {
        if self.id == JokerId::Vampire {
            self.state.add_x_mult(enhancements_absorbed as f32 * 0.1);
        }
    }

    /// Canio: 銷毀人頭牌時調用 (+1.0 X Mult per face card)
    pub fn update_canio_on_face_destroyed(&mut self, face_cards_destroyed: i32) {
        if self.id == JokerId::Canio {
            self.state.add_x_mult(face_cards_destroyed as f32);
        }
    }

    /// Lucky Cat: Lucky 牌觸發時調用 (+0.25 X Mult per trigger)
    pub fn update_lucky_cat_on_trigger(&mut self, triggers: i32) {
        if self.id == JokerId::Lucky_Cat {
            self.state.add_x_mult(triggers as f32 * 0.25);
        }
    }

    /// Hologram: 加牌到牌組時調用 (+0.25 X Mult per card)
    pub fn update_hologram_on_card_added(&mut self, cards_added: i32) {
        if self.id == JokerId::Hologram {
            self.state.add_x_mult(cards_added as f32 * 0.25);
        }
    }

    /// Constellation: 使用行星牌時調用 (+0.1 X Mult per planet)
    pub fn update_constellation_on_planet_used(&mut self) {
        if self.id == JokerId::Constellation {
            self.state.add_x_mult(0.1);
        }
    }

    /// Madness: 銷毀 Joker 時調用 (+0.5 X Mult per Joker destroyed)
    pub fn update_madness_on_joker_destroyed(&mut self, jokers_destroyed: i32) {
        if self.id == JokerId::Madness {
            self.state.add_x_mult(jokers_destroyed as f32 * 0.5);
        }
    }

    /// Yorick: 棄牌時調用 (每 23 張 +1.0 X Mult)
    /// Yorick 使用 Accumulator 狀態：x_mult 追蹤累積倍率，chips 追蹤棄牌計數
    pub fn update_yorick_on_discard(&mut self, cards_discarded: i32) {
        if self.id == JokerId::Yorick {
            if let JokerState::Accumulator { chips, x_mult, .. } = &mut self.state {
                *chips += cards_discarded;
                while *chips >= 23 {
                    *chips -= 23;
                    *x_mult += 1.0;
                }
            }
        }
    }

    /// Glass Joker: Glass 牌碎裂時調用 (+0.75 X Mult per glass broken)
    pub fn update_glass_on_break(&mut self, glass_broken: i32) {
        if self.id == JokerId::GlassJoker {
            self.state.add_x_mult(glass_broken as f32 * 0.75);
        }
    }

    /// AncientJoker: 設置當前花色 (每回合開始時隨機調用)
    pub fn set_ancient_suit(&mut self, suit: u8) {
        if self.id == JokerId::AncientJoker {
            self.state.set_target_suit(suit % 4);
        }
    }

    /// Castle: 設置當前花色 (每回合開始時隨機調用)
    pub fn set_castle_suit(&mut self, suit: u8) {
        if self.id == JokerId::Castle {
            self.state.set_target_suit(suit % 4);
        }
    }

    /// Castle: 棄牌時調用 (如果花色匹配，+3 Chips)
    pub fn update_castle_on_discard(&mut self, discarded_suit: u8) {
        if self.id == JokerId::Castle {
            if discarded_suit == self.state.get_target_suit() {
                self.state.add_target_value(3);
            }
        }
    }

    /// TheIdol: 設置目標牌 (每回合開始時隨機調用)
    pub fn set_idol_target(&mut self, rank: u8, suit: u8) {
        if self.id == JokerId::TheIdol {
            self.state.set_target_rank(rank);
            self.state.set_target_suit(suit);
        }
    }

    /// Hit The Road: 棄 Jack 時調用 (+0.5 X Mult per Jack)
    pub fn update_hit_the_road_on_jack_discard(&mut self, jacks_discarded: i32) {
        if self.id == JokerId::Hit_The_Road {
            self.state.add_x_mult(jacks_discarded as f32 * 0.5);
        }
    }

    /// 獲取此 Joker 的 X Mult 值（用於計分）
    pub fn get_x_mult(&self) -> f32 {
        self.state.get_x_mult()
    }

    /// Wee: 每輪開始時增加 chips (+8 Chips per round)
    pub fn update_wee_on_round(&mut self) {
        if self.id == JokerId::Wee {
            self.state.add_chips(8);
        }
    }

    /// Merry: 每輪開始時增加 mult (+3 Mult per round)
    pub fn update_merry_on_round(&mut self) {
        if self.id == JokerId::Merry {
            self.state.add_mult(3);
        }
    }

    /// GreenJoker: 每回合重置 mult
    pub fn reset_green_joker(&mut self) {
        if self.id == JokerId::GreenJoker {
            if let JokerState::Accumulator { mult, .. } = &mut self.state {
                *mult = 0;
            }
        }
    }

    /// GreenJoker: 每手牌增加 mult (+1 Mult per hand)
    pub fn update_green_joker_on_hand(&mut self) {
        if self.id == JokerId::GreenJoker {
            self.state.add_mult(1);
        }
    }

    /// RideTheBus: 重置 mult（出現人頭牌時）
    pub fn reset_ride_the_bus(&mut self) {
        if self.id == JokerId::RideTheBus {
            if let JokerState::Accumulator { mult, .. } = &mut self.state {
                *mult = 0;
            }
        }
    }

    /// RideTheBus: 增加 mult（無人頭牌時，+1 Mult per hand）
    pub fn update_ride_the_bus_on_hand(&mut self) {
        if self.id == JokerId::RideTheBus {
            self.state.add_mult(1);
        }
    }

    /// IceCream: 每手牌減少 chips，返回是否應該銷毀
    pub fn update_ice_cream_on_hand(&mut self) -> bool {
        if self.id == JokerId::IceCream {
            self.state.add_chips(-5);
            return self.state.get_chips() <= 0;
        }
        false
    }

    /// Popcorn: 每輪減少 mult，返回是否應該銷毀
    pub fn update_popcorn_on_round(&mut self) -> bool {
        if self.id == JokerId::Popcorn {
            self.state.add_mult(-4);
            return self.state.get_mult() <= 0;
        }
        false
    }

    /// Obelisk: 重置連續計數（打出最常打牌型時）
    pub fn reset_obelisk_streak(&mut self) {
        if self.id == JokerId::Obelisk {
            self.state.reset_counter();
        }
    }

    /// Obelisk: 增加連續計數（打出非最常打牌型時）
    pub fn increment_obelisk_streak(&mut self) {
        if self.id == JokerId::Obelisk {
            if let JokerState::Counter { current, .. } = &mut self.state {
                *current += 1;
            }
        }
    }

    /// Selzer: 獲取剩餘重觸發次數
    pub fn get_selzer_charges(&self) -> i32 {
        if self.id == JokerId::Selzer {
            return self.state.get_counter();
        }
        0
    }

    /// Selzer: 扣減已使用的 charges，返回是否應該銷毀
    pub fn use_selzer_charges(&mut self, used: i32) -> bool {
        if self.id == JokerId::Selzer {
            if let JokerState::Counter { current, .. } = &mut self.state {
                *current -= used;
            }
            return self.state.get_counter() <= 0;
        }
        false
    }

    /// TurtleBean: 每輪 -1 手牌大小，返回是否應該銷毀
    pub fn update_turtle_bean_on_round(&mut self) -> bool {
        if self.id == JokerId::TurtleBean {
            if let JokerState::Counter { current, .. } = &mut self.state {
                *current -= 1;
            }
            return self.state.get_counter() <= 0;
        }
        false
    }

    /// TurtleBean: 獲取當前手牌大小加成
    pub fn get_turtle_hand_mod(&self) -> i32 {
        if self.id == JokerId::TurtleBean {
            return self.state.get_counter();
        }
        0
    }

    /// Rocket: 過 Boss Blind 後增加每回合獎勵
    pub fn increment_rocket_money(&mut self) {
        if self.id == JokerId::Rocket {
            self.state.add_mult(1);
        }
    }

    /// Rocket: 獲取當前每回合獎勵金額
    pub fn get_rocket_money(&self) -> i32 {
        if self.id == JokerId::Rocket {
            return self.state.get_mult();
        }
        0
    }

    /// ToDoList: 設置目標牌型
    pub fn set_todo_hand_type(&mut self, hand_type: u8) {
        if self.id == JokerId::ToDoList {
            self.state.set_target_rank(hand_type);
        }
    }

    /// ToDoList: 獲取目標牌型
    pub fn get_todo_hand_type(&self) -> u8 {
        if self.id == JokerId::ToDoList {
            return self.state.get_target_rank();
        }
        0
    }

    /// ChaosTheClown: 重置免費 reroll（每回合開始）
    pub fn reset_chaos_free_reroll(&mut self) {
        if self.id == JokerId::ChaosTheClown {
            self.state.reset_counter();
        }
    }

    /// ChaosTheClown: 檢查是否有免費 reroll 可用
    pub fn has_chaos_free_reroll(&self) -> bool {
        if self.id == JokerId::ChaosTheClown {
            return self.state.get_counter() == 0;
        }
        false
    }

    /// ChaosTheClown: 標記免費 reroll 已使用
    pub fn use_chaos_free_reroll(&mut self) {
        if self.id == JokerId::ChaosTheClown {
            if let JokerState::Counter { current, .. } = &mut self.state {
                *current = 1;
            }
        }
    }

    /// TradingCard: 檢查是否已觸發
    pub fn is_trading_card_triggered(&self) -> bool {
        if self.id == JokerId::TradingCard {
            return self.state.get_counter() > 0;
        }
        false
    }

    /// TradingCard: 標記為已觸發
    pub fn trigger_trading_card(&mut self) {
        if self.id == JokerId::TradingCard {
            if let JokerState::Counter { current, .. } = &mut self.state {
                *current = 1;
            }
        }
    }
}

/// 計算所有 Joker 的總加成
pub fn compute_joker_bonus(
    jokers: &[JokerSlot],
    ctx: &ScoringContext,
    rng_values: &[u8],
) -> JokerBonus {
    let mut total = JokerBonus::new();

    // 收集所有 enabled 的 Joker 及其原始索引
    let enabled_jokers: Vec<(usize, &JokerSlot)> = jokers
        .iter()
        .enumerate()
        .filter(|(_, j)| j.enabled)
        .collect();

    for (idx_in_enabled, &(original_idx, joker)) in enabled_jokers.iter().enumerate() {
        let rng_val = rng_values.get(idx_in_enabled).copied().unwrap_or(0);

        // 檢查是否為複製類 Joker
        let effect = match joker.id {
            JokerId::Blueprint => {
                // Blueprint: 複製右邊第一個非複製類 Joker 的能力
                if let Some(target) = find_copy_target_right(jokers, original_idx) {
                    compute_joker_effect_with_state(target, ctx, rng_val)
                } else {
                    JokerBonus::new() // 右邊沒有可複製的 Joker
                }
            }
            JokerId::Brainstorm => {
                // Brainstorm: 複製最左邊第一個非複製類 Joker 的能力
                if let Some(target) = find_copy_target_leftmost(jokers) {
                    compute_joker_effect_with_state(target, ctx, rng_val)
                } else {
                    JokerBonus::new() // 沒有可複製的 Joker
                }
            }
            _ => compute_joker_effect_with_state(joker, ctx, rng_val),
        };
        total.merge(&effect);
    }

    total
}

/// 找到 Blueprint 複製的目標（右邊第一個非複製類 Joker）
fn find_copy_target_right(jokers: &[JokerSlot], start_idx: usize) -> Option<&JokerSlot> {
    for joker in jokers.iter().skip(start_idx + 1) {
        if joker.enabled && !is_copy_joker(joker.id) {
            return Some(joker);
        }
    }
    None
}

/// 找到 Brainstorm 複製的目標（最左邊第一個非複製類 Joker）
fn find_copy_target_leftmost(jokers: &[JokerSlot]) -> Option<&JokerSlot> {
    for joker in jokers.iter() {
        if joker.enabled && !is_copy_joker(joker.id) {
            return Some(joker);
        }
    }
    None
}

/// 判斷是否為複製類 Joker
fn is_copy_joker(id: JokerId) -> bool {
    matches!(id, JokerId::Blueprint | JokerId::Brainstorm)
}

// ============================================================================
// V2 效果計算（使用 joker_def 模板系統）
// ============================================================================

/// 從 ScoringContext 創建 ComputeContextV2
fn scoring_to_compute_context<'a>(ctx: &'a ScoringContext<'a>) -> ComputeContextV2<'a> {
    ComputeContextV2 {
        played_cards: ctx.played_cards,
        hand: ctx.hand,
        hand_id: ctx.hand_id,
        is_first_hand: ctx.is_first_hand,
        is_final_hand: ctx.is_final_hand,
        money_held: ctx.money_held,
        joker_count: ctx.joker_count,
        joker_slot_limit: ctx.joker_slot_limit,
        discards_remaining: ctx.discards_remaining,
        hands_played_this_round: ctx.hands_played_this_round,
        hands_played_this_run: ctx.hands_played_this_run,
        deck_size: ctx.deck_size,
        enhanced_cards_in_deck: ctx.enhanced_cards_in_deck,
        uncommon_joker_count: ctx.uncommon_joker_count,
    }
}

/// 將 JokerBonusNew 轉換為 JokerBonus
fn convert_bonus(new_bonus: &JokerBonusNew) -> JokerBonus {
    JokerBonus {
        chip_bonus: new_bonus.chip_bonus,
        add_mult: new_bonus.add_mult,
        mul_mult: new_bonus.mul_mult,
        money_bonus: new_bonus.money_bonus,
        retriggers: new_bonus.retriggers,
    }
}

/// 計算單個 Joker 效果（使用 V2 模板系統）
pub fn compute_joker_effect_with_state_v2(
    joker: &JokerSlot,
    ctx: &ScoringContext,
    rng_value: u8,
) -> JokerBonus {
    let compute_ctx = scoring_to_compute_context(ctx);
    let joker_index = joker.id as usize;
    let new_bonus = compute_joker_effect_v2(joker_index, &joker.state, &compute_ctx, rng_value);
    convert_bonus(&new_bonus)
}

/// 計算所有 Joker 的總加成（使用 V2 模板系統）
pub fn compute_joker_bonus_v2(
    jokers: &[JokerSlot],
    ctx: &ScoringContext,
    rng_values: &[u8],
) -> JokerBonus {
    let mut total = JokerBonus::new();

    // 收集所有 enabled 的 Joker 及其原始索引
    let enabled_jokers: Vec<(usize, &JokerSlot)> = jokers
        .iter()
        .enumerate()
        .filter(|(_, j)| j.enabled)
        .collect();

    for (idx_in_enabled, &(original_idx, joker)) in enabled_jokers.iter().enumerate() {
        let rng_val = rng_values.get(idx_in_enabled).copied().unwrap_or(0);

        // 檢查是否為複製類 Joker
        let effect = match joker.id {
            JokerId::Blueprint => {
                // Blueprint: 複製右邊第一個非複製類 Joker 的能力
                if let Some(target) = find_copy_target_right(jokers, original_idx) {
                    compute_joker_effect_with_state_v2(target, ctx, rng_val)
                } else {
                    JokerBonus::new()
                }
            }
            JokerId::Brainstorm => {
                // Brainstorm: 複製最左邊第一個非複製類 Joker 的能力
                if let Some(target) = find_copy_target_leftmost(jokers) {
                    compute_joker_effect_with_state_v2(target, ctx, rng_val)
                } else {
                    JokerBonus::new()
                }
            }
            _ => compute_joker_effect_with_state_v2(joker, ctx, rng_val),
        };
        total.merge(&effect);
    }

    total
}

/// 計算單個 Joker 效果（使用 JokerSlot 狀態）
///
/// 已遷移至 V2 模板系統，此函數現在直接調用 `compute_joker_effect_with_state_v2`。
#[inline]
pub fn compute_joker_effect_with_state(
    joker: &JokerSlot,
    ctx: &ScoringContext,
    rng_value: u8,
) -> JokerBonus {
    compute_joker_effect_with_state_v2(joker, ctx, rng_value)
}

// ============================================================================
// 事件觸發系統 - JokerSlot 高階包裝
// ============================================================================

/// 處理遊戲事件，更新所有 JokerSlot 狀態並收集結果
///
/// 這是 `trigger_joker_events` 的高階包裝，直接操作 JokerSlot 數組。
///
/// # 參數
/// - `event`: 發生的遊戲事件
/// - `jokers`: 所有 Joker 的槽位（可變引用）
/// - `ctx`: 觸發器上下文（包含額外信息）
///
/// # 返回
/// 觸發器執行結果（金幣變化、銷毀列表等）
pub fn trigger_joker_slot_events(
    event: GameEvent,
    jokers: &mut [JokerSlot],
    ctx: &TriggerContext,
) -> TriggerResult {
    // 收集啟用的 Joker 索引和 ID 索引
    let enabled_indices: Vec<(usize, usize)> = jokers
        .iter()
        .enumerate()
        .filter(|(_, j)| j.enabled)
        .map(|(slot_idx, j)| (slot_idx, j.id.to_index()))
        .collect();

    if enabled_indices.is_empty() {
        return TriggerResult::new();
    }

    // 提取狀態和 ID 索引
    let mut states: Vec<JokerState> = enabled_indices
        .iter()
        .map(|(slot_idx, _)| jokers[*slot_idx].state.clone())
        .collect();
    let joker_indices: Vec<usize> = enabled_indices
        .iter()
        .map(|(_, id_idx)| *id_idx)
        .collect();

    // 調用底層觸發函數
    let result = trigger_joker_events(event, &mut states, &joker_indices, ctx);

    // 將修改後的狀態寫回 JokerSlot
    for (i, (slot_idx, _)) in enabled_indices.iter().enumerate() {
        jokers[*slot_idx].state = states[i].clone();
    }

    // 處理銷毀的 Joker（映射回實際的 slot 索引）
    let mut actual_destroy_indices = Vec::new();
    for &state_idx in &result.jokers_to_destroy {
        if state_idx < enabled_indices.len() {
            actual_destroy_indices.push(enabled_indices[state_idx].0);
        }
    }

    // 返回帶有正確索引的結果
    TriggerResult {
        money_delta: result.money_delta,
        jokers_to_destroy: actual_destroy_indices,
        disable_boss_blind: result.disable_boss_blind,
        create_negative_copy: result.create_negative_copy,
        madness_destroys: result.madness_destroys,
        tarot_to_create: result.tarot_to_create,
        planet_to_create: result.planet_to_create,
        red_card_mult_increase: result.red_card_mult_increase,
    }
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
    fn test_joker_id_indices() {
        // 確保索引轉換正確
        for i in 0..50 {
            if let Some(id) = JokerId::from_index(i) {
                assert_eq!(id.to_index(), i);
            }
        }
    }

    #[test]
    fn test_basic_joker() {
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Joker, &ctx, 0);
        assert_eq!(bonus.add_mult, 4);
    }

    #[test]
    fn test_jolly_joker_with_pair() {
        let cards = make_cards(&[(5, 0), (5, 1)]);
        let ctx = ScoringContext::new(&cards, HandId::Pair);
        let bonus = compute_core_joker_effect(JokerId::JollyJoker, &ctx, 0);
        assert_eq!(bonus.add_mult, 8);
    }

    #[test]
    fn test_jolly_joker_without_pair() {
        let cards = make_cards(&[(5, 0), (6, 1)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::JollyJoker, &ctx, 0);
        assert_eq!(bonus.add_mult, 0);
    }

    #[test]
    fn test_half_joker() {
        let cards = make_cards(&[(5, 0), (6, 1), (7, 2)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::HalfJoker, &ctx, 0);
        assert_eq!(bonus.add_mult, 20);
    }

    #[test]
    fn test_banner_joker() {
        let cards = make_cards(&[(5, 0)]);
        let mut ctx = ScoringContext::new(&cards, HandId::HighCard);
        ctx.discards_remaining = 3;
        let bonus = compute_core_joker_effect(JokerId::Banner, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 90);
    }

    #[test]
    fn test_greedy_joker() {
        let cards = make_cards(&[(5, 1), (6, 1), (7, 1)]); // 3 diamonds
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::GreedyJoker, &ctx, 0);
        assert_eq!(bonus.money_bonus, 9);
    }

    #[test]
    fn test_abstract_joker() {
        let cards = make_cards(&[(5, 0)]);
        let mut ctx = ScoringContext::new(&cards, HandId::HighCard);
        ctx.joker_count = 4;
        let bonus = compute_core_joker_effect(JokerId::AbstractJoker, &ctx, 0);
        assert_eq!(bonus.add_mult, 12);
    }

    #[test]
    fn test_misprint_joker() {
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus1 = compute_core_joker_effect(JokerId::Misprint, &ctx, 0);
        assert_eq!(bonus1.add_mult, 0);
        let bonus2 = compute_core_joker_effect(JokerId::Misprint, &ctx, 23);
        assert_eq!(bonus2.add_mult, 23);
    }

    #[test]
    fn test_the_duo_mult() {
        let cards = make_cards(&[(5, 0), (5, 1)]);
        let ctx = ScoringContext::new(&cards, HandId::Pair);
        let bonus = compute_core_joker_effect(JokerId::The_Duo, &ctx, 0);
        assert_eq!(bonus.mul_mult, 2.0);
    }

    #[test]
    fn test_fibonacci() {
        // 1, 2, 3, 5, 8 cards
        let cards = make_cards(&[(1, 0), (2, 1), (3, 2), (5, 3), (8, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Fibonacci, &ctx, 0);
        assert_eq!(bonus.add_mult, 40); // 5 cards * 8
    }

    #[test]
    fn test_walkie_with_10() {
        let cards = make_cards(&[(10, 0), (5, 1)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Walkie, &ctx, 0);
        assert_eq!(bonus.add_mult, 10);
    }

    #[test]
    fn test_walkie_with_4() {
        let cards = make_cards(&[(4, 0), (7, 1)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Walkie, &ctx, 0);
        assert_eq!(bonus.add_mult, 10);
    }

    #[test]
    fn test_walkie_without_10_or_4() {
        let cards = make_cards(&[(5, 0), (6, 1), (7, 2)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Walkie, &ctx, 0);
        assert_eq!(bonus.add_mult, 0);
    }

    #[test]
    fn test_joker_all_available() {
        let all = JokerId::all_available();
        assert!(all.len() > 100); // Should have 100+ available jokers
    }

    #[test]
    fn test_joker_by_rarity() {
        let common = JokerId::by_rarity(1);
        let uncommon = JokerId::by_rarity(2);
        let rare = JokerId::by_rarity(3);
        let legendary = JokerId::by_rarity(4);

        assert!(!common.is_empty());
        assert!(!uncommon.is_empty());
        assert!(!rare.is_empty());
        assert!(!legendary.is_empty());
    }

    // ====== X Mult Jokers 單元測試 ======

    #[test]
    fn test_vampire_initial_mult() {
        let joker = JokerSlot::new(JokerId::Vampire);
        assert_eq!(joker.get_x_mult(), 1.0);
    }

    #[test]
    fn test_vampire_enhancement_absorption() {
        let mut joker = JokerSlot::new(JokerId::Vampire);
        joker.update_vampire_on_enhancement(3); // 吸收 3 個增強
        assert!((joker.get_x_mult() - 1.3).abs() < 0.001); // 1.0 + 0.3
    }

    #[test]
    fn test_canio_initial_mult() {
        let joker = JokerSlot::new(JokerId::Canio);
        assert_eq!(joker.get_x_mult(), 1.0);
    }

    #[test]
    fn test_canio_face_destroyed() {
        let mut joker = JokerSlot::new(JokerId::Canio);
        joker.update_canio_on_face_destroyed(2); // 銷毀 2 張人頭牌
        assert!((joker.get_x_mult() - 3.0).abs() < 0.001); // 1.0 + 2.0
    }

    #[test]
    fn test_lucky_cat_trigger() {
        let mut joker = JokerSlot::new(JokerId::Lucky_Cat);
        joker.update_lucky_cat_on_trigger(4); // 4 次 Lucky 觸發
        assert!((joker.get_x_mult() - 2.0).abs() < 0.001); // 1.0 + 1.0
    }

    #[test]
    fn test_hologram_cards_added() {
        let mut joker = JokerSlot::new(JokerId::Hologram);
        joker.update_hologram_on_card_added(4); // 加 4 張牌
        assert!((joker.get_x_mult() - 2.0).abs() < 0.001); // 1.0 + 1.0
    }

    #[test]
    fn test_constellation_planet_used() {
        let mut joker = JokerSlot::new(JokerId::Constellation);
        for _ in 0..5 {
            joker.update_constellation_on_planet_used();
        }
        assert!((joker.get_x_mult() - 1.5).abs() < 0.001); // 1.0 + 0.5
    }

    #[test]
    fn test_madness_initial_mult() {
        let joker = JokerSlot::new(JokerId::Madness);
        assert!((joker.get_x_mult() - 0.5).abs() < 0.001); // 起始 X0.5
    }

    #[test]
    fn test_madness_joker_destroyed() {
        let mut joker = JokerSlot::new(JokerId::Madness);
        joker.update_madness_on_joker_destroyed(2); // 銷毀 2 個 Joker
        assert!((joker.get_x_mult() - 1.5).abs() < 0.001); // 0.5 + 1.0
    }

    #[test]
    fn test_yorick_discards() {
        let mut joker = JokerSlot::new(JokerId::Yorick);
        assert_eq!(joker.get_x_mult(), 1.0);

        // 棄 22 張 (不觸發)
        joker.update_yorick_on_discard(22);
        assert_eq!(joker.get_x_mult(), 1.0);

        // 再棄 1 張 (觸發第一次)
        joker.update_yorick_on_discard(1);
        assert_eq!(joker.get_x_mult(), 2.0);

        // 棄 46 張 (觸發兩次)
        joker.update_yorick_on_discard(46);
        assert_eq!(joker.get_x_mult(), 4.0);
    }

    #[test]
    fn test_glass_joker_break() {
        let mut joker = JokerSlot::new(JokerId::GlassJoker);
        joker.update_glass_on_break(2); // 2 張 Glass 碎裂
        assert!((joker.get_x_mult() - 2.5).abs() < 0.001); // 1.0 + 1.5
    }

    #[test]
    fn test_compute_joker_effect_with_state_vampire() {
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);

        let mut joker = JokerSlot::new(JokerId::Vampire);
        joker.update_vampire_on_enhancement(5); // 1.5 X Mult

        let bonus = compute_joker_effect_with_state(&joker, &ctx, 0);
        assert!((bonus.mul_mult - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_compute_joker_effect_with_state_madness() {
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);

        let mut joker = JokerSlot::new(JokerId::Madness);
        // 起始 0.5, 銷毀 3 個 Joker 後: 0.5 + 1.5 = 2.0
        joker.update_madness_on_joker_destroyed(3);

        let bonus = compute_joker_effect_with_state(&joker, &ctx, 0);
        assert!((bonus.mul_mult - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_joker_effect_with_state_yorick() {
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);

        let mut joker = JokerSlot::new(JokerId::Yorick);
        joker.update_yorick_on_discard(69); // 觸發 3 次

        let bonus = compute_joker_effect_with_state(&joker, &ctx, 0);
        assert!((bonus.mul_mult - 4.0).abs() < 0.001); // 1.0 + 3.0
    }

    #[test]
    fn test_get_x_mult_non_xmult_joker() {
        let joker = JokerSlot::new(JokerId::Joker);
        assert_eq!(joker.get_x_mult(), 1.0); // 非 X Mult Joker 返回 1.0
    }

    // ====== 特殊效果類 Jokers 單元測試 ======

    #[test]
    fn test_gros_michel_adds_mult() {
        // Gros Michel: +15 Mult, 1/15 chance to self-destruct at round end
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Gros_Michel, &ctx, 0);
        assert_eq!(bonus.add_mult, 15);
        assert_eq!(bonus.mul_mult, 1.0); // No X Mult
        assert_eq!(bonus.money_bonus, 0);
    }

    #[test]
    fn test_cavendish_x3_mult() {
        // Cavendish: X3 Mult, 1/1000 chance to self-destruct per play
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Cavendish, &ctx, 0);
        assert_eq!(bonus.mul_mult, 3.0);
        assert_eq!(bonus.add_mult, 0);
    }

    #[test]
    fn test_matador_no_trigger() {
        // Matador: +$8 when Boss Blind ability triggers
        // When boss ability does NOT trigger, no money bonus
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Matador, &ctx, 0);
        assert_eq!(bonus.money_bonus, 0);
    }

    #[test]
    fn test_matador_with_trigger() {
        // Matador: +$8 when Boss Blind ability triggers
        let cards = make_cards(&[(5, 0)]);
        let mut ctx = ScoringContext::new(&cards, HandId::HighCard);
        ctx.boss_ability_triggered = true;
        let bonus = compute_core_joker_effect(JokerId::Matador, &ctx, 0);
        assert_eq!(bonus.money_bonus, 8);
    }

    #[test]
    fn test_mr_bones_no_scoring_bonus() {
        // MrBones: Prevents death if chips > 25% of requirement
        // This effect is handled at game level, no scoring bonus
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::MrBones, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 0);
        assert_eq!(bonus.add_mult, 0);
        assert_eq!(bonus.mul_mult, 1.0);
        assert_eq!(bonus.money_bonus, 0);
    }

    #[test]
    fn test_luchador_no_scoring_bonus() {
        // Luchador: Sell to disable current Boss Blind effect
        // Sell effect handled at game level, no scoring bonus
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Luchador, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 0);
        assert_eq!(bonus.add_mult, 0);
        assert_eq!(bonus.mul_mult, 1.0);
        assert_eq!(bonus.money_bonus, 0);
    }

    #[test]
    fn test_chicot_is_rule_modifier() {
        // Chicot: Disables Boss Blind effects (passive)
        // This is a rule modifier, no direct scoring bonus
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Chicot, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 0);
        assert_eq!(bonus.add_mult, 0);
        assert_eq!(bonus.mul_mult, 1.0);
    }

    #[test]
    fn test_ceremonial_no_scoring_bonus() {
        // Ceremonial: On select_blind, destroy rightmost Joker, gain 2x sell value as Mult
        // The destruction event is handled at game level
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Ceremonial, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 0);
        assert_eq!(bonus.add_mult, 0);
        assert_eq!(bonus.mul_mult, 1.0);
    }

    #[test]
    fn test_invisible_joker_no_scoring_bonus() {
        // InvisibleJoker: After 2 rounds, sell to duplicate adjacent Joker
        // Round tracking and duplication handled at game level
        let cards = make_cards(&[(5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::InvisibleJoker, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 0);
        assert_eq!(bonus.add_mult, 0);
        assert_eq!(bonus.mul_mult, 1.0);
        assert_eq!(bonus.money_bonus, 0);
    }

    #[test]
    fn test_special_joker_rarity() {
        // Verify these special jokers have correct rarity (based on actual rarity() fn)
        assert_eq!(JokerId::MrBones.rarity(), 3); // Rare
        assert_eq!(JokerId::Matador.rarity(), 1); // Common (default)
        assert_eq!(JokerId::Luchador.rarity(), 1); // Common (default)
        assert_eq!(JokerId::Chicot.rarity(), 4); // Legendary
        assert_eq!(JokerId::Ceremonial.rarity(), 1); // Common (default)
        assert_eq!(JokerId::InvisibleJoker.rarity(), 1); // Common (default)
        assert_eq!(JokerId::Cavendish.rarity(), 3); // Rare
        assert_eq!(JokerId::Gros_Michel.rarity(), 1); // Common
    }

    #[test]
    fn test_special_jokers_in_all_available() {
        // Verify all special effect jokers are available for selection
        let all = JokerId::all_available();
        assert!(all.contains(&JokerId::MrBones));
        assert!(all.contains(&JokerId::Matador));
        assert!(all.contains(&JokerId::Luchador));
        assert!(all.contains(&JokerId::Chicot));
        assert!(all.contains(&JokerId::Ceremonial));
        assert!(all.contains(&JokerId::InvisibleJoker));
        assert!(all.contains(&JokerId::Cavendish));
        assert!(all.contains(&JokerId::Gros_Michel));
    }

    // ====== 條件觸發類 Jokers 單元測試 ======

    #[test]
    fn test_even_steven_per_card() {
        // EvenSteven: +4 Mult per even card (2, 4, 6, 8, 10)
        // Cards: 2, 4, 6 (all even, rank <= 10)
        let cards = make_cards(&[(2, 0), (4, 0), (6, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::EvenSteven, &ctx, 0);
        assert_eq!(bonus.add_mult, 12); // 3 even cards × 4 mult = 12
    }

    #[test]
    fn test_even_steven_excludes_face_cards() {
        // EvenSteven should NOT count J(11), Q(12), K(13) even though Q(12) is "even"
        // Cards: Q(12), K(13), 4
        let cards = make_cards(&[(12, 0), (13, 0), (4, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::EvenSteven, &ctx, 0);
        assert_eq!(bonus.add_mult, 4); // Only 4 is even (rank <= 10), Q is excluded
    }

    #[test]
    fn test_odd_todd_per_card() {
        // OddTodd: +31 Chips per odd card (A=1, 3, 5, 7, 9)
        // Cards: A(1), 3, 5 (all odd, rank <= 9 or rank == 1)
        let cards = make_cards(&[(1, 0), (3, 0), (5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::OddTodd, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 93); // 3 odd cards × 31 chips = 93
    }

    #[test]
    fn test_odd_todd_excludes_face_cards() {
        // OddTodd should NOT count J(11), K(13) even though they are "odd"
        // Cards: J(11), K(13), 3
        let cards = make_cards(&[(11, 0), (13, 0), (3, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::OddTodd, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 31); // Only 3 is odd (rank <= 9), J and K are excluded
    }

    #[test]
    fn test_even_steven_x2_all_even() {
        // Even_Steven (#138): X2 Mult if ALL scoring cards are even (2, 4, 6, 8, 10)
        // Cards: 2, 4, 6 (all even)
        let cards = make_cards(&[(2, 0), (4, 0), (6, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Even_Steven, &ctx, 0);
        assert_eq!(bonus.mul_mult, 2.0);
    }

    #[test]
    fn test_even_steven_x2_not_all_even() {
        // Even_Steven (#138): NO X2 if any card is not even
        // Cards: 2, 4, 5 (5 is odd, so no X2)
        let cards = make_cards(&[(2, 0), (4, 0), (5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Even_Steven, &ctx, 0);
        assert_eq!(bonus.mul_mult, 1.0); // No X2 because 5 is odd
    }

    #[test]
    fn test_even_steven_x2_face_card_breaks() {
        // Even_Steven (#138): Face cards (J, Q, K) should break the "all even" condition
        // Cards: 2, 4, Q(12) - Q is rank > 10, so not considered even
        let cards = make_cards(&[(2, 0), (4, 0), (12, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Even_Steven, &ctx, 0);
        assert_eq!(bonus.mul_mult, 1.0); // No X2 because Q is not a valid even card
    }

    #[test]
    fn test_odd_todd_2_x2_all_odd() {
        // Odd_Todd_2 (#139): X2 Mult if ALL scoring cards are odd (A, 3, 5, 7, 9)
        // Cards: A(1), 3, 5 (all odd)
        let cards = make_cards(&[(1, 0), (3, 0), (5, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Odd_Todd_2, &ctx, 0);
        assert_eq!(bonus.mul_mult, 2.0);
    }

    #[test]
    fn test_odd_todd_2_x2_not_all_odd() {
        // Odd_Todd_2 (#139): NO X2 if any card is not odd
        // Cards: A(1), 3, 4 (4 is even, so no X2)
        let cards = make_cards(&[(1, 0), (3, 0), (4, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Odd_Todd_2, &ctx, 0);
        assert_eq!(bonus.mul_mult, 1.0); // No X2 because 4 is even
    }

    #[test]
    fn test_odd_todd_2_x2_face_card_breaks() {
        // Odd_Todd_2 (#139): Face cards (J, K) should break the "all odd" condition
        // Cards: A(1), 3, J(11) - J is rank > 9, so not considered odd
        let cards = make_cards(&[(1, 0), (3, 0), (11, 0)]);
        let ctx = ScoringContext::new(&cards, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Odd_Todd_2, &ctx, 0);
        assert_eq!(bonus.mul_mult, 1.0); // No X2 because J is not a valid odd card
    }

    #[test]
    fn test_baron_kings_in_hand() {
        // Baron: X1.5 Mult per King held in hand (not played)
        // Played: 5, Hand: K, K (2 Kings in hand)
        let played = make_cards(&[(5, 0)]);
        let hand = make_cards(&[(13, 0), (13, 1)]); // Two Kings
        let ctx = ScoringContext::with_hand(&played, &hand, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Baron, &ctx, 0);
        // 1.5^2 = 2.25
        assert!((bonus.mul_mult - 2.25).abs() < 0.001);
    }

    #[test]
    fn test_baron_no_kings() {
        // Baron: No bonus if no Kings in hand
        let played = make_cards(&[(5, 0)]);
        let hand = make_cards(&[(12, 0), (11, 0)]); // Q and J, no Kings
        let ctx = ScoringContext::with_hand(&played, &hand, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Baron, &ctx, 0);
        assert_eq!(bonus.mul_mult, 1.0); // No X Mult bonus
    }

    #[test]
    fn test_baron_kings_played_not_counted() {
        // Baron: Kings that are PLAYED should NOT count
        // Played: K, Hand: 5 (King is played, not in hand)
        let played = make_cards(&[(13, 0)]); // King played
        let hand = make_cards(&[(5, 0)]); // No Kings in hand
        let ctx = ScoringContext::with_hand(&played, &hand, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::Baron, &ctx, 0);
        assert_eq!(bonus.mul_mult, 1.0); // No bonus because King was played
    }

    #[test]
    fn test_shoot_the_moon_queens_in_hand() {
        // ShootTheMoon: +13 Mult per Queen held in hand (not played)
        // Played: 5, Hand: Q, Q (2 Queens in hand)
        let played = make_cards(&[(5, 0)]);
        let hand = make_cards(&[(12, 0), (12, 1)]); // Two Queens
        let ctx = ScoringContext::with_hand(&played, &hand, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::ShootTheMoon, &ctx, 0);
        assert_eq!(bonus.add_mult, 26); // 2 Queens × 13 Mult = 26
    }

    #[test]
    fn test_shoot_the_moon_no_queens() {
        // ShootTheMoon: No bonus if no Queens in hand
        let played = make_cards(&[(5, 0)]);
        let hand = make_cards(&[(13, 0), (11, 0)]); // K and J, no Queens
        let ctx = ScoringContext::with_hand(&played, &hand, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::ShootTheMoon, &ctx, 0);
        assert_eq!(bonus.add_mult, 0); // No Mult bonus
    }

    #[test]
    fn test_shoot_the_moon_queens_played_not_counted() {
        // ShootTheMoon: Queens that are PLAYED should NOT count
        // Played: Q, Hand: 5 (Queen is played, not in hand)
        let played = make_cards(&[(12, 0)]); // Queen played
        let hand = make_cards(&[(5, 0)]); // No Queens in hand
        let ctx = ScoringContext::with_hand(&played, &hand, HandId::HighCard);
        let bonus = compute_core_joker_effect(JokerId::ShootTheMoon, &ctx, 0);
        assert_eq!(bonus.add_mult, 0); // No bonus because Queen was played
    }

    #[test]
    fn test_stone_joker() {
        // Stone Joker: +25 Chips per Stone card in the full deck
        let cards = make_cards(&[(5, 0)]);
        let mut ctx = ScoringContext::new(&cards, HandId::HighCard);
        ctx.stone_cards_in_deck = 4; // 4 Stone cards in deck
        let bonus = compute_core_joker_effect(JokerId::Stone, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 100); // 4 × 25 = 100 chips
    }

    #[test]
    fn test_stone_joker_no_stone_cards() {
        // Stone Joker: No bonus if no Stone cards in deck
        let cards = make_cards(&[(5, 0)]);
        let mut ctx = ScoringContext::new(&cards, HandId::HighCard);
        ctx.stone_cards_in_deck = 0;
        let bonus = compute_core_joker_effect(JokerId::Stone, &ctx, 0);
        assert_eq!(bonus.chip_bonus, 0);
    }
}
