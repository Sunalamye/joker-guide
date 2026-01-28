//! 卡包系統
//!
//! 定義遊戲中的各種卡包類型及其內容生成邏輯
//!
//! # 架構
//!
//! 使用聲明式 `PACK_DEFS` 表定義所有卡包的元數據。

use rand::prelude::*;
use rand::rngs::StdRng;

use super::cards::{Card, Edition, Enhancement, Seal};
use super::consumables::{PlanetId, SpectralId, TarotId};
use super::joker::JokerId;

// ============================================================================
// Pack 定義系統
// ============================================================================

/// 卡包內容類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackContentType {
    Tarot,
    Planet,
    Spectral,
    PlayingCard,
    Joker,
}

/// Pack 定義結構
#[derive(Clone, Copy)]
pub struct PackDef {
    pub name: &'static str,
    pub cost: i64,
    pub card_count: usize,
    pub pick_count: usize,
    pub content_type: PackContentType,
}

/// Pack 定義表（順序與 PackType 枚舉一致）
pub static PACK_DEFS: [PackDef; 12] = [
    // 標準卡包 (0-4)
    PackDef { name: "Arcana Pack", cost: 4, card_count: 3, pick_count: 1, content_type: PackContentType::Tarot },
    PackDef { name: "Celestial Pack", cost: 4, card_count: 3, pick_count: 1, content_type: PackContentType::Planet },
    PackDef { name: "Spectral Pack", cost: 4, card_count: 2, pick_count: 1, content_type: PackContentType::Spectral },
    PackDef { name: "Standard Pack", cost: 4, card_count: 3, pick_count: 1, content_type: PackContentType::PlayingCard },
    PackDef { name: "Buffoon Pack", cost: 4, card_count: 2, pick_count: 1, content_type: PackContentType::Joker },
    // Mega 卡包 (5-7)
    PackDef { name: "Mega Arcana Pack", cost: 8, card_count: 5, pick_count: 2, content_type: PackContentType::Tarot },
    PackDef { name: "Mega Celestial Pack", cost: 8, card_count: 5, pick_count: 2, content_type: PackContentType::Planet },
    PackDef { name: "Mega Standard Pack", cost: 8, card_count: 5, pick_count: 2, content_type: PackContentType::PlayingCard },
    // Jumbo 卡包 (8-11)
    PackDef { name: "Jumbo Arcana Pack", cost: 6, card_count: 5, pick_count: 1, content_type: PackContentType::Tarot },
    PackDef { name: "Jumbo Celestial Pack", cost: 6, card_count: 5, pick_count: 1, content_type: PackContentType::Planet },
    PackDef { name: "Jumbo Standard Pack", cost: 6, card_count: 5, pick_count: 1, content_type: PackContentType::PlayingCard },
    PackDef { name: "Jumbo Buffoon Pack", cost: 6, card_count: 4, pick_count: 1, content_type: PackContentType::Joker },
];

/// 卡包類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackType {
    // 標準卡包
    /// Arcana Pack: 3 張 Tarot，選 1 張
    Arcana,
    /// Celestial Pack: 3 張 Planet，選 1 張
    Celestial,
    /// Spectral Pack: 2 張 Spectral，選 1 張
    Spectral,
    /// Standard Pack: 3 張撲克牌，選 1 張
    Standard,
    /// Buffoon Pack: 2 張 Joker，選 1 張
    Buffoon,

    // Mega 卡包（更多選擇）
    /// Mega Arcana Pack: 5 張 Tarot，選 2 張
    MegaArcana,
    /// Mega Celestial Pack: 5 張 Planet，選 2 張
    MegaCelestial,
    /// Mega Standard Pack: 5 張撲克牌，選 2 張
    MegaStandard,

    // Jumbo 卡包（更多選擇）
    /// Jumbo Arcana Pack: 5 張 Tarot，選 1 張
    JumboArcana,
    /// Jumbo Celestial Pack: 5 張 Planet，選 1 張
    JumboCelestial,
    /// Jumbo Standard Pack: 5 張撲克牌，選 1 張
    JumboStandard,
    /// Jumbo Buffoon Pack: 4 張 Joker，選 1 張
    JumboBuffoon,
}

