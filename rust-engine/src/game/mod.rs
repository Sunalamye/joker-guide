//! 遊戲核心模組
//!
//! 包含 Balatro 遊戲的核心定義：
//! - `constants`: 遊戲常量
//! - `cards`: 卡牌、增強、封印、版本定義
//! - `blinds`: Blind、Boss Blind、Ante 定義
//! - `hand_types`: 牌型定義
//! - `scoring`: 計分引擎
//! - `joker`: Joker 系統 (Tiered Architecture)
//! - `shop`: 商店系統
//! - `tags`: Tag 系統（跳過 Blind 獎勵）
//! - `decks`: 起始牌組系統
//! - `stakes`: Stake 難度系統
//! - `vouchers`: Voucher 永久升級系統
//! - `consumables`: 消耗品系統 (Tarot/Planet/Spectral)
//! - `packs`: 卡包系統
//!
//! 注意：獎勵計算由 Python 端處理，Rust 端只提供遊戲狀態

#![allow(unused_imports)]

pub mod constants;
pub mod cards;
pub mod blinds;
pub mod hand_types;
pub mod scoring;
pub mod joker;
pub mod joker_def;
pub mod shop;
pub mod tags;
pub mod decks;
pub mod stakes;
pub mod vouchers;
pub mod consumables;
pub mod packs;

// Re-export 常用類型（公開 API，可能未在內部使用）
pub use constants::*;
pub use cards::{Card, Enhancement, Seal, Edition, standard_deck, card_index};
pub use blinds::{Stage, GameEnd, BlindType, BossBlind, Ante};
pub use hand_types::{HandId, HandScore};
pub use scoring::{score_hand, score_hand_with_rules, hand_potential, JokerRules};
pub use joker::{JokerId, JokerSlot, JokerBonus, ScoringContext, compute_joker_bonus, JOKER_COUNT};
pub use joker_def::{
    JokerState, CardFilter, Condition, StateField, CompareOp,
    BonusDef, EffectDef, CardScope, GameEvent, Rarity, TriggerContext,
    JokerBonus as JokerBonusNew,
    SPADE, DIAMOND, HEART, CLUB,
};
pub use shop::{Shop, ShopItem};
pub use tags::{Tag, TagId, TAG_COUNT};
pub use decks::{DeckType, DeckConfig, DECK_TYPE_COUNT};
pub use stakes::{Stake, StakeConfig, STAKE_COUNT};
pub use vouchers::{VoucherId, VoucherEffects, VOUCHER_COUNT};
pub use consumables::{
    Consumable, ConsumableType, ConsumableSlots, HandLevels,
    TarotId, PlanetId, SpectralId,
    CONSUMABLE_COUNT, TAROT_COUNT, PLANET_COUNT, SPECTRAL_COUNT, CONSUMABLE_SLOTS,
};
pub use packs::{PackType, PackContentType, PackOpeningState, PackItem, PackContents, PACK_TYPE_COUNT};
