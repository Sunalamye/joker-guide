//! 卡包系統
//!
//! 定義遊戲中的各種卡包類型及其內容生成邏輯

use rand::prelude::*;
use rand::rngs::StdRng;

use super::cards::{Card, Edition, Enhancement, Seal};
use super::consumables::{PlanetId, SpectralId, TarotId};
use super::joker::JokerId;

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
        match self {
            PackType::Arcana => "Arcana Pack",
            PackType::Celestial => "Celestial Pack",
            PackType::Spectral => "Spectral Pack",
            PackType::Standard => "Standard Pack",
            PackType::Buffoon => "Buffoon Pack",
            PackType::MegaArcana => "Mega Arcana Pack",
            PackType::MegaCelestial => "Mega Celestial Pack",
            PackType::MegaStandard => "Mega Standard Pack",
            PackType::JumboArcana => "Jumbo Arcana Pack",
            PackType::JumboCelestial => "Jumbo Celestial Pack",
            PackType::JumboStandard => "Jumbo Standard Pack",
            PackType::JumboBuffoon => "Jumbo Buffoon Pack",
        }
    }

    /// 卡包價格
    pub fn cost(&self) -> i64 {
        match self {
            PackType::Arcana => 4,
            PackType::Celestial => 4,
            PackType::Spectral => 4,
            PackType::Standard => 4,
            PackType::Buffoon => 4,
            PackType::MegaArcana => 8,
            PackType::MegaCelestial => 8,
            PackType::MegaStandard => 8,
            PackType::JumboArcana => 6,
            PackType::JumboCelestial => 6,
            PackType::JumboStandard => 6,
            PackType::JumboBuffoon => 6,
        }
    }

    /// 卡包提供的卡片數量
    pub fn card_count(&self) -> usize {
        match self {
            PackType::Arcana => 3,
            PackType::Celestial => 3,
            PackType::Spectral => 2,
            PackType::Standard => 3,
            PackType::Buffoon => 2,
            PackType::MegaArcana => 5,
            PackType::MegaCelestial => 5,
            PackType::MegaStandard => 5,
            PackType::JumboArcana => 5,
            PackType::JumboCelestial => 5,
            PackType::JumboStandard => 5,
            PackType::JumboBuffoon => 4,
        }
    }

    /// 可選擇的卡片數量
    pub fn pick_count(&self) -> usize {
        match self {
            PackType::Arcana => 1,
            PackType::Celestial => 1,
            PackType::Spectral => 1,
            PackType::Standard => 1,
            PackType::Buffoon => 1,
            PackType::MegaArcana => 2,
            PackType::MegaCelestial => 2,
            PackType::MegaStandard => 2,
            PackType::JumboArcana => 1,
            PackType::JumboCelestial => 1,
            PackType::JumboStandard => 1,
            PackType::JumboBuffoon => 1,
        }
    }

    /// 卡包內容類型
    pub fn content_type(&self) -> PackContentType {
        match self {
            PackType::Arcana | PackType::MegaArcana | PackType::JumboArcana => {
                PackContentType::Tarot
            }
            PackType::Celestial | PackType::MegaCelestial | PackType::JumboCelestial => {
                PackContentType::Planet
            }
            PackType::Spectral => PackContentType::Spectral,
            PackType::Standard | PackType::MegaStandard | PackType::JumboStandard => {
                PackContentType::PlayingCard
            }
            PackType::Buffoon | PackType::JumboBuffoon => PackContentType::Joker,
        }
    }

    /// to_index 用於 observation
    pub fn to_index(&self) -> usize {
        match self {
            PackType::Arcana => 0,
            PackType::Celestial => 1,
            PackType::Spectral => 2,
            PackType::Standard => 3,
            PackType::Buffoon => 4,
            PackType::MegaArcana => 5,
            PackType::MegaCelestial => 6,
            PackType::MegaStandard => 7,
            PackType::JumboArcana => 8,
            PackType::JumboCelestial => 9,
            PackType::JumboStandard => 10,
            PackType::JumboBuffoon => 11,
        }
    }

    /// 從索引轉換
    pub fn from_index(index: usize) -> Option<PackType> {
        match index {
            0 => Some(PackType::Arcana),
            1 => Some(PackType::Celestial),
            2 => Some(PackType::Spectral),
            3 => Some(PackType::Standard),
            4 => Some(PackType::Buffoon),
            5 => Some(PackType::MegaArcana),
            6 => Some(PackType::MegaCelestial),
            7 => Some(PackType::MegaStandard),
            8 => Some(PackType::JumboArcana),
            9 => Some(PackType::JumboCelestial),
            10 => Some(PackType::JumboStandard),
            11 => Some(PackType::JumboBuffoon),
            _ => None,
        }
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

/// 卡包內容類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackContentType {
    Tarot,
    Planet,
    Spectral,
    PlayingCard,
    Joker,
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
