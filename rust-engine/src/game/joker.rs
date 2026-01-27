//! Joker 系統 - Tiered Architecture
//!
//! 使用分層架構實現 Joker 效果：
//! - Tier 1 (CoreJoker): 高頻 Joker，使用 enum 實現靜態分發
//! - Tier 2 (ConditionalJoker): 條件觸發類，使用 trait object
//! - Tier 3: 複雜/動態 Joker，未來擴展

use super::cards::Card;
use super::hand_types::HandId;

// ============================================================================
// Joker ID 系統 - 消除字串比對
// ============================================================================

/// Joker 總數
pub const JOKER_COUNT: usize = 150;

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
    RideTheBus = 20,     // +1 Mult per consecutive non-face hand
    SteelJoker = 21,     // X0.2 Mult per Steel card in full hand
    GlassJoker = 22,     // X0.75 Mult per Glass card broken
    Hologram = 23,       // X0.25 Mult per card added to deck
    FourFingers = 24,    // Straights/Flushes can be made with 4 cards
    Shortcut = 25,       // Straights can skip 1 rank
    Splash = 26,         // Every card counts in every hand type
    Photograph = 27,     // X2 Mult for first Face card played
    GreenJoker = 28,     // +1 Mult per hand played (resets per round)
    SuperPosition = 29,  // Creates X2 Mult when hand is both Straight & Flush
    DuskJoker = 30,      // X2 Mult on last hand of round
    Fibonacci = 31,      // Ace/2/3/5/8 played give +8 Mult
    ScaryFace = 32,      // Face cards give +30 Chips
    EvenSteven = 33,     // Even cards give +4 Mult
    OddTodd = 34,        // Odd cards give +31 Chips
    Scholar = 35,        // Aces give +20 Chips and +4 Mult
    BusinessCard = 36,   // Face cards have 1/2 chance +$2
    Supernova = 37,      // +Mult equal to times hand played this run
    Erosion = 38,        // +4 Mult for each card below 52 in deck
    ToTheMoon = 39,      // +$1 per $5 held (replaces interest)

    // ========================================================================
    // Tier 1: Economy Jokers (索引 40-59)
    // ========================================================================
    GoldenJoker = 40,    // +$4 at end of round
    Bull = 41,           // +2 Chips for each $1 held
    Egg = 42,            // +$3 sell value per round
    Cartomancer = 43,    // Creates Tarot on Blind skip
    Astronomer = 44,     // Creates Planet on Blind skip
    Rocket = 45,         // +$1 at end of round (scaling)
    FortuneTeller = 46,  // +1 Mult per Tarot used this run
    Faceless = 47,       // +$5 if 3+ Face cards discarded
    SpaceJoker = 48,     // 1/4 chance to upgrade hand level
    Vagabond = 49,       // Creates Tarot if hand played with <=4 cards
    Stuntman = 50,       // +250 Chips, -2 hand size
    Brainstorm = 51,     // Copies leftmost Joker ability
    Satellite = 52,      // +$1 per unique Planet used this run
    ShootTheMoon = 53,   // +13 Mult per Queen in hand
    Bloodstone = 54,     // 1/2 chance X1.5 Mult for Hearts scored
    Arrowhead = 55,      // Spades give +50 Chips
    Onyx = 56,           // Clubs give +80 Mult
    Opal = 57,           // Diamonds give +1.5 X Mult
    Drunkard = 58,       // +1 discard each round
    SteakJoker = 59,     // X2 Mult, loses $1 sell per round

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
    Cavendish = 100,     // X3 Mult, 1/1000 chance to self-destruct
    Card_Sharp = 101,    // X3 Mult if hand already played this round
    Delayed = 102,       // +$2 per round if no discards used
    Hack = 103,          // Retrigger 2/3/4/5 cards
    Pareidolia = 104,    // All cards count as Face cards
    Gros_Michel = 105,   // +15 Mult, 1/15 chance to self-destruct
    Even_Steven = 106,   // X2 Mult if hand only contains evens
    Odd_Todd_2 = 107,    // X2 Mult if hand only contains odds
    Juggler = 108,       // +1 hand size
    DriversLicense = 109, // X3 Mult if you have 16+ enhanced cards
    Hit_The_Road = 110,  // X0.5 Mult, gains X0.5 for each Jack discarded
    The_Duo = 111,       // X2 Mult if hand contains Pair
    The_Trio = 112,      // X3 Mult if hand contains Three of a Kind
    The_Family = 113,    // X4 Mult if hand contains Four of a Kind
    The_Order = 114,     // X3 Mult if hand contains Straight
    The_Tribe = 115,     // X2 Mult if hand contains Flush
    Stencil = 116,       // X1 Mult for each empty Joker slot
    Perkeo = 117,        // Creates Negative copy of 1 consumable at end of shop
    Flower_Pot = 118,    // X3 Mult if hand has Diamond/Club/Heart/Spade
    BluePrint = 119,     // Reserved
    Canio = 120,         // X1 Mult, gains X1 for every face card destroyed
    Triboulet = 121,     // Kings and Queens give X2 Mult
    Yorick = 122,        // X1 Mult, gains X1 for every 23 cards discarded
    Chicot = 123,        // Disables Boss Blind effects
    Perkeo_2 = 124,      // Reserved
    Seeing_Double = 125, // X2 Mult if hand has Club AND another suit
    Matador = 126,       // +$8 when Boss Blind ability triggers
    Stuntman_2 = 127,    // Reserved
    Stone = 128,         // +25 Chips for each Stone card in deck
    Lucky_Cat = 129,     // X0.25 Mult for each Lucky card triggered
    Obelisk = 130,       // X0.2 Mult per consecutive hand without most played type
    Runner = 131,        // +15 Chips if hand is Straight
    Courier = 132,       // +25 Chips per card below Ace in hand
    Cloud9 = 133,        // Reserved
    Spare_Trousers = 134, // +2 Mult if hand has Two Pair
    Ring_Master = 135,   // Jokers can appear multiple times
    Golden_Ticket = 136, // Gold cards give +$3 at round end
    Rough_Gem_2 = 137,   // Reserved
    Bootstraps = 138,    // +2 Mult for each $5 above $0
    Caino = 139,         // X1 Mult, gains X0.1 for every face card destroyed
    Flash = 140,         // +2 Mult per reroll used
    Trousers = 141,      // +4 Mult if hand has Two Pair
    Reserved_1 = 142,
    Reserved_2 = 143,
    Reserved_3 = 144,
    Reserved_4 = 145,
    Reserved_5 = 146,
    Reserved_6 = 147,
    Reserved_7 = 148,
    Reserved_8 = 149,
}