impl PackType {
    /// 所有卡包類型
    pub fn all() -> &'static [PackType] {
        &[
            PackType::Arcana,
            PackType::Celestial,
            PackType::Spectral,
            PackType::Standard,
            PackType::Buffoon,
            PackType::MegaArcana,
            PackType::MegaCelestial,
            PackType::MegaStandard,
            PackType::JumboArcana,
            PackType::JumboCelestial,
            PackType::JumboStandard,
            PackType::JumboBuffoon,
        ]
    }

    /// 卡包名稱
    pub fn name(&self) -> &'static str {
        PACK_DEFS[self.to_index()].name
    }

    /// 卡包價格
    pub fn cost(&self) -> i64 {
        PACK_DEFS[self.to_index()].cost
    }

    /// 卡包提供的卡片數量
    pub fn card_count(&self) -> usize {
        PACK_DEFS[self.to_index()].card_count
    }

    /// 可選擇的卡片數量
    pub fn pick_count(&self) -> usize {
        PACK_DEFS[self.to_index()].pick_count
    }

    /// 卡包內容類型
    pub fn content_type(&self) -> PackContentType {
        PACK_DEFS[self.to_index()].content_type
    }

    /// to_index 用於 observation
    pub fn to_index(&self) -> usize {
        Self::all().iter().position(|p| p == self).unwrap_or(0)
    }

    /// 從索引轉換
    pub fn from_index(index: usize) -> Option<PackType> {
        Self::all().get(index).copied()
    }

    /// 隨機選擇一個卡包類型（商店生成用）
    pub fn random(rng: &mut StdRng) -> Self {
        // 基礎卡包較常見
        let weights = [
            (PackType::Arcana, 20),
            (PackType::Celestial, 20),
            (PackType::Spectral, 5),
            (PackType::Standard, 20),
            (PackType::Buffoon, 15),
            (PackType::MegaArcana, 4),
            (PackType::MegaCelestial, 4),
            (PackType::MegaStandard, 4),
            (PackType::JumboArcana, 2),
            (PackType::JumboCelestial, 2),
            (PackType::JumboStandard, 2),
            (PackType::JumboBuffoon, 2),
        ];

        let total: u32 = weights.iter().map(|(_, w)| w).sum();
        let roll = rng.gen_range(0..total);

        let mut cumulative = 0;
        for (pack, weight) in weights.iter() {
            cumulative += weight;
            if roll < cumulative {
                return *pack;
            }
        }

        PackType::Arcana
    }
}

/// 卡包開啟狀態
#[derive(Clone, Debug)]
pub struct PackOpeningState {
    /// 正在開啟的卡包類型
    pub pack_type: PackType,
    /// 可選擇的內容索引
    pub available_choices: Vec<usize>,
    /// 剩餘可選數量
    pub picks_remaining: usize,
    /// 是否已完成選擇
    pub completed: bool,
}

impl PackOpeningState {
    /// 創建新的卡包開啟狀態
    pub fn new(pack_type: PackType) -> Self {
        let card_count = pack_type.card_count();
        Self {
            pack_type,
            available_choices: (0..card_count).collect(),
            picks_remaining: pack_type.pick_count(),
            completed: false,
        }
    }

    /// 選擇一個項目
    pub fn pick(&mut self, index: usize) -> bool {
        if self.completed || self.picks_remaining == 0 {
            return false;
        }

        if let Some(pos) = self.available_choices.iter().position(|&x| x == index) {
            self.available_choices.remove(pos);
            self.picks_remaining -= 1;

            if self.picks_remaining == 0 {
                self.completed = true;
            }
            true
        } else {
            false
        }
    }

    /// 跳過剩餘選擇
    pub fn skip(&mut self) {
        self.picks_remaining = 0;
        self.completed = true;
    }
}

/// 卡包類型總數
pub const PACK_TYPE_COUNT: usize = 12;

// ============================================================================
// 卡包內容生成
// ============================================================================

/// 卡包內容項目
#[derive(Clone, Debug)]
pub enum PackItem {
    Tarot(TarotId),
    Planet(PlanetId),
    Spectral(SpectralId),
    Joker(JokerId, Edition),
    PlayingCard(Card),
}

/// 卡包內容
#[derive(Clone, Debug)]
pub struct PackContents {
    pub pack_type: PackType,
    pub items: Vec<PackItem>,
}

impl PackContents {
    /// 生成卡包內容
    pub fn generate(pack_type: PackType, rng: &mut StdRng) -> Self {
        let count = pack_type.card_count();
        let items = match pack_type.content_type() {
            PackContentType::Tarot => Self::generate_tarots(count, rng),
            PackContentType::Planet => Self::generate_planets(count, rng),
            PackContentType::Spectral => Self::generate_spectrals(count, rng),
            PackContentType::Joker => Self::generate_jokers(count, rng),
            PackContentType::PlayingCard => Self::generate_playing_cards(count, rng),
        };

        Self { pack_type, items }
    }

    /// 生成 Tarot 卡
    fn generate_tarots(count: usize, rng: &mut StdRng) -> Vec<PackItem> {
        let all_tarots = TarotId::all();
        let mut result = Vec::with_capacity(count);
        let mut used = std::collections::HashSet::new();

        while result.len() < count && used.len() < all_tarots.len() {
            let idx = rng.gen_range(0..all_tarots.len());
            if !used.contains(&idx) {
                used.insert(idx);
                result.push(PackItem::Tarot(all_tarots[idx]));
            }
        }

        result
    }

