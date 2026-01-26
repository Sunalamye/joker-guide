use std::fs;
use std::path::Path;
use std::sync::Mutex;

use rand::seq::SliceRandom;
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde_json::Value;
use tonic::{Request, Response, Status};

use joker_env::proto::joker_env_server::{JokerEnv, JokerEnvServer};
use joker_env::proto::{
    Action, EnvInfo, GetSpecRequest, GetSpecResponse, Observation, ResetRequest, ResetResponse,
    StepRequest, StepResponse, Tensor, TensorSpec,
};

// ============================================================================
// 常量定義
// ============================================================================

const HAND_SIZE: usize = 8; // 手牌數量（可選的牌）
const MAX_SELECTED: usize = 5; // 最多選擇 5 張打出
const JOKER_SLOTS: usize = 5;
const PLAYS_PER_BLIND: i32 = 4;
const DISCARDS_PER_BLIND: i32 = 3;
const STARTING_MONEY: i64 = 4;
const INTEREST_RATE: f32 = 0.1;
const MAX_INTEREST: i64 = 5;
const MONEY_PER_REMAINING_HAND: i64 = 1;
const SHOP_JOKER_COUNT: usize = 2;

// Observation 大小
const SCALAR_COUNT: usize = 13; // 擴展的標量特徵 (加入 boss_blind_id)
const SELECTION_FEATURES: usize = HAND_SIZE;
// Card features: 13 rank one-hot + 4 suit one-hot + 4 enhancement info
//   enhancement (normalized 0-8), seal (normalized 0-4), edition (normalized 0-4), face_down (0/1)
const CARD_BASE_FEATURES: usize = 17; // 13 rank one-hot + 4 suit one-hot
const CARD_ENHANCE_FEATURES: usize = 4; // enhancement, seal, edition, face_down
const CARD_FEATURES: usize = CARD_BASE_FEATURES + CARD_ENHANCE_FEATURES; // 21
const HAND_FEATURES: usize = HAND_SIZE * CARD_FEATURES;
const HAND_TYPE_COUNT: usize = 10;
const DECK_FEATURES: usize = 52;
const JOKER_FEATURES: usize = JOKER_SLOTS * 2;
const SHOP_FEATURES: usize = SHOP_JOKER_COUNT * 2; // shop joker id + cost
const BOSS_BLIND_FEATURES: usize = 1; // Boss Blind ID (正規化)
const OBS_SIZE: i32 = (SCALAR_COUNT
    + SELECTION_FEATURES
    + HAND_FEATURES
    + HAND_TYPE_COUNT
    + DECK_FEATURES
    + JOKER_FEATURES
    + SHOP_FEATURES) as i32;

// Action 類型
const ACTION_TYPE_SELECT: i32 = 0;
const ACTION_TYPE_PLAY: i32 = 1;
const ACTION_TYPE_DISCARD: i32 = 2;
const ACTION_TYPE_SELECT_BLIND: i32 = 3;
const ACTION_TYPE_CASH_OUT: i32 = 4;
const ACTION_TYPE_BUY_JOKER: i32 = 5;
const ACTION_TYPE_NEXT_ROUND: i32 = 6;

const ACTION_MASK_SIZE: i32 = 7 + (HAND_SIZE as i32 * 2) + 3 + SHOP_JOKER_COUNT as i32;
// 7 action types + card selection + blind selection (3) + shop jokers

const MAX_STEPS: i32 = 200; // 增加最大步數以支援完整遊戲

// ============================================================================
// 遊戲階段和 Blind 定義
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    PreBlind,
    Blind,
    PostBlind,
    Shop,
    End(GameEnd),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameEnd {
    Win,
    Lose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlindType {
    Small,
    Big,
    Boss,
}

impl BlindType {
    fn reward(&self) -> i64 {
        match self {
            BlindType::Small => 3,
            BlindType::Big => 4,
            BlindType::Boss => 5,
        }
    }

    fn score_multiplier(&self) -> f32 {
        match self {
            BlindType::Small => 1.0,
            BlindType::Big => 1.5,
            BlindType::Boss => 2.0,
        }
    }

    fn next(&self) -> Option<BlindType> {
        match self {
            BlindType::Small => Some(BlindType::Big),
            BlindType::Big => Some(BlindType::Boss),
            BlindType::Boss => None, // Boss 之後進入下一個 Ante
        }
    }

    fn to_int(&self) -> i32 {
        match self {
            BlindType::Small => 0,
            BlindType::Big => 1,
            BlindType::Boss => 2,
        }
    }
}

/// Boss Blind 類型 - 每個有獨特的 debuff 效果
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BossBlind {
    // 分數修改類
    TheHook,      // 每手開始時隨機棄 2 張
    TheWall,      // 需要 4x 分數 (而非 2x)
    TheWheel,     // 1/7 的牌面朝下
    TheArm,       // 降低出過的牌型等級
    TheFlint,     // 基礎 chips 和 mult 減半

    // 花色禁用類
    TheClub,      // 梅花牌不計分
    TheDiamond,   // 方塊牌不計分 (又名 The Window)
    TheHeart,     // 紅心牌不計分
    TheSpade,     // 黑桃牌不計分 (又名 The Goad - 只有 Spade 計分的反面)

    // 強制行為類
    ThePsychic,   // 必須出 5 張牌
    TheMouth,     // 只能出一種牌型
    TheEye,       // 不能重複出同一種牌型
    ThePlant,     // Face Card 不計分
    TheSerpent,   // 每次出牌後抽 3 棄 3

    // 經濟懲罰類
    TheOx,        // 出 #(當前ante) 牌型時失去 $1
    TheHouse,     // 第一手是面朝下的
    TheMark,      // 所有 Face Card 面朝下
    TheFish,      // 開始時面朝下的牌打亂順序

    // 特殊類
    TheManacle,   // 手牌上限 -1
    ThePillar,    // 已打過的牌不再計分
    TheNeedle,    // 只有 1 次出牌機會
    TheHead,      // 紅心牌只能在第一手出

    // Ante 8 專屬
    VioletVessel, // 需要 6x 分數
    Crimson,      // 每輪 hand 數 -1
    Cerulean,     // 強制在開始時使用消耗品
    Amber,        // 無法使用消耗品
    Verdant,      // 所有牌在回合開始時面朝下
}

impl BossBlind {
    /// Boss Blind 的分數倍數 (大部分是 2x)
    fn score_multiplier(&self) -> f32 {
        match self {
            BossBlind::TheWall => 4.0,
            BossBlind::VioletVessel => 6.0,
            _ => 2.0,
        }
    }

    /// 轉換為整數 ID (用於 observation)
    fn to_int(&self) -> i32 {
        match self {
            BossBlind::TheHook => 0,
            BossBlind::TheWall => 1,
            BossBlind::TheWheel => 2,
            BossBlind::TheArm => 3,
            BossBlind::TheFlint => 4,
            BossBlind::TheClub => 5,
            BossBlind::TheDiamond => 6,
            BossBlind::TheHeart => 7,
            BossBlind::TheSpade => 8,
            BossBlind::ThePsychic => 9,
            BossBlind::TheMouth => 10,
            BossBlind::TheEye => 11,
            BossBlind::ThePlant => 12,
            BossBlind::TheSerpent => 13,
            BossBlind::TheOx => 14,
            BossBlind::TheHouse => 15,
            BossBlind::TheMark => 16,
            BossBlind::TheFish => 17,
            BossBlind::TheManacle => 18,
            BossBlind::ThePillar => 19,
            BossBlind::TheNeedle => 20,
            BossBlind::TheHead => 21,
            BossBlind::VioletVessel => 22,
            BossBlind::Crimson => 23,
            BossBlind::Cerulean => 24,
            BossBlind::Amber => 25,
            BossBlind::Verdant => 26,
        }
    }

    /// 常規 Boss 列表 (Ante 1-7 使用)
    fn regular_bosses() -> &'static [BossBlind] {
        &[
            BossBlind::TheHook,
            BossBlind::TheWall,
            BossBlind::TheWheel,
            BossBlind::TheArm,
            BossBlind::TheFlint,
            BossBlind::TheClub,
            BossBlind::TheDiamond,
            BossBlind::TheHeart,
            BossBlind::TheSpade,
            BossBlind::ThePsychic,
            BossBlind::TheMouth,
            BossBlind::TheEye,
            BossBlind::ThePlant,
            BossBlind::TheSerpent,
            BossBlind::TheOx,
            BossBlind::TheHouse,
            BossBlind::TheMark,
            BossBlind::TheFish,
            BossBlind::TheManacle,
            BossBlind::ThePillar,
            BossBlind::TheNeedle,
            BossBlind::TheHead,
        ]
    }

    /// Ante 8 專屬 Boss 列表
    fn showdown_bosses() -> &'static [BossBlind] {
        &[
            BossBlind::VioletVessel,
            BossBlind::Crimson,
            BossBlind::Cerulean,
            BossBlind::Amber,
            BossBlind::Verdant,
        ]
    }

    /// 檢查是否禁用指定花色
    fn disables_suit(&self, suit: u8) -> bool {
        match (self, suit) {
            (BossBlind::TheClub, 0) => true,    // Clubs = 0
            (BossBlind::TheDiamond, 1) => true, // Diamonds = 1
            (BossBlind::TheHeart, 2) => true,   // Hearts = 2
            (BossBlind::TheSpade, 3) => true,   // Spades = 3
            _ => false,
        }
    }

    /// 檢查是否禁用 Face Card
    fn disables_face_cards(&self) -> bool {
        matches!(self, BossBlind::ThePlant)
    }

    /// 檢查是否需要剛好 5 張牌
    fn requires_five_cards(&self) -> bool {
        matches!(self, BossBlind::ThePsychic)
    }

    /// 獲取最大出牌次數 (某些 Boss 會限制)
    fn max_plays(&self) -> Option<i32> {
        match self {
            BossBlind::TheNeedle => Some(1),
            BossBlind::Crimson => Some(PLAYS_PER_BLIND - 1),
            _ => None,
        }
    }
}