impl JokerId {
    /// 獲取 Joker 的基礎成本
    pub fn base_cost(&self) -> i64 {
        match self {
            // Common jokers (rarity 1): 2-5
            JokerId::Joker => 2,
            JokerId::GreedyJoker | JokerId::LustyJoker |
            JokerId::WrathfulJoker | JokerId::GluttonousJoker => 5,
            JokerId::JollyJoker | JokerId::SlyJoker => 4,
            JokerId::ZanyJoker | JokerId::WilyJoker => 5,
            JokerId::MadJoker | JokerId::CleverJoker => 5,
            JokerId::CrazyJoker | JokerId::DeviousJoker => 5,
            JokerId::DrollJoker | JokerId::CraftyJoker => 5,
            JokerId::HalfJoker => 5,
            JokerId::Banner => 5,
            JokerId::MysticSummit => 6,
            JokerId::Misprint => 4,
            JokerId::AbstractJoker => 6,

            // Uncommon jokers (rarity 2): 5-7
            JokerId::RideTheBus => 6,
            JokerId::GreenJoker => 5,
            JokerId::SteelJoker => 7,
            JokerId::GlassJoker => 6,
            JokerId::Hologram => 5,
            JokerId::BlueJoker => 5,
            JokerId::SuperPosition => 6,
            JokerId::DuskJoker => 6,
            JokerId::Fibonacci => 6,
            JokerId::ScaryFace => 5,
            JokerId::EvenSteven => 5,
            JokerId::OddTodd => 5,
            JokerId::Scholar => 5,
            JokerId::BusinessCard => 5,
            JokerId::Supernova => 6,
            JokerId::GoldenJoker => 6,
            JokerId::Egg => 4,
            JokerId::Bull => 6,
            JokerId::Rocket => 6,
            JokerId::FortuneTeller => 5,
            JokerId::SpaceJoker => 5,
            JokerId::Erosion => 6,
            JokerId::ToTheMoon => 6,

            // Rare jokers (rarity 3): 7-8
            JokerId::FourFingers => 7,
            JokerId::Shortcut => 6,
            JokerId::Splash => 7,
            JokerId::Photograph => 7,
            JokerId::Bloodstone => 7,
            JokerId::Arrowhead => 7,
            JokerId::Onyx => 8,
            JokerId::Opal => 8,
            JokerId::Blueprint => 10,
            JokerId::Brainstorm => 10,
            JokerId::Baron => 8,
            JokerId::Acrobat => 8,
            JokerId::SockAndBuskin => 8,
            JokerId::DNA => 8,
            JokerId::Mime => 8,
            JokerId::Vampire => 8,
            JokerId::Cavendish => 7,
            JokerId::Card_Sharp => 7,
            JokerId::The_Duo => 7,
            JokerId::The_Trio => 7,
            JokerId::The_Family => 8,
            JokerId::The_Order => 7,
            JokerId::The_Tribe => 7,

            // Default cost
            _ => 5,
        }
    }

