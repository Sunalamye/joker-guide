//! 遊戲常量定義

// ============================================================================
// 遊戲規則常量
// ============================================================================

pub const HAND_SIZE: usize = 8;          // 手牌數量
pub const MAX_SELECTED: usize = 5;       // 最多選擇 5 張打出
pub const JOKER_SLOTS: usize = 5;        // Joker 欄位數
pub const PLAYS_PER_BLIND: i32 = 4;      // 每 Blind 出牌次數
pub const DISCARDS_PER_BLIND: i32 = 3;   // 每 Blind 棄牌次數
pub const STARTING_MONEY: i64 = 4;       // 起始金幣
pub const INTEREST_RATE: f32 = 0.1;      // 利息率
pub const MAX_INTEREST: i64 = 5;         // 最大利息
pub const MONEY_PER_REMAINING_HAND: i64 = 1;  // 每剩餘出牌次數獎勵
pub const SHOP_JOKER_COUNT: usize = 2;   // 商店 Joker 數量
pub const MAX_STEPS: i32 = 200;          // 最大步數

// ============================================================================
// Observation 常量
// ============================================================================

pub const SCALAR_COUNT: usize = 32;      // 標量特徵數（包含 13 個牌型等級）
pub const SELECTION_FEATURES: usize = HAND_SIZE;
pub const CARD_BASE_FEATURES: usize = 17;    // 13 rank + 4 suit
pub const CARD_ENHANCE_FEATURES: usize = 4;  // enhancement, seal, edition, face_down
pub const CARD_FEATURES: usize = CARD_BASE_FEATURES + CARD_ENHANCE_FEATURES;  // 21
pub const HAND_FEATURES: usize = HAND_SIZE * CARD_FEATURES;
pub const HAND_TYPE_COUNT: usize = 13;  // 含進階牌型: FiveKind, FlushHouse, FlushFive
pub const DECK_FEATURES: usize = 52;

// Joker 特徵: id (150) + enabled (1) + eternal (1) + negative (1) = 153 per joker
pub const JOKER_ID_SIZE: usize = 150;
pub const JOKER_SINGLE_FEATURES: usize = JOKER_ID_SIZE + 3;
pub const JOKER_FEATURES: usize = JOKER_SLOTS * JOKER_SINGLE_FEATURES;

pub const SHOP_FEATURES: usize = SHOP_JOKER_COUNT * (JOKER_ID_SIZE + 1); // id + cost

// 新增觀察空間
pub const BOSS_BLIND_COUNT: usize = 27;
pub const DECK_TYPE_FEATURES: usize = 12;    // DECK_TYPE_COUNT (one-hot)
pub const STAKE_FEATURES: usize = 8;         // STAKE_COUNT (one-hot)
pub const VOUCHER_FEATURES: usize = 34;      // VOUCHER_COUNT (binary flags for owned)
pub const CONSUMABLE_FEATURES: usize = 52;   // CONSUMABLE_COUNT per slot
pub const TAG_FEATURES: usize = 24;          // TAG_COUNT (count of each tag)

pub const OBS_SIZE: i32 = (SCALAR_COUNT
    + SELECTION_FEATURES
    + HAND_FEATURES
    + HAND_TYPE_COUNT
    + DECK_FEATURES
    + JOKER_FEATURES
    + SHOP_FEATURES
    + BOSS_BLIND_COUNT                       // Boss Blind one-hot
    + DECK_TYPE_FEATURES                     // Deck type one-hot
    + STAKE_FEATURES                         // Stake one-hot
    + VOUCHER_FEATURES                       // Voucher ownership flags
    + CONSUMABLE_SLOT_COUNT * CONSUMABLE_FEATURES  // Consumables
    + TAG_FEATURES                           // Tag counts
) as i32;

// ============================================================================
// Action 常量
// ============================================================================

pub const ACTION_TYPE_SELECT: i32 = 0;
pub const ACTION_TYPE_PLAY: i32 = 1;
pub const ACTION_TYPE_DISCARD: i32 = 2;
pub const ACTION_TYPE_SELECT_BLIND: i32 = 3;
pub const ACTION_TYPE_CASH_OUT: i32 = 4;
pub const ACTION_TYPE_BUY_JOKER: i32 = 5;
pub const ACTION_TYPE_NEXT_ROUND: i32 = 6;
pub const ACTION_TYPE_REROLL: i32 = 7;
pub const ACTION_TYPE_SELL_JOKER: i32 = 8;
pub const ACTION_TYPE_SKIP_BLIND: i32 = 9;
pub const ACTION_TYPE_USE_CONSUMABLE: i32 = 10;
pub const ACTION_TYPE_BUY_VOUCHER: i32 = 11;
pub const ACTION_TYPE_BUY_PACK: i32 = 12;

pub const ACTION_TYPE_COUNT: i32 = 13;

// 消耗品和 Voucher 常量
pub const CONSUMABLE_SLOT_COUNT: usize = 2;  // 消耗品槽位
pub const SHOP_VOUCHER_COUNT: usize = 1;     // 商店 Voucher 槽位
pub const SHOP_PACK_COUNT: usize = 2;        // 商店卡包槽位

// Action mask layout:
// [0..13]: Action types (13)
// [13..13+HAND_SIZE*2]: Card selection (16)
// [29..32]: Blind selection (3)
// [32..32+SHOP_JOKER_COUNT]: Shop joker purchase (2)
// [34..34+JOKER_SLOTS]: Sell joker slots (5)
// [39]: Reroll (1)
// [40]: Skip Blind (1)
// [41..41+CONSUMABLE_SLOT_COUNT]: Use consumable (2)
// [43]: Buy voucher (1)
// [44..44+SHOP_PACK_COUNT]: Buy pack (2)
pub const ACTION_MASK_SIZE: i32 = ACTION_TYPE_COUNT
    + (HAND_SIZE as i32 * 2)
    + 3
    + SHOP_JOKER_COUNT as i32
    + JOKER_SLOTS as i32
    + 1
    + 1
    + CONSUMABLE_SLOT_COUNT as i32
    + SHOP_VOUCHER_COUNT as i32
    + SHOP_PACK_COUNT as i32;