    /// 生成 Planet 卡
    fn generate_planets(count: usize, rng: &mut StdRng) -> Vec<PackItem> {
        let all_planets = PlanetId::all();
        let mut result = Vec::with_capacity(count);
        let mut used = std::collections::HashSet::new();

        while result.len() < count && used.len() < all_planets.len() {
            let idx = rng.gen_range(0..all_planets.len());
            if !used.contains(&idx) {
                used.insert(idx);
                result.push(PackItem::Planet(all_planets[idx]));
            }
        }

        result
    }

    /// 生成 Spectral 卡
    fn generate_spectrals(count: usize, rng: &mut StdRng) -> Vec<PackItem> {
        let all_spectrals = SpectralId::all();
        let mut result = Vec::with_capacity(count);
        let mut used = std::collections::HashSet::new();

        while result.len() < count && used.len() < all_spectrals.len() {
            let idx = rng.gen_range(0..all_spectrals.len());
            if !used.contains(&idx) {
                used.insert(idx);
                result.push(PackItem::Spectral(all_spectrals[idx]));
            }
        }

        result
    }

    /// 生成 Joker
    fn generate_jokers(count: usize, rng: &mut StdRng) -> Vec<PackItem> {
        let mut result = Vec::with_capacity(count);

        for _ in 0..count {
            // 隨機稀有度: Common (60%), Uncommon (30%), Rare (10%)
            let joker_id = {
                let roll = rng.gen_range(0..100);
                if roll < 60 {
                    JokerId::random_common(rng)
                } else if roll < 90 {
                    // Uncommon - 使用 by_rarity(2) 並隨機選擇
                    let uncommon = JokerId::by_rarity(2);
                    if uncommon.is_empty() {
                        JokerId::random_common(rng)
                    } else {
                        uncommon[rng.gen_range(0..uncommon.len())]
                    }
                } else {
                    JokerId::random_rare(rng)
                }
            };

            // 隨機版本: Base (85%), Foil (7%), Holo (5%), Poly (3%)
            let edition = {
                let roll = rng.gen_range(0..100);
                if roll < 85 {
                    Edition::Base
                } else if roll < 92 {
                    Edition::Foil
                } else if roll < 97 {
                    Edition::Holographic
                } else {
                    Edition::Polychrome
                }
            };

            result.push(PackItem::Joker(joker_id, edition));
        }

        result
    }

    /// 生成撲克牌
    fn generate_playing_cards(count: usize, rng: &mut StdRng) -> Vec<PackItem> {
        let mut result = Vec::with_capacity(count);

        for _ in 0..count {
            let rank = rng.gen_range(1..=13);
            let suit = rng.gen_range(0..4);
            let mut card = Card::new(rank, suit);

            // 隨機增強: None (70%), 其他 (30%)
            if rng.gen_range(0..100) >= 70 {
                card.enhancement = match rng.gen_range(0..8) {
                    0 => Enhancement::Bonus,
                    1 => Enhancement::Mult,
                    2 => Enhancement::Wild,
                    3 => Enhancement::Glass,
                    4 => Enhancement::Steel,
                    5 => Enhancement::Stone,
                    6 => Enhancement::Gold,
                    7 => Enhancement::Lucky,
                    _ => Enhancement::None,
                };
            }

            // 隨機封印: None (85%), 其他 (15%)
            if rng.gen_range(0..100) >= 85 {
                card.seal = match rng.gen_range(0..4) {
                    0 => Seal::Gold,
                    1 => Seal::Red,
                    2 => Seal::Blue,
                    3 => Seal::Purple,
                    _ => Seal::None,
                };
            }

            // 隨機版本: None (90%), 其他 (10%)
            if rng.gen_range(0..100) >= 90 {
                card.edition = match rng.gen_range(0..3) {
                    0 => Edition::Foil,
                    1 => Edition::Holographic,
                    2 => Edition::Polychrome,
                    _ => Edition::Base,
                };
            }

            result.push(PackItem::PlayingCard(card));
        }

        result
    }
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_type_all() {
        assert_eq!(PackType::all().len(), PACK_TYPE_COUNT);
    }

    #[test]
    fn test_pack_costs() {
        assert_eq!(PackType::Arcana.cost(), 4);
        assert_eq!(PackType::MegaArcana.cost(), 8);
        assert_eq!(PackType::JumboArcana.cost(), 6);
    }

    #[test]
    fn test_pack_opening() {
        let mut state = PackOpeningState::new(PackType::MegaArcana);
        assert_eq!(state.available_choices.len(), 5);
        assert_eq!(state.picks_remaining, 2);

        assert!(state.pick(0));
        assert_eq!(state.picks_remaining, 1);
        assert!(!state.completed);

        assert!(state.pick(2));
        assert_eq!(state.picks_remaining, 0);
        assert!(state.completed);

        // 無法再選
        assert!(!state.pick(1));
    }

    #[test]
    fn test_pack_index() {
        for pack in PackType::all() {
            let index = pack.to_index();
            assert_eq!(PackType::from_index(index), Some(*pack));
        }
    }
}