    /// 獲取稀有度 (1=Common, 2=Uncommon, 3=Rare, 4=Legendary)
    pub fn rarity(&self) -> u8 {
        match self {
            // Common (1)
            JokerId::Joker | JokerId::Misprint |
            JokerId::GreedyJoker | JokerId::LustyJoker |
            JokerId::WrathfulJoker | JokerId::GluttonousJoker |
            JokerId::JollyJoker | JokerId::ZanyJoker |
            JokerId::MadJoker | JokerId::CrazyJoker |
            JokerId::DrollJoker | JokerId::SlyJoker |
            JokerId::WilyJoker | JokerId::CleverJoker |
            JokerId::DeviousJoker | JokerId::CraftyJoker |
            JokerId::HalfJoker | JokerId::Banner |
            JokerId::ScaryFace | JokerId::EvenSteven |
            JokerId::OddTodd | JokerId::Scholar |
            JokerId::Egg | JokerId::Drunkard |
            JokerId::GreenJoker | JokerId::Fibonacci => 1,

            // Uncommon (2)
            JokerId::MysticSummit | JokerId::AbstractJoker |
            JokerId::RideTheBus | JokerId::Hologram |
            JokerId::BlueJoker | JokerId::SuperPosition |
            JokerId::DuskJoker | JokerId::BusinessCard |
            JokerId::Supernova | JokerId::GoldenJoker |
            JokerId::Bull | JokerId::Rocket |
            JokerId::FortuneTeller | JokerId::SpaceJoker |
            JokerId::Erosion | JokerId::ToTheMoon |
            JokerId::IceCream | JokerId::Constellation |
            JokerId::Hiker | JokerId::CloudNine |
            JokerId::Popcorn | JokerId::AncientJoker |
            JokerId::Ramen | JokerId::Walkie |
            JokerId::Selzer | JokerId::Castle |
            JokerId::Smiley | JokerId::Campfire => 2,

            // Rare (3)
            JokerId::SteelJoker | JokerId::GlassJoker |
            JokerId::FourFingers | JokerId::Shortcut |
            JokerId::Splash | JokerId::Photograph |
            JokerId::Bloodstone | JokerId::Arrowhead |
            JokerId::Onyx | JokerId::Opal |
            JokerId::SteakJoker | JokerId::DNA |
            JokerId::Sixth | JokerId::Ticket |
            JokerId::MrBones | JokerId::Acrobat |
            JokerId::SockAndBuskin | JokerId::Swashbuckler |
            JokerId::Troubadour | JokerId::Certificate |
            JokerId::Smeared | JokerId::Throwback |
            JokerId::HangingChad | JokerId::RoughGem |
            JokerId::Mime | JokerId::CreditCard |
            JokerId::Blueprint | JokerId::Brainstorm |
            JokerId::Baron | JokerId::Vampire |
            JokerId::Cavendish | JokerId::Card_Sharp |
            JokerId::The_Duo | JokerId::The_Trio |
            JokerId::The_Family | JokerId::The_Order |
            JokerId::The_Tribe | JokerId::Stencil => 3,

            // Legendary (4)
            JokerId::Canio | JokerId::Triboulet |
            JokerId::Yorick | JokerId::Chicot |
            JokerId::Perkeo | JokerId::Caino => 4,

            // Default to common
            _ => 1,
        }
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
            .filter(|j| !matches!(j,
                JokerId::Reserved_1 | JokerId::Reserved_2 | JokerId::Reserved_3 |
                JokerId::Reserved_4 | JokerId::Reserved_5 | JokerId::Reserved_6 |
                JokerId::Reserved_7 | JokerId::Reserved_8 |
                JokerId::BluePrint | JokerId::Perkeo_2 | JokerId::Stuntman_2 |
                JokerId::Cloud9 | JokerId::Rough_Gem_2
            ))
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
}

impl<'a> ScoringContext<'a> {
    pub fn new(played_cards: &'a [Card], hand_id: HandId) -> Self {
        Self {
            played_cards,
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
        }
    }
}

// ============================================================================
// Tier 1: Core Joker 效果計算 (靜態分發，最快)
// ============================================================================

/// 計算單個 Core Joker 的效果
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
            if matches!(ctx.hand_id, HandId::Pair | HandId::TwoPair | HandId::FullHouse) {
                bonus.add_mult += 8;
            }
        }
        JokerId::SlyJoker => {
            if matches!(ctx.hand_id, HandId::Pair | HandId::TwoPair | HandId::FullHouse) {
                bonus.chip_bonus += 50;
            }
        }

        // ====== Three of a Kind 加成 ======
        JokerId::ZanyJoker => {
            if matches!(ctx.hand_id, HandId::ThreeKind | HandId::FullHouse | HandId::FourKind) {
                bonus.add_mult += 12;
            }
        }
        JokerId::WilyJoker => {
            if matches!(ctx.hand_id, HandId::ThreeKind | HandId::FullHouse | HandId::FourKind) {
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
            if matches!(ctx.hand_id, HandId::Straight | HandId::StraightFlush | HandId::RoyalFlush) {
                bonus.add_mult += 12;
            }
        }
        JokerId::DeviousJoker => {
            if matches!(ctx.hand_id, HandId::Straight | HandId::StraightFlush | HandId::RoyalFlush) {
                bonus.chip_bonus += 100;
            }
        }

        // ====== Flush 加成 ======
        JokerId::DrollJoker => {
            if matches!(ctx.hand_id, HandId::Flush | HandId::StraightFlush | HandId::RoyalFlush) {
                bonus.add_mult += 10;
            }
        }
        JokerId::CraftyJoker => {
            if matches!(ctx.hand_id, HandId::Flush | HandId::StraightFlush | HandId::RoyalFlush) {
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

        // ====== 卡牌計數類 ======
        JokerId::BlueJoker => {
            bonus.chip_bonus += ctx.deck_size as i64 * 2;
        }
        JokerId::Fibonacci => {
            let fib_cards = ctx.played_cards.iter()
                .filter(|c| matches!(c.rank, 1 | 2 | 3 | 5 | 8))
                .count();
            bonus.add_mult += fib_cards as i64 * 8;
        }
        JokerId::ScaryFace => {
            let face_count = ctx.played_cards.iter()
                .filter(|c| c.rank >= 11 && c.rank <= 13)
                .count();
            bonus.chip_bonus += face_count as i64 * 30;
        }
        JokerId::EvenSteven => {
            let even_count = ctx.played_cards.iter()
                .filter(|c| c.rank % 2 == 0)
                .count();
            bonus.add_mult += even_count as i64 * 4;
        }
        JokerId::OddTodd => {
            let odd_count = ctx.played_cards.iter()
                .filter(|c| c.rank % 2 == 1)
                .count();
            bonus.chip_bonus += odd_count as i64 * 31;
        }
        JokerId::Scholar => {
            let ace_count = ctx.played_cards.iter()
                .filter(|c| c.rank == 1 || c.rank == 14)
                .count();
            bonus.chip_bonus += ace_count as i64 * 20;
            bonus.add_mult += ace_count as i64 * 4;
        }
        JokerId::Smiley => {
            let face_count = ctx.played_cards.iter()
                .filter(|c| c.rank >= 11 && c.rank <= 13)
                .count();
            bonus.add_mult += face_count as i64 * 5;
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
        JokerId::Photograph => {
            if ctx.played_cards.iter().any(|c| c.rank >= 11 && c.rank <= 13) {
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

        // ====== The X 系列 ======
        JokerId::The_Duo => {
            if matches!(ctx.hand_id, HandId::Pair | HandId::TwoPair | HandId::FullHouse |
                       HandId::ThreeKind | HandId::FourKind | HandId::FiveKind) {
                bonus.mul_mult *= 2.0;
            }
        }
        JokerId::The_Trio => {
            if matches!(ctx.hand_id, HandId::ThreeKind | HandId::FullHouse |
                       HandId::FourKind | HandId::FiveKind) {
                bonus.mul_mult *= 3.0;
            }
        }
        JokerId::The_Family => {
            if matches!(ctx.hand_id, HandId::FourKind | HandId::FiveKind) {
                bonus.mul_mult *= 4.0;
            }
        }
        JokerId::The_Order => {
            if matches!(ctx.hand_id, HandId::Straight | HandId::StraightFlush | HandId::RoyalFlush) {
                bonus.mul_mult *= 3.0;
            }
        }
        JokerId::The_Tribe => {
            if matches!(ctx.hand_id, HandId::Flush | HandId::StraightFlush | HandId::RoyalFlush |
                       HandId::FlushHouse | HandId::FlushFive) {
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
            let face_count = ctx.played_cards.iter()
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
            let low_cards = ctx.played_cards.iter()
                .filter(|c| matches!(c.rank, 2 | 3 | 4 | 5))
                .count();
            bonus.retriggers += low_cards as i32;
        }

        // ====== 其他狀態類 ======
        JokerId::Throwback => {
            bonus.mul_mult *= 1.0 + (ctx.blinds_skipped as f32 * 0.25);
        }
        JokerId::RedCard => {
            bonus.add_mult += ctx.rerolls_this_run as i64 * 3;
        }
        JokerId::Flash => {
            bonus.add_mult += ctx.rerolls_this_run as i64 * 2;
        }
        JokerId::Baron => {
            // Kings in hand (not played) give X1.5 - handled separately
            // This is a simplification
            bonus.mul_mult *= 1.5;
        }
        JokerId::ShootTheMoon => {
            // Queens in hand give +13 mult
            bonus.add_mult += 13;
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
            if matches!(ctx.hand_id, HandId::Straight | HandId::StraightFlush | HandId::RoyalFlush) {
                bonus.chip_bonus += 15;
            }
        }
        JokerId::Bootstraps => {
            if ctx.money_held > 0 {
                bonus.add_mult += (ctx.money_held / 5) * 2;
            }
        }

        // 規則修改類（不直接加分，影響其他系統）
        JokerId::FourFingers | JokerId::Shortcut | JokerId::Splash |
        JokerId::Smeared | JokerId::Pareidolia | JokerId::Chicot => {
            // 這些會影響計分規則，不在這裡處理
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
    pub x_mult_accumulated: f32,
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
            x_mult_accumulated: 1.0,
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
}

/// 計算所有 Joker 的總加成
pub fn compute_joker_bonus(jokers: &[JokerSlot], ctx: &ScoringContext, rng_values: &[u8]) -> JokerBonus {
    let mut total = JokerBonus::new();

    for (i, joker) in jokers.iter().filter(|j| j.enabled).enumerate() {
        let rng_val = rng_values.get(i).copied().unwrap_or(0);
        let effect = compute_core_joker_effect(joker.id, ctx, rng_val);
        total.merge(&effect);
    }

    total
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
}