const BOSS_BLIND_COUNT: usize = 27; // Boss Blind 種類數量

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ante {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
}

impl Ante {
    fn base_score(&self) -> i64 {
        // 正確的 Balatro 基礎分數 (White Stake)
        match self {
            Ante::One => 300,
            Ante::Two => 800,
            Ante::Three => 2_000,
            Ante::Four => 5_000,
            Ante::Five => 11_000,
            Ante::Six => 20_000,
            Ante::Seven => 35_000,
            Ante::Eight => 50_000,
        }
    }

    fn next(&self) -> Option<Ante> {
        match self {
            Ante::One => Some(Ante::Two),
            Ante::Two => Some(Ante::Three),
            Ante::Three => Some(Ante::Four),
            Ante::Four => Some(Ante::Five),
            Ante::Five => Some(Ante::Six),
            Ante::Six => Some(Ante::Seven),
            Ante::Seven => Some(Ante::Eight),
            Ante::Eight => None,
        }
    }

    fn to_int(&self) -> i32 {
        match self {
            Ante::One => 1,
            Ante::Two => 2,
            Ante::Three => 3,
            Ante::Four => 4,
            Ante::Five => 5,
            Ante::Six => 6,
            Ante::Seven => 7,
            Ante::Eight => 8,
        }
    }
}

// ============================================================================
// 卡牌和牌型定義
// ============================================================================

/// 卡片增強類型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Enhancement {
    #[default]
    None,
    Bonus,    // +30 chips
    Mult,     // +4 mult
    Wild,     // 可當任意花色
    Glass,    // x2 Mult，1/4 機率破碎
    Steel,    // x1.5 Mult（在手牌中時）
    Stone,    // +50 chips，不計花色/點數
    Gold,     // 回合結束時 +$3
    Lucky,    // 1/5 機率 +20 Mult，1/15 機率 +$20
}

impl Enhancement {
    /// 轉換為整數 ID (用於 observation)
    fn to_int(&self) -> u8 {
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
    fn all() -> &'static [Enhancement] {
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
enum Seal {
    #[default]
    None,
    Gold,    // 打出時 +$3
    Red,     // 觸發 2 次
    Blue,    // 最後一手牌創建 Planet 卡
    Purple,  // 棄掉時創建 Tarot 卡
}

impl Seal {
    /// 轉換為整數 ID (用於 observation)
    fn to_int(&self) -> u8 {
        match self {
            Seal::None => 0,
            Seal::Gold => 1,
            Seal::Red => 2,
            Seal::Blue => 3,
            Seal::Purple => 4,
        }
    }

    /// 所有封印類型（用於隨機選擇）
    fn all() -> &'static [Seal] {
        &[Seal::Gold, Seal::Red, Seal::Blue, Seal::Purple]
    }
}

/// 卡片版本類型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Edition {
    #[default]
    Base,
    Foil,        // +50 chips
    Holographic, // +10 mult
    Polychrome,  // x1.5 mult
    Negative,    // +1 Joker slot (特殊，通常用於 Joker)
}

impl Edition {
    /// 轉換為整數 ID (用於 observation)
    fn to_int(&self) -> u8 {
        match self {
            Edition::Base => 0,
            Edition::Foil => 1,
            Edition::Holographic => 2,
            Edition::Polychrome => 3,
            Edition::Negative => 4,
        }
    }

    /// 所有版本類型（用於隨機選擇，不含 Negative）
    fn all_common() -> &'static [Edition] {
        &[Edition::Foil, Edition::Holographic, Edition::Polychrome]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Card {
    rank: u8,              // 1..=13 (Ace = 1)
    suit: u8,              // 0..=3
    enhancement: Enhancement,
    seal: Seal,
    edition: Edition,
    face_down: bool,       // 是否面朝下（某些 Boss Blind 效果）
}

impl Card {
    fn new(rank: u8, suit: u8) -> Self {
        Self {
            rank,
            suit,
            enhancement: Enhancement::None,
            seal: Seal::None,
            edition: Edition::Base,
            face_down: false,
        }
    }

    /// 基礎 chips（不含增強效果）
    fn base_chips(&self) -> i64 {
        match self.rank {
            1 => 11,  // Ace
            11 | 12 | 13 => 10, // J, Q, K
            n => n as i64,
        }
    }

    /// 總 chips（含增強和版本效果）
    fn chips(&self) -> i64 {
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
        base + enhancement_bonus + edition_bonus
    }

    /// 加法 mult 加成
    fn add_mult(&self) -> i64 {
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
    fn x_mult(&self) -> f32 {
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
    fn is_face(&self) -> bool {
        self.rank >= 11 && self.rank <= 13
    }

    /// Wild 牌是否可匹配指定花色
    fn matches_suit(&self, target_suit: u8) -> bool {
        if self.enhancement == Enhancement::Wild {
            true // Wild 可匹配任意花色
        } else {
            self.suit == target_suit
        }
    }

    /// Stone 牌不參與牌型判定
    fn counts_for_hand(&self) -> bool {
        self.enhancement != Enhancement::Stone
    }

    /// 獲取有效花色（用於計分）
    fn effective_suit(&self) -> u8 {
        if self.enhancement == Enhancement::Stone {
            255 // 無效花色
        } else {
            self.suit
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandId {
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
}

struct HandScore {
    base_chips: i64,
    base_mult: i64,
    id: HandId,
}

// ============================================================================
// Joker 定義
// ============================================================================

#[derive(Clone)]
struct JokerSlot {
    id: i32,
    enabled: bool,
    type_key: String,
    cost: i64,
}

impl JokerSlot {
    fn new(id: i32, type_key: String, cost: i64) -> Self {
        Self {
            id,
            enabled: true,
            type_key,
            cost,
        }
    }
}

struct JokerBonus {
    chip_bonus: i64,
    add_mult: i64,
    mul_mult: f32,
}

// ============================================================================
// 商店定義
// ============================================================================

#[derive(Clone)]
struct Shop {
    jokers: Vec<JokerSlot>,
}

impl Shop {
    fn new() -> Self {
        Self { jokers: Vec::new() }
    }

    fn refresh(&mut self, rng: &mut StdRng, joker_types: &[String]) {
        self.jokers.clear();
        for i in 0..SHOP_JOKER_COUNT {
            let type_key = joker_types
                .choose(rng)
                .cloned()
                .unwrap_or_else(|| "+c".to_string());
            let cost = rng.gen_range(4..=8);
            self.jokers.push(JokerSlot::new(i as i32 + 1, type_key, cost));
        }
    }

    fn buy(&mut self, index: usize) -> Option<JokerSlot> {
        if index < self.jokers.len() {
            Some(self.jokers.remove(index))
        } else {
            None
        }
    }
}

// ============================================================================
// 遊戲狀態
// ============================================================================

struct EnvState {
    rng: StdRng,

    // 牌組
    deck: Vec<Card>,
    hand: Vec<Card>,
    discarded: Vec<Card>,
    selected_mask: u32,

    // Joker
    jokers: Vec<JokerSlot>,
    joker_slot_limit: usize,

    // 商店
    shop: Shop,

    // 遊戲進度
    stage: Stage,
    blind_type: Option<BlindType>,
    boss_blind: Option<BossBlind>, // 當前 Boss Blind 類型
    ante: Ante,
    round: i32,

    // 當前 Blind 狀態
    plays_left: i32,
    discards_left: i32,
    score: i64,

    // Boss Blind 追蹤
    played_hand_types: Vec<usize>, // 已出過的牌型 (用於 TheEye, TheMouth)
    first_hand_type: Option<usize>, // 第一手出的牌型 (用於 TheMouth)

    // 經濟
    money: i64,
    reward: i64,

    // 統計
    episode_step: i32,

    // 配置
    joker_type_keys: Vec<String>,
}

impl EnvState {
    fn new(seed: u64, joker_types: Vec<String>) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let deck = standard_deck();

        Self {
            rng,
            deck,
            hand: Vec::new(),
            discarded: Vec::new(),
            selected_mask: 0,
            jokers: Vec::new(),
            joker_slot_limit: JOKER_SLOTS,
            shop: Shop::new(),
            stage: Stage::PreBlind,
            blind_type: None,
            boss_blind: None,
            ante: Ante::One,
            round: 1,
            plays_left: PLAYS_PER_BLIND,
            discards_left: DISCARDS_PER_BLIND,
            score: 0,
            played_hand_types: Vec::new(),
            first_hand_type: None,
            money: STARTING_MONEY,
            reward: 0,
            episode_step: 0,
            joker_type_keys: joker_types,
        }
    }

    fn required_score(&self) -> i64 {
        let base = self.ante.base_score();
        // Boss Blind 使用自己的倍數，其他用 BlindType 的倍數
        let multiplier = if self.blind_type == Some(BlindType::Boss) {
            self.boss_blind.map(|b| b.score_multiplier()).unwrap_or(2.0)
        } else {
            self.blind_type.map(|b| b.score_multiplier()).unwrap_or(1.0)
        };
        (base as f32 * multiplier) as i64
    }

    fn deal(&mut self) {
        // 把手牌和弃牌堆放回牌組
        self.deck.append(&mut self.hand);
        self.deck.append(&mut self.discarded);
        self.deck.shuffle(&mut self.rng);

        // 發新手牌
        self.hand = self.deck.drain(0..HAND_SIZE.min(self.deck.len())).collect();
        self.selected_mask = 0;
    }

    fn calc_reward(&self) -> i64 {
        let blind = self.blind_type.unwrap_or(BlindType::Small);
        let base = blind.reward();
        let interest = ((self.money as f32 * INTEREST_RATE).floor() as i64).min(MAX_INTEREST);
        let hand_bonus = self.plays_left as i64 * MONEY_PER_REMAINING_HAND;
        base + interest + hand_bonus
    }

    fn discard_selected(&mut self, mask: u32) {
        let mut new_hand = Vec::new();
        for (idx, card) in self.hand.iter().enumerate() {
            if ((mask >> idx) & 1) == 1 {
                self.discarded.push(*card);
            } else {
                new_hand.push(*card);
            }
        }

        let draw_count = HAND_SIZE - new_hand.len();
        for _ in 0..draw_count {
            if let Some(card) = self.deck.pop() {
                new_hand.push(card);
            }
        }

        self.hand = new_hand;
    }

    fn refresh_shop(&mut self) {
        self.shop.refresh(&mut self.rng, &self.joker_type_keys);
    }

    /// TheHook: 隨機棄 2 張手牌
    fn apply_hook_discard(&mut self) {
        let discard_count = 2.min(self.hand.len());
        for _ in 0..discard_count {
            if self.hand.is_empty() { break; }
            let idx = self.rng.gen_range(0..self.hand.len());
            let card = self.hand.remove(idx);
            self.discarded.push(card);
        }
        // 補牌
        let draw_count = HAND_SIZE - self.hand.len();
        for _ in 0..draw_count {
            if let Some(card) = self.deck.pop() {
                self.hand.push(card);
            }
        }
    }

    /// TheSerpent: 抽 3 張，棄 3 張
    fn apply_serpent_effect(&mut self) {
        // 先抽 3 張
        for _ in 0..3 {
            if let Some(card) = self.deck.pop() {
                self.hand.push(card);
            }
        }
        // 再隨機棄 3 張
        let discard_count = 3.min(self.hand.len());
        for _ in 0..discard_count {
            if self.hand.is_empty() { break; }
            let idx = self.rng.gen_range(0..self.hand.len());
            let card = self.hand.remove(idx);
            self.discarded.push(card);
        }
    }

    /// 選擇隨機 Boss Blind
    fn select_random_boss(&mut self) {
        let bosses = if self.ante == Ante::Eight {
            BossBlind::showdown_bosses()
        } else {
            BossBlind::regular_bosses()
        };
        self.boss_blind = bosses.choose(&mut self.rng).copied();
    }

    /// 處理 Glass 牌破碎：從手牌和牌組中移除
    fn break_glass_cards(&mut self, selected_mask: u32, glass_indices: &[usize]) {
        if glass_indices.is_empty() {
            return;
        }

        // 找出被選中的牌的實際索引
        let mut selected_idx = 0;
        let mut to_remove = Vec::new();

        for (hand_idx, _) in self.hand.iter().enumerate() {
            if ((selected_mask >> hand_idx) & 1) == 1 {
                if glass_indices.contains(&selected_idx) {
                    to_remove.push(hand_idx);
                }
                selected_idx += 1;
            }
        }

        // 從高到低移除以避免索引偏移問題
        to_remove.sort_unstable_by(|a, b| b.cmp(a));
        for idx in to_remove {
            self.hand.remove(idx);
            // Glass 牌破碎後不會回到牌組，直接消失
        }
    }

    /// 處理棄牌（含 Purple Seal 效果）
    fn discard_with_seals(&mut self, mask: u32) -> i32 {
        let mut purple_count = 0;

        let mut new_hand = Vec::new();
        for (idx, card) in self.hand.iter().enumerate() {
            if ((mask >> idx) & 1) == 1 {
                // Purple Seal: 棄掉時創建 Tarot 卡（簡化：計數，之後實作消耗品系統再處理）
                if card.seal == Seal::Purple {
                    purple_count += 1;
                }
                self.discarded.push(*card);
            } else {
                new_hand.push(*card);
            }
        }

        let draw_count = HAND_SIZE - new_hand.len();
        for _ in 0..draw_count {
            if let Some(card) = self.deck.pop() {
                new_hand.push(card);
            }
        }

        self.hand = new_hand;
        purple_count
    }

    /// 計算 Steel 牌的 mult 加成（在手牌中時）
    fn steel_mult_bonus(&self) -> f32 {
        let mut x_mult = 1.0;
        for card in &self.hand {
            if card.enhancement == Enhancement::Steel {
                x_mult *= 1.5;
            }
        }
        x_mult
    }

    /// 計算 Gold 牌的回合結束金幣（回合結束時每張 +$3）
    fn gold_card_money(&self) -> i64 {
        self.hand
            .iter()
            .filter(|c| c.enhancement == Enhancement::Gold)
            .count() as i64 * 3
    }
}

// ============================================================================
// 輔助函數
// ============================================================================

fn standard_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);
    for suit in 0..4 {
        for rank in 1..=13 {
            deck.push(Card::new(rank, suit));
        }
    }
    deck
}

fn build_selected_hand(hand: &[Card], mask: u32) -> Vec<Card> {
    let mut selected = Vec::new();
    for (idx, card) in hand.iter().enumerate() {
        if ((mask >> idx) & 1) == 1 {
            selected.push(*card);
        }
    }
    // 確保至少有一張牌
    if selected.is_empty() && !hand.is_empty() {
        selected.push(hand[0]);
    }
    selected
}

fn compute_joker_bonus(jokers: &[JokerSlot]) -> JokerBonus {
    let mut bonus = JokerBonus {
        chip_bonus: 0,
        add_mult: 0,
        mul_mult: 1.0,
    };

    for joker in jokers.iter().filter(|j| j.enabled) {
        match joker.type_key.as_str() {
            "+c" => bonus.chip_bonus += 30,
            "!!" => bonus.chip_bonus += 80,
            "+$" => bonus.chip_bonus += 50,
            "+m" => bonus.add_mult += 4,
            "++" => bonus.add_mult += 8,
            "Xm" => bonus.mul_mult *= 1.5,
            "X2" => bonus.mul_mult *= 2.0,
            "..." => bonus.chip_bonus += 20,
            _ => bonus.chip_bonus += 10,
        }
    }

    bonus
}

fn score_hand(hand: &[Card]) -> HandScore {
    if hand.is_empty() {
        return HandScore {
            base_chips: 5,
            base_mult: 1,
            id: HandId::HighCard,
        };
    }

    // 過濾 Stone 牌（不參與牌型判定）和面朝下的牌
    let scoring_cards: Vec<&Card> = hand
        .iter()
        .take(MAX_SELECTED)
        .filter(|c| c.counts_for_hand() && !c.face_down)
        .collect();

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
    // 找出最多的花色並加上 wild 牌
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
    } else if count_values.get(0) == Some(&4) {
        HandId::FourKind
    } else if count_values.get(0) == Some(&3) && count_values.get(1) == Some(&2) {
        HandId::FullHouse
    } else if is_flush {
        HandId::Flush
    } else if is_straight {
        HandId::Straight
    } else if count_values.get(0) == Some(&3) {
        HandId::ThreeKind
    } else if count_values.get(0) == Some(&2) && count_values.get(1) == Some(&2) {
        HandId::TwoPair
    } else if count_values.get(0) == Some(&2) {
        HandId::Pair
    } else {
        HandId::HighCard
    };

    let (base_chips, base_mult) = match id {
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
    };

    HandScore {
        base_chips,
        base_mult,
        id,
    }
}

fn check_straight(rank_counts: &[u8; 13]) -> bool {
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

    // 檢查 A-2-3-4-5
    if rank_counts[0] > 0
        && rank_counts[1] > 0
        && rank_counts[2] > 0
        && rank_counts[3] > 0
        && rank_counts[4] > 0
    {
        return true;
    }

    // 檢查 10-J-Q-K-A
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

fn is_royal(rank_counts: &[u8; 13]) -> bool {
    rank_counts[0] > 0  // A
        && rank_counts[9] > 0   // 10
        && rank_counts[10] > 0  // J
        && rank_counts[11] > 0  // Q
        && rank_counts[12] > 0 // K
}

fn hand_type_index(hand_id: HandId) -> usize {
    match hand_id {
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
    }
}

fn card_index(card: Card) -> usize {
    (card.suit as usize * 13) + (card.rank as usize - 1)
}

fn hand_potential(hand: &[Card]) -> f32 {
    let score = score_hand(hand);
    let raw_score = score.base_chips * score.base_mult;
    (raw_score as f32 / 200.0).min(1.0)
}


/// 卡片計分結果（用於追蹤金幣和破碎效果）
struct CardScoreResult {
    score: i64,
    hand_id: HandId,
    money_gained: i64,        // 從 Gold Seal / Lucky 獲得的金幣
    glass_to_break: Vec<usize>, // 需要破碎的 Glass 牌索引
}

/// 計算出牌分數（考慮 Boss Blind debuff 和卡片增強）
fn calculate_play_score(
    selected: &[Card],
    jokers: &[JokerSlot],
    boss_blind: Option<BossBlind>,
    rng: &mut StdRng,
) -> CardScoreResult {
    let hand_score = score_hand(selected);
    let bonus = compute_joker_bonus(jokers);

    let mut total_chips = hand_score.base_chips + bonus.chip_bonus;
    let mut total_mult = hand_score.base_mult + bonus.add_mult;
    let mut x_mult = bonus.mul_mult;
    let mut money_gained: i64 = 0;
    let mut glass_to_break = Vec::new();

    // 計算每張牌的貢獻（考慮增強、封印、版本效果）
    for (idx, card) in selected.iter().enumerate() {
        // 面朝下的牌不計分
        if card.face_down {
            continue;
        }

        // 檢查花色是否被 Boss 禁用
        let suit_disabled = boss_blind
            .map(|b| b.disables_suit(card.suit))
            .unwrap_or(false);

        // Wild 牌不受花色禁用影響
        let effectively_disabled = suit_disabled && card.enhancement != Enhancement::Wild;

        // 檢查 Face Card 是否被禁用
        let is_face = card.is_face();
        let face_disabled = boss_blind
            .map(|b| b.disables_face_cards() && is_face)
            .unwrap_or(false);

        if effectively_disabled || face_disabled {
            continue;
        }

        // Red Seal: 效果觸發兩次
        let trigger_count = if card.seal == Seal::Red { 2 } else { 1 };

        for _ in 0..trigger_count {
            // 加上卡片的 chips（含增強和版本加成）
            total_chips += card.chips();

            // 加上卡片的 add mult
            total_mult += card.add_mult();

            // 乘上卡片的 x mult
            x_mult *= card.x_mult();

            // Lucky 牌特殊效果
            if card.enhancement == Enhancement::Lucky {
                // 1/5 機率 +20 Mult
                if rng.gen_range(0..5) == 0 {
                    total_mult += 20;
                }
                // 1/15 機率 +$20
                if rng.gen_range(0..15) == 0 {
                    money_gained += 20;
                }
            }

            // Glass 牌：1/4 機率破碎
            if card.enhancement == Enhancement::Glass {
                if rng.gen_range(0..4) == 0 {
                    glass_to_break.push(idx);
                }
            }
        }

        // Gold Seal: 打出時 +$3（不受 Red Seal 影響）
        if card.seal == Seal::Gold {
            money_gained += 3;
        }
    }

    // TheFlint: 基礎 chips 和 mult 減半
    if boss_blind == Some(BossBlind::TheFlint) {
        total_chips = (total_chips + 1) / 2;
        total_mult = (total_mult + 1) / 2;
    }

    let final_mult = ((total_mult as f32) * x_mult).max(1.0) as i64;
    let score = total_chips * final_mult;

    CardScoreResult {
        score,
        hand_id: hand_score.id,
        money_gained,
        glass_to_break,
    }
}

/// 簡化版計分（不含隨機效果，用於 combo_score 等）
fn calculate_play_score_simple(
    selected: &[Card],
    jokers: &[JokerSlot],
    boss_blind: Option<BossBlind>,
) -> (i64, HandId) {
    let hand_score = score_hand(selected);
    let bonus = compute_joker_bonus(jokers);

    let mut total_chips = hand_score.base_chips + bonus.chip_bonus;
    let mut total_mult = hand_score.base_mult + bonus.add_mult;
    let mut x_mult = bonus.mul_mult;

    for card in selected {
        if card.face_down {
            continue;
        }

        let suit_disabled = boss_blind
            .map(|b| b.disables_suit(card.suit))
            .unwrap_or(false);
        let effectively_disabled = suit_disabled && card.enhancement != Enhancement::Wild;
        let is_face = card.is_face();
        let face_disabled = boss_blind
            .map(|b| b.disables_face_cards() && is_face)
            .unwrap_or(false);

        if effectively_disabled || face_disabled {
            continue;
        }

        let trigger_count = if card.seal == Seal::Red { 2 } else { 1 };
        for _ in 0..trigger_count {
            total_chips += card.chips();
            total_mult += card.add_mult();
            x_mult *= card.x_mult();
        }
    }

    if boss_blind == Some(BossBlind::TheFlint) {
        total_chips = (total_chips + 1) / 2;
        total_mult = (total_mult + 1) / 2;
    }

    let final_mult = ((total_mult as f32) * x_mult).max(1.0) as i64;
    let score = total_chips * final_mult;

    (score, hand_score.id)
}

// ============================================================================
// Observation 構建
// ============================================================================

fn observation_from_state(state: &EnvState) -> Tensor {
    let mut data = Vec::with_capacity(OBS_SIZE as usize);

    let required = state.required_score().max(1) as f32;

    // Scalars (12)
    data.push(state.score as f32 / required); // 當前分數/目標
    data.push(state.ante.to_int() as f32 / 8.0); // Ante 進度
    data.push(state.blind_type.map(|b| b.to_int()).unwrap_or(-1) as f32 / 2.0); // Blind 類型
    data.push(match state.stage {
        Stage::PreBlind => 0.0,
        Stage::Blind => 1.0,
        Stage::PostBlind => 2.0,
        Stage::Shop => 3.0,
        Stage::End(_) => 4.0,
    } / 4.0); // Stage
    data.push(state.plays_left as f32 / PLAYS_PER_BLIND as f32);
    data.push(state.discards_left as f32 / DISCARDS_PER_BLIND as f32);
    data.push(state.money as f32 / 50.0); // 正規化金幣
    data.push(state.reward as f32 / 20.0); // 正規化獎勵
    data.push(state.deck.len() as f32 / 52.0);
    data.push(state.jokers.len() as f32 / state.joker_slot_limit as f32);
    data.push(state.round as f32 / 24.0); // 最多 8 ante * 3 blind = 24 輪
    data.push(state.episode_step as f32 / MAX_STEPS as f32);
    // Boss Blind ID (正規化到 0~1，-1 表示非 Boss)
    data.push(state.boss_blind.map(|b| b.to_int() as f32 / BOSS_BLIND_COUNT as f32).unwrap_or(-0.1));

    // Selection mask (8)
    for idx in 0..HAND_SIZE {
        let selected = ((state.selected_mask >> idx) & 1) == 1;
        data.push(if selected { 1.0 } else { 0.0 });
    }

    // Hand features (8 * 21 = 168)
    for idx in 0..HAND_SIZE {
        if let Some(card) = state.hand.get(idx) {
            // Rank one-hot (13)
            for r in 0..13 {
                data.push(if r == (card.rank - 1) as usize { 1.0 } else { 0.0 });
            }
            // Suit one-hot (4)
            for s in 0..4 {
                data.push(if s == card.suit as usize { 1.0 } else { 0.0 });
            }
            // Enhancement (normalized 0-8)
            data.push(card.enhancement.to_int() as f32 / 8.0);
            // Seal (normalized 0-4)
            data.push(card.seal.to_int() as f32 / 4.0);
            // Edition (normalized 0-4)
            data.push(card.edition.to_int() as f32 / 4.0);
            // Face down (0 or 1)
            data.push(if card.face_down { 1.0 } else { 0.0 });
        } else {
            data.extend(std::iter::repeat(0.0).take(CARD_FEATURES));
        }
    }

    // Hand type one-hot (10)
    let selected_hand = build_selected_hand(&state.hand, state.selected_mask);
    let hand_id = if selected_hand.is_empty() {
        HandId::HighCard
    } else {
        score_hand(&selected_hand).id
    };
    let ht_index = hand_type_index(hand_id);
    for idx in 0..HAND_TYPE_COUNT {
        data.push(if idx == ht_index { 1.0 } else { 0.0 });
    }

    // Deck counts (52)
    let mut deck_counts = [0.0f32; DECK_FEATURES];
    for card in &state.deck {
        let index = card_index(*card);
        deck_counts[index] += 1.0;
    }
    data.extend(deck_counts);

    // Joker features (5 * 2 = 10)
    for slot in 0..JOKER_SLOTS {
        if let Some(joker) = state.jokers.get(slot) {
            data.push(joker.id as f32);
            data.push(if joker.enabled { 1.0 } else { 0.0 });
        } else {
            data.push(0.0);
            data.push(0.0);
        }
    }

    // Shop features (2 * 2 = 4)
    for slot in 0..SHOP_JOKER_COUNT {
        if let Some(joker) = state.shop.jokers.get(slot) {
            data.push(joker.id as f32);
            data.push(joker.cost as f32 / 10.0);
        } else {
            data.push(0.0);
            data.push(0.0);
        }
    }

    // Padding if needed
    while data.len() < OBS_SIZE as usize {
        data.push(0.0);
    }

    Tensor {
        data,
        shape: vec![OBS_SIZE],
    }
}

// ============================================================================
// Action Mask 構建
// ============================================================================

fn action_mask_from_state(state: &EnvState, done: bool) -> Tensor {
    let mut data = vec![0.0; ACTION_MASK_SIZE as usize];

    if done {
        return Tensor {
            data,
            shape: vec![ACTION_MASK_SIZE],
        };
    }

    let mut offset = 0;

    // Action types (7)
    let in_blind = state.stage == Stage::Blind;
    let in_pre_blind = state.stage == Stage::PreBlind;
    let in_post_blind = state.stage == Stage::PostBlind;
    let in_shop = state.stage == Stage::Shop;

    data[0] = if in_blind { 1.0 } else { 0.0 }; // SELECT
    data[1] = if in_blind && state.plays_left > 0 { 1.0 } else { 0.0 }; // PLAY
    data[2] = if in_blind && state.discards_left > 0 { 1.0 } else { 0.0 }; // DISCARD
    data[3] = if in_pre_blind { 1.0 } else { 0.0 }; // SELECT_BLIND
    data[4] = if in_post_blind { 1.0 } else { 0.0 }; // CASH_OUT
    data[5] = if in_shop { 1.0 } else { 0.0 }; // BUY_JOKER
    data[6] = if in_shop { 1.0 } else { 0.0 }; // NEXT_ROUND
    offset += 7;

    // Card selection (8 * 2 = 16)
    let can_select = in_blind;
    for _ in 0..HAND_SIZE {
        data[offset] = if can_select { 1.0 } else { 0.0 }; // 不選
        data[offset + 1] = if can_select { 1.0 } else { 0.0 }; // 選
        offset += 2;
    }

    // Blind selection (3)
    data[offset] = if in_pre_blind { 1.0 } else { 0.0 }; // Small
    data[offset + 1] = if in_pre_blind && state.blind_type == Some(BlindType::Small) {
        1.0
    } else {
        0.0
    }; // Big
    data[offset + 2] = if in_pre_blind && state.blind_type == Some(BlindType::Big) {
        1.0
    } else {
        0.0
    }; // Boss
    offset += 3;

    // Shop joker purchase (2)
    for i in 0..SHOP_JOKER_COUNT {
        let can_buy = in_shop
            && state.shop.jokers.get(i).map(|j| j.cost <= state.money).unwrap_or(false)
            && state.jokers.len() < state.joker_slot_limit;
        data[offset + i] = if can_buy { 1.0 } else { 0.0 };
    }

    Tensor {
        data,
        shape: vec![ACTION_MASK_SIZE],
    }
}

// ============================================================================
// 獎勵函數（正規化到相近尺度）
// ============================================================================

/// 計算 Joker 組合的戰力分數（模擬實際得分公式）
fn combo_score(jokers: &[JokerSlot]) -> f32 {
    if jokers.is_empty() {
        return 0.0;
    }

    let mut chip_power = 0.0;
    let mut mult_power = 0.0;
    let mut x_mult_power = 1.0;

    for j in jokers.iter().filter(|j| j.enabled) {
        match j.type_key.as_str() {
            "+c" => chip_power += 30.0,
            "!!" => chip_power += 80.0,
            "+$" => chip_power += 50.0,
            "+m" => mult_power += 4.0,
            "++" => mult_power += 8.0,
            "Xm" => x_mult_power *= 1.5,
            "X2" => x_mult_power *= 2.0,
            "..." => chip_power += 20.0,
            _ => chip_power += 10.0,
        }
    }

    // 模擬實際得分公式：(base_chips + chip_bonus) * (base_mult + mult_bonus) * x_mult
    // 假設基礎牌型給 50 chips, 3 mult
    let simulated: f32 = (50.0 + chip_power) * (3.0 + mult_power) * x_mult_power;

    // 用 log2 壓縮範圍，避免極端值
    simulated.max(1.0).log2()
}

/// 買 Joker 獎勵：基於組合分數變化（方案 B）
fn joker_buy_reward(old_jokers: &[JokerSlot], new_jokers: &[JokerSlot], cost: i64, money_before: i64) -> f32 {
    let score_delta = combo_score(new_jokers) - combo_score(old_jokers);

    // 考慮經濟成本：花費佔總資金的比例
    let cost_ratio = if money_before > 0 {
        (cost as f32 / money_before as f32).min(1.0)
    } else {
        1.0
    };

    // 獎勵 = 分數提升 - 經濟成本懲罰
    // 分數提升大且花費佔比小 = 好的購買
    let reward = score_delta * 0.3 - cost_ratio * 0.1;
    reward.clamp(-0.3, 0.5)
}

/// 出牌獎勵：正規化到 0~0.3
fn play_reward(score_gained: i64, required: i64) -> f32 {
    if required <= 0 || score_gained <= 0 {
        return 0.0;
    }
    let ratio = (score_gained as f32 / required as f32).min(1.0);
    ratio * 0.3
}

/// 過關獎勵：正規化到 0.3~1.0
fn blind_clear_reward(plays_left: i32, blind_type: BlindType) -> f32 {
    let base = match blind_type {
        BlindType::Small => 0.3,
        BlindType::Big => 0.5,
        BlindType::Boss => 0.7,
    };
    // 效率獎勵：剩餘出牌次數越多越好
    let efficiency = plays_left as f32 * 0.05;
    (base + efficiency).min(1.0)
}

/// Ante 進度獎勵：非線性，中後期更有價值
fn ante_progress_reward(old_ante: Ante, new_ante: Ante) -> f32 {
    let ante_value = |a: Ante| -> f32 {
        match a {
            Ante::One => 0.0,
            Ante::Two => 0.05,
            Ante::Three => 0.1,
            Ante::Four => 0.15,
            Ante::Five => 0.25,   // 關鍵轉型期
            Ante::Six => 0.4,
            Ante::Seven => 0.6,
            Ante::Eight => 1.0,   // 勝利在望
        }
    };
    ante_value(new_ante) - ante_value(old_ante)
}

/// 遊戲結束獎勵：正規化到 -0.5~1.0
fn game_end_reward(end: GameEnd, ante: Ante) -> f32 {
    match end {
        GameEnd::Win => 1.0,
        GameEnd::Lose => {
            // 越早失敗懲罰越重
            let progress = ante.to_int() as f32 / 8.0;
            -0.5 * (1.0 - progress) // Ante 1: -0.44, Ante 8: 0
        }
    }
}

/// 金幣獎勵：考慮遊戲階段
fn money_reward(money: i64, ante: Ante) -> f32 {
    // 利息潛力：維持 $25+ 可獲最大利息
    let interest_potential = (money as f32 / 25.0).min(1.0);

    // 階段權重：早期存錢更重要
    let stage_weight = match ante {
        Ante::One | Ante::Two => 1.5,
        Ante::Three | Ante::Four => 1.0,
        Ante::Five | Ante::Six => 0.7,
        Ante::Seven | Ante::Eight => 0.3, // 後期錢不重要了
    };

    interest_potential * 0.15 * stage_weight
}

// ============================================================================
// gRPC 服務
// ============================================================================

struct EnvService {
    state: Mutex<EnvState>,
    joker_type_keys: Vec<String>,
}

impl Default for EnvService {
    fn default() -> Self {
        let joker_type_keys = load_joker_type_keys();
        Self {
            state: Mutex::new(EnvState::new(0, joker_type_keys.clone())),
            joker_type_keys,
        }
    }
}

#[tonic::async_trait]
impl JokerEnv for EnvService {
    async fn reset(
        &self,
        request: Request<ResetRequest>,
    ) -> Result<Response<ResetResponse>, Status> {
        let seed = request.into_inner().seed;
        let mut state = self
            .state
            .lock()
            .map_err(|_| Status::internal("lock error"))?;

        *state = EnvState::new(seed, self.joker_type_keys.clone());

        let observation = Observation {
            features: Some(observation_from_state(&state)),
            action_mask: Some(action_mask_from_state(&state, false)),
        };

        let info = EnvInfo {
            episode_step: state.episode_step,
            chips: state.score,
            mult: 1,
            blind_target: state.required_score(),
        };

        Ok(Response::new(ResetResponse {
            observation: Some(observation),
            info: Some(info),
        }))
    }

    async fn step(&self, request: Request<StepRequest>) -> Result<Response<StepResponse>, Status> {
        let StepRequest { action } = request.into_inner();
        let action = action.unwrap_or(Action {
            action_id: 0,
            params: vec![],
            action_type: ACTION_TYPE_SELECT,
        });

        let mut state = self
            .state
            .lock()
            .map_err(|_| Status::internal("lock error"))?;

        let action_type = action.action_type;
        let action_id = action.action_id as u32;

        let mut reward = 0.0;
        let mut done = false;

        match state.stage {
            Stage::PreBlind => {
                if action_type == ACTION_TYPE_SELECT_BLIND {
                    // 選擇 Blind - 使用 next() 方法進入下一個 Blind
                    let next_blind = state
                        .blind_type
                        .and_then(|b| b.next())
                        .unwrap_or(BlindType::Small);
                    state.blind_type = Some(next_blind);
                    state.stage = Stage::Blind;

                    // 如果是 Boss Blind，隨機選擇 Boss 類型
                    if next_blind == BlindType::Boss {
                        state.select_random_boss();
                        // 某些 Boss 會修改出牌次數
                        state.plays_left = state.boss_blind
                            .and_then(|b| b.max_plays())
                            .unwrap_or(PLAYS_PER_BLIND);
                    } else {
                        state.boss_blind = None;
                        state.plays_left = PLAYS_PER_BLIND;
                    }

                    state.discards_left = DISCARDS_PER_BLIND;
                    state.score = 0;
                    state.played_hand_types.clear();
                    state.first_hand_type = None;
                    state.deal();

                    // TheHook: 每手開始時隨機棄 2 張
                    if state.boss_blind == Some(BossBlind::TheHook) {
                        state.apply_hook_discard();
                    }
                }
            }

            Stage::Blind => {
                match action_type {
                    ACTION_TYPE_SELECT => {
                        // 選擇卡片
                        let mask = action_id & ((1 << HAND_SIZE) - 1);
                        let count = mask.count_ones() as usize;
                        if count <= MAX_SELECTED {
                            state.selected_mask = mask;
                        }
                    }

                    ACTION_TYPE_PLAY => {
                        if state.plays_left > 0 {
                            let selected = build_selected_hand(&state.hand, state.selected_mask);
                            let selected_count = selected.len();

                            // ThePsychic: 必須剛好 5 張牌
                            let psychic_ok = !state.boss_blind
                                .map(|b| b.requires_five_cards() && selected_count != 5)
                                .unwrap_or(false);

                            if !psychic_ok {
                                // 不符合 ThePsychic 要求，不能出牌
                                // (保持狀態不變，等待重新選擇)
                            } else {
                                // 計算分數（考慮 Boss debuff 和卡片增強）
                                // 先提取需要的值以避免借用衝突
                                let jokers_clone = state.jokers.clone();
                                let boss_blind = state.boss_blind;
                                let score_result = calculate_play_score(
                                    &selected,
                                    &jokers_clone,
                                    boss_blind,
                                    &mut state.rng,
                                );
                                let score_gained = score_result.score;
                                let hand_id = score_result.hand_id;
                                let hand_type_idx = hand_type_index(hand_id);

                                // TheEye: 不能重複出同一種牌型
                                let eye_ok = !state.boss_blind
                                    .map(|b| matches!(b, BossBlind::TheEye) && state.played_hand_types.contains(&hand_type_idx))
                                    .unwrap_or(false);

                                // TheMouth: 只能出一種牌型
                                let mouth_ok = !state.boss_blind
                                    .map(|b| matches!(b, BossBlind::TheMouth) &&
                                         state.first_hand_type.is_some() &&
                                         state.first_hand_type != Some(hand_type_idx))
                                    .unwrap_or(false);

                                if !eye_ok || !mouth_ok {
                                    // 違反牌型限制，給予小懲罰但仍計分
                                    reward -= 0.1;
                                }

                                // 記錄牌型
                                state.played_hand_types.push(hand_type_idx);
                                if state.first_hand_type.is_none() {
                                    state.first_hand_type = Some(hand_type_idx);
                                }

                                state.score += score_gained;
                                state.plays_left -= 1;

                                // 獲得金幣（Gold Seal, Lucky）
                                state.money += score_result.money_gained;

                                // 處理 Glass 牌破碎：將打出的牌標記並移除
                                let selected_mask = state.selected_mask;
                                state.break_glass_cards(selected_mask, &score_result.glass_to_break);

                                let required = state.required_score();
                                reward += play_reward(score_gained, required);

                                // 檢查是否過關
                                if state.score >= required {
                                    let blind = state.blind_type.unwrap_or(BlindType::Small);
                                    reward += blind_clear_reward(state.plays_left, blind);
                                    state.reward = state.calc_reward();
                                    state.stage = Stage::PostBlind;
                                } else if state.plays_left == 0 {
                                    // 失敗
                                    state.stage = Stage::End(GameEnd::Lose);
                                    reward += game_end_reward(GameEnd::Lose, state.ante);
                                    done = true;
                                } else {
                                    // 繼續，發新牌
                                    state.deal();

                                    // TheSerpent: 每次出牌後抽 3 棄 3
                                    if state.boss_blind == Some(BossBlind::TheSerpent) {
                                        state.apply_serpent_effect();
                                    }

                                    // TheHook: 每手開始時隨機棄 2 張
                                    if state.boss_blind == Some(BossBlind::TheHook) {
                                        state.apply_hook_discard();
                                    }
                                }
                            }
                        }
                    }

                    ACTION_TYPE_DISCARD => {
                        if state.discards_left > 0 && state.selected_mask > 0 {
                            let old_potential = hand_potential(&state.hand);
                            let mask = state.selected_mask;

                            // 使用支援 Purple Seal 的棄牌方法
                            let purple_count = state.discard_with_seals(mask);

                            // Purple Seal 觸發的 Tarot 卡創建（待消耗品系統實作）
                            // 目前僅追蹤，不實際創建
                            let _ = purple_count;

                            let new_potential = hand_potential(&state.hand);
                            reward += (new_potential - old_potential).clamp(-0.3, 0.5);
                            state.discards_left -= 1;
                            state.selected_mask = 0;
                        }
                    }

                    _ => {}
                }
            }

            Stage::PostBlind => {
                if action_type == ACTION_TYPE_CASH_OUT {
                    state.money += state.reward;
                    state.reward = 0;
                    state.stage = Stage::Shop;

                    // 金幣獎勵：鼓勵存錢賺利息（考慮遊戲階段）
                    reward += money_reward(state.money, state.ante);

                    // 刷新商店
                    state.refresh_shop();
                }
            }

            Stage::Shop => {
                match action_type {
                    ACTION_TYPE_BUY_JOKER => {
                        let index = action_id as usize;
                        if let Some(joker) = state.shop.jokers.get(index) {
                            if joker.cost <= state.money
                                && state.jokers.len() < state.joker_slot_limit
                            {
                                // 記錄購買前狀態
                                let old_jokers = state.jokers.clone();
                                let money_before = state.money;
                                let cost = joker.cost;

                                state.money -= cost;
                                if let Some(bought) = state.shop.buy(index) {
                                    state.jokers.push(bought);
                                    // 基於組合分數變化的獎勵
                                    reward += joker_buy_reward(&old_jokers, &state.jokers, cost, money_before);
                                }
                            }
                        }
                    }

                    ACTION_TYPE_NEXT_ROUND => {
                        // 進入下一輪
                        let current_blind = state.blind_type.unwrap_or(BlindType::Small);

                        if current_blind == BlindType::Boss {
                            // Boss 過關，進入下一個 Ante
                            if let Some(next_ante) = state.ante.next() {
                                let old_ante = state.ante;
                                state.ante = next_ante;
                                reward += ante_progress_reward(old_ante, next_ante);
                                state.blind_type = None;
                                state.stage = Stage::PreBlind;
                                state.round += 1;
                            } else {
                                // Ante 8 過關，遊戲勝利
                                state.stage = Stage::End(GameEnd::Win);
                                reward += game_end_reward(GameEnd::Win, state.ante);
                                done = true;
                            }
                        } else {
                            // 進入下一個 Blind
                            state.stage = Stage::PreBlind;
                            state.round += 1;
                        }
                    }

                    _ => {}
                }
            }

            Stage::End(_) => {
                done = true;
            }
        }

        state.episode_step += 1;
        if state.episode_step >= MAX_STEPS {
            state.stage = Stage::End(GameEnd::Lose);
            done = true;
        }

        let observation = Observation {
            features: Some(observation_from_state(&state)),
            action_mask: Some(action_mask_from_state(&state, done)),
        };

        let info = EnvInfo {
            episode_step: state.episode_step,
            chips: state.score,
            mult: 1,
            blind_target: state.required_score(),
        };

        Ok(Response::new(StepResponse {
            observation: Some(observation),
            reward: reward as f32,
            done,
            info: Some(info),
        }))
    }

    async fn get_spec(
        &self,
        _request: Request<GetSpecRequest>,
    ) -> Result<Response<GetSpecResponse>, Status> {
        let observation = TensorSpec {
            shape: vec![OBS_SIZE],
            dtype: "f32".to_string(),
        };

        let action_mask = TensorSpec {
            shape: vec![ACTION_MASK_SIZE],
            dtype: "f32".to_string(),
        };

        Ok(Response::new(GetSpecResponse {
            observation: Some(observation),
            action_mask: Some(action_mask),
            action_space: ACTION_MASK_SIZE,
        }))
    }
}

fn load_joker_type_keys() -> Vec<String> {
    let path = Path::new("../data/jokers-meta.json");
    let Ok(data) = fs::read_to_string(path) else {
        return vec!["+c".to_string(), "+m".to_string(), "Xm".to_string()];
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&data) else {
        return vec!["+c".to_string(), "+m".to_string(), "Xm".to_string()];
    };

    let mut keys = Vec::new();
    if let Some(types) = parsed.get("types").and_then(|t| t.as_object()) {
        keys.extend(types.keys().cloned());
    }
    if keys.is_empty() {
        keys = vec!["+c".to_string(), "+m".to_string(), "Xm".to_string()];
    }
    keys
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    let env = EnvService::default();

    println!("JokerEnv gRPC server listening on {}", addr);
    println!("Full game flow enabled: PreBlind -> Blind -> PostBlind -> Shop -> ...");

    tonic::transport::Server::builder()
        .add_service(JokerEnvServer::new(env))
        .serve(addr)
        .await?;

    Ok(())
}
