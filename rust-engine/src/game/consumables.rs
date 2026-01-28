//! 消耗品系統
//!
//! 包含三種類型的消耗品：
//! - Tarot: 修改卡牌
//! - Planet: 升級牌型
//! - Spectral: 特殊效果
//!
//! # 架構
//!
//! 使用聲明式定義表簡化 name() 和 to_index() 方法。

use rand::prelude::*;
use rand::rngs::StdRng;

/// 消耗品槽位上限
pub const CONSUMABLE_SLOTS: usize = 2;

/// Tarot 卡數量
pub const TAROT_COUNT: usize = 22;
/// Planet 卡數量
pub const PLANET_COUNT: usize = 12;
/// Spectral 卡數量
pub const SPECTRAL_COUNT: usize = 18;
/// 消耗品總數
pub const CONSUMABLE_COUNT: usize = TAROT_COUNT + PLANET_COUNT + SPECTRAL_COUNT;

// ============================================================================
// Tarot 定義表
// ============================================================================

/// Tarot 名稱表（順序與 TarotId 枚舉一致）
static TAROT_NAMES: [&str; TAROT_COUNT] = [
    "The Fool", "The Magician", "The High Priestess", "The Empress",
    "The Emperor", "The Hierophant", "The Lovers", "The Chariot",
    "Justice", "The Hermit", "The Wheel of Fortune", "Strength",
    "The Hanged Man", "Death", "Temperance", "The Devil",
    "The Tower", "The Star", "The Moon", "The Sun",
    "Judgement", "The World",
];

// ============================================================================
// Planet 定義表
// ============================================================================

/// Planet 定義結構
#[derive(Clone, Copy)]
struct PlanetDef {
    name: &'static str,
    hand_type_index: usize,
}

/// Planet 定義表（順序與 PlanetId 枚舉一致）
static PLANET_DEFS: [PlanetDef; PLANET_COUNT] = [
    PlanetDef { name: "Mercury", hand_type_index: 1 },  // Pair
    PlanetDef { name: "Venus", hand_type_index: 3 },    // Three of a Kind
    PlanetDef { name: "Earth", hand_type_index: 6 },    // Full House
    PlanetDef { name: "Mars", hand_type_index: 7 },     // Four of a Kind
    PlanetDef { name: "Jupiter", hand_type_index: 5 },  // Flush
    PlanetDef { name: "Saturn", hand_type_index: 4 },   // Straight
    PlanetDef { name: "Uranus", hand_type_index: 2 },   // Two Pair
    PlanetDef { name: "Neptune", hand_type_index: 8 },  // Straight Flush
    PlanetDef { name: "Pluto", hand_type_index: 0 },    // High Card
    PlanetDef { name: "Planet X", hand_type_index: 9 }, // Five of a Kind
    PlanetDef { name: "Ceres", hand_type_index: 10 },   // Flush House
    PlanetDef { name: "Eris", hand_type_index: 11 },    // Flush Five
];

// ============================================================================
// Spectral 定義表
// ============================================================================

/// Spectral 名稱表（順序與 SpectralId 枚舉一致）
static SPECTRAL_NAMES: [&str; SPECTRAL_COUNT] = [
    "Familiar", "Grim", "Incantation", "Talisman", "Aura", "Wraith",
    "Sigil", "Ouija", "Ectoplasm", "Immolate", "Ankh", "Deja Vu",
    "Hex", "Trance", "Medium", "Cryptid", "The Soul", "Black Hole",
];

/// 消耗品類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsumableType {
    Tarot,
    Planet,
    Spectral,
}

/// Tarot 卡 ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TarotId {
    /// The Fool: 複製最後使用的 Tarot/Planet
    TheFool,
    /// The Magician: 增強選中牌為 Lucky
    TheMagician,
    /// The High Priestess: 創造最多 2 張 Planet 卡
    TheHighPriestess,
    /// The Empress: 增強選中牌為 Mult 卡
    TheEmpress,
    /// The Emperor: 創造最多 2 張 Tarot 卡
    TheEmperor,
    /// The Hierophant: 增強選中牌為 Bonus 卡
    TheHierophant,
    /// The Lovers: 增強選中牌為 Wild 卡
    TheLovers,
    /// The Chariot: 增強選中牌為 Steel 卡
    TheChariot,
    /// Justice: 增強選中牌為 Glass 卡
    Justice,
    /// The Hermit: 金錢翻倍（最高 $20）
    TheHermit,
    /// The Wheel of Fortune: 1/4 機率加 Foil/Holo/Poly 到隨機 Joker
    TheWheelOfFortune,
    /// Strength: 提升選中牌的點數 +1
    Strength,
    /// The Hanged Man: 銷毀最多 2 張選中的牌
    TheHangedMan,
    /// Death: 選 2 張牌，左邊變成右邊的複製
    Death,
    /// Temperance: 獲得 Joker 總售價（最高 $50）
    Temperance,
    /// The Devil: 增強選中牌為 Gold 卡
    TheDevil,
    /// The Tower: 增強選中牌為 Stone 卡
    TheTower,
    /// The Star: 將選中的牌轉為鑽石花色
    TheStar,
    /// The Moon: 將選中的牌轉為梅花花色
    TheMoon,
    /// The Sun: 將選中的牌轉為紅心花色
    TheSun,
    /// Judgement: 創造隨機 Joker
    Judgement,
    /// The World: 將選中的牌轉為黑桃花色
    TheWorld,
}

impl TarotId {
    /// 所有 Tarot 卡
    pub fn all() -> &'static [TarotId] {
        &[
            TarotId::TheFool,
            TarotId::TheMagician,
            TarotId::TheHighPriestess,
            TarotId::TheEmpress,
            TarotId::TheEmperor,
            TarotId::TheHierophant,
            TarotId::TheLovers,
            TarotId::TheChariot,
            TarotId::Justice,
            TarotId::TheHermit,
            TarotId::TheWheelOfFortune,
            TarotId::Strength,
            TarotId::TheHangedMan,
            TarotId::Death,
            TarotId::Temperance,
            TarotId::TheDevil,
            TarotId::TheTower,
            TarotId::TheStar,
            TarotId::TheMoon,
            TarotId::TheSun,
            TarotId::Judgement,
            TarotId::TheWorld,
        ]
    }

    /// 名稱（使用 TAROT_NAMES 表查詢）
    pub fn name(&self) -> &'static str {
        TAROT_NAMES[self.to_index()]
    }

    /// 需要選擇的牌數量（0 = 不需要選牌）
    pub fn selection_count(&self) -> (usize, usize) {
        match self {
            TarotId::TheMagician | TarotId::TheEmpress | TarotId::TheHierophant
            | TarotId::TheLovers | TarotId::TheChariot | TarotId::Justice
            | TarotId::TheDevil | TarotId::TheTower | TarotId::TheStar
            | TarotId::TheMoon | TarotId::TheSun | TarotId::TheWorld => (1, 3),
            TarotId::Strength | TarotId::TheHangedMan => (1, 2),
            TarotId::Death => (2, 2),
            _ => (0, 0),
        }
    }

    /// 轉換為索引（使用 all() 數組位置查詢）
    pub fn to_index(&self) -> usize {
        Self::all().iter().position(|t| t == self).unwrap_or(0)
    }

    /// 從索引創建
    pub fn from_index(index: usize) -> Option<Self> {
        TarotId::all().get(index).copied()
    }

    /// 隨機選擇
    pub fn random(rng: &mut StdRng) -> Self {
        *TarotId::all().choose(rng).unwrap()
    }
}

/// Planet 卡 ID（對應牌型升級）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanetId {
    /// Mercury: 升級 Pair
    Mercury,
    /// Venus: 升級 Three of a Kind
    Venus,
    /// Earth: 升級 Full House
    Earth,
    /// Mars: 升級 Four of a Kind
    Mars,
    /// Jupiter: 升級 Flush
    Jupiter,
    /// Saturn: 升級 Straight
    Saturn,
    /// Uranus: 升級 Two Pair
    Uranus,
    /// Neptune: 升級 Straight Flush
    Neptune,
    /// Pluto: 升級 High Card
    Pluto,
    /// Planet X: 升級 Five of a Kind
    PlanetX,
    /// Ceres: 升級 Flush House
    Ceres,
    /// Eris: 升級 Flush Five
    Eris,
}

impl PlanetId {
    /// 所有 Planet 卡
    pub fn all() -> &'static [PlanetId] {
        &[
            PlanetId::Mercury,
            PlanetId::Venus,
            PlanetId::Earth,
            PlanetId::Mars,
            PlanetId::Jupiter,
            PlanetId::Saturn,
            PlanetId::Uranus,
            PlanetId::Neptune,
            PlanetId::Pluto,
            PlanetId::PlanetX,
            PlanetId::Ceres,
            PlanetId::Eris,
        ]
    }

    /// 名稱（使用 PLANET_DEFS 表查詢）
    pub fn name(&self) -> &'static str {
        PLANET_DEFS[self.to_index()].name
    }

    /// 對應的牌型索引（使用 PLANET_DEFS 表查詢）
    pub fn hand_type_index(&self) -> usize {
        PLANET_DEFS[self.to_index()].hand_type_index
    }

    /// 從牌型索引創建 Planet（使用 PLANET_DEFS 表查詢）
    pub fn from_hand_type_index(idx: usize) -> Option<PlanetId> {
        PLANET_DEFS.iter()
            .position(|def| def.hand_type_index == idx)
            .and_then(Self::from_index)
    }

    /// 轉換為索引（使用 all() 數組位置查詢）
    pub fn to_index(&self) -> usize {
        Self::all().iter().position(|p| p == self).unwrap_or(0)
    }

    /// 從索引創建
    pub fn from_index(index: usize) -> Option<Self> {
        PlanetId::all().get(index).copied()
    }

    /// 隨機選擇
    pub fn random(rng: &mut StdRng) -> Self {
        *PlanetId::all().choose(rng).unwrap()
    }
}

/// Spectral 卡 ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpectralId {
    /// Familiar: 銷毀 1 張，加 3 張隨機 Face Card
    Familiar,
    /// Grim: 銷毀 1 張，加 2 張隨機 Ace
    Grim,
    /// Incantation: 銷毀 1 張，加 4 張隨機數字牌
    Incantation,
    /// Talisman: 加 Gold Seal 到選中的牌
    Talisman,
    /// Aura: 加 Foil/Holo/Poly 到選中的牌
    Aura,
    /// Wraith: 創造稀有 Joker，金錢設為 $0
    Wraith,
    /// Sigil: 將所有手牌轉為隨機花色
    Sigil,
    /// Ouija: 將所有手牌轉為隨機點數，-1 手牌大小
    Ouija,
    /// Ectoplasm: 加 Negative 到隨機 Joker，-1 手牌大小
    Ectoplasm,
    /// Immolate: 銷毀 5 張隨機牌，獲得 $20
    Immolate,
    /// Ankh: 複製 1 個隨機 Joker，銷毀其他所有 Joker
    Ankh,
    /// Deja Vu: 加 Red Seal 到選中的牌
    DejaVu,
    /// Hex: 加 Polychrome 到隨機 Joker，銷毀其他
    Hex,
    /// Trance: 加 Blue Seal 到選中的牌
    Trance,
    /// Medium: 加 Purple Seal 到選中的牌
    Medium,
    /// Cryptid: 在牌組中創造 2 張選中牌的複製
    Cryptid,
    /// The Soul: 創造傳奇 Joker
    TheSoul,
    /// Black Hole: 所有牌型升級 1 級
    BlackHole,
}

impl SpectralId {
    /// 所有 Spectral 卡
    pub fn all() -> &'static [SpectralId] {
        &[
            SpectralId::Familiar,
            SpectralId::Grim,
            SpectralId::Incantation,
            SpectralId::Talisman,
            SpectralId::Aura,
            SpectralId::Wraith,
            SpectralId::Sigil,
            SpectralId::Ouija,
            SpectralId::Ectoplasm,
            SpectralId::Immolate,
            SpectralId::Ankh,
            SpectralId::DejaVu,
            SpectralId::Hex,
            SpectralId::Trance,
            SpectralId::Medium,
            SpectralId::Cryptid,
            SpectralId::TheSoul,
            SpectralId::BlackHole,
        ]
    }

    /// 名稱（使用 SPECTRAL_NAMES 表查詢）
    pub fn name(&self) -> &'static str {
        SPECTRAL_NAMES[self.to_index()]
    }

    /// 需要選擇的牌數量
    pub fn selection_count(&self) -> (usize, usize) {
        match self {
            SpectralId::Familiar | SpectralId::Grim | SpectralId::Incantation
            | SpectralId::Talisman | SpectralId::Aura | SpectralId::DejaVu
            | SpectralId::Trance | SpectralId::Medium | SpectralId::Cryptid => (1, 1),
            _ => (0, 0),
        }
    }

    /// 轉換為索引（使用 all() 數組位置查詢）
    pub fn to_index(&self) -> usize {
        Self::all().iter().position(|s| s == self).unwrap_or(0)
    }

    /// 從索引創建
    pub fn from_index(index: usize) -> Option<Self> {
        SpectralId::all().get(index).copied()
    }

    /// 隨機選擇
    pub fn random(rng: &mut StdRng) -> Self {
        *SpectralId::all().choose(rng).unwrap()
    }
}

/// 消耗品
#[derive(Clone, Debug)]
pub enum Consumable {
    Tarot(TarotId),
    Planet(PlanetId),
    Spectral(SpectralId),
}

impl Consumable {
    /// 獲取消耗品類型
    pub fn consumable_type(&self) -> ConsumableType {
        match self {
            Consumable::Tarot(_) => ConsumableType::Tarot,
            Consumable::Planet(_) => ConsumableType::Planet,
            Consumable::Spectral(_) => ConsumableType::Spectral,
        }
    }

    /// 獲取名稱
    pub fn name(&self) -> &'static str {
        match self {
            Consumable::Tarot(id) => id.name(),
            Consumable::Planet(id) => id.name(),
            Consumable::Spectral(id) => id.name(),
        }
    }

    /// 獲取購買價格
    pub fn cost(&self) -> i64 {
        match self {
            Consumable::Tarot(_) => 3,
            Consumable::Planet(_) => 3,
            Consumable::Spectral(_) => 4,
        }
    }

    /// 需要選擇的牌數量 (min, max)
    pub fn selection_count(&self) -> (usize, usize) {
        match self {
            Consumable::Tarot(id) => id.selection_count(),
            Consumable::Planet(_) => (0, 0),
            Consumable::Spectral(id) => id.selection_count(),
        }
    }

    /// 轉換為全域索引（Tarot: 0-21, Planet: 22-33, Spectral: 34-51）
    pub fn to_global_index(&self) -> usize {
        match self {
            Consumable::Tarot(id) => id.to_index(),
            Consumable::Planet(id) => TAROT_COUNT + id.to_index(),
            Consumable::Spectral(id) => TAROT_COUNT + PLANET_COUNT + id.to_index(),
        }
    }

    /// 從全域索引創建
    pub fn from_global_index(index: usize) -> Option<Self> {
        if index < TAROT_COUNT {
            TarotId::from_index(index).map(Consumable::Tarot)
        } else if index < TAROT_COUNT + PLANET_COUNT {
            PlanetId::from_index(index - TAROT_COUNT).map(Consumable::Planet)
        } else if index < CONSUMABLE_COUNT {
            SpectralId::from_index(index - TAROT_COUNT - PLANET_COUNT).map(Consumable::Spectral)
        } else {
            None
        }
    }

    /// 隨機生成 Tarot
    pub fn random_tarot(rng: &mut StdRng) -> Self {
        Consumable::Tarot(TarotId::random(rng))
    }

    /// 隨機生成 Planet
    pub fn random_planet(rng: &mut StdRng) -> Self {
        Consumable::Planet(PlanetId::random(rng))
    }

    /// 隨機生成 Spectral
    pub fn random_spectral(rng: &mut StdRng) -> Self {
        Consumable::Spectral(SpectralId::random(rng))
    }
}

/// 消耗品槽位
#[derive(Clone, Debug, Default)]
pub struct ConsumableSlots {
    /// 消耗品列表
    pub items: Vec<Consumable>,
    /// 槽位上限
    pub capacity: usize,
}

impl ConsumableSlots {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            capacity: CONSUMABLE_SLOTS,
        }
    }

    /// 添加消耗品
    pub fn add(&mut self, consumable: Consumable) -> bool {
        if self.items.len() < self.capacity {
            self.items.push(consumable);
            true
        } else {
            false
        }
    }

    /// 使用消耗品（移除並返回）
    pub fn use_item(&mut self, index: usize) -> Option<Consumable> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    /// 是否已滿
    pub fn is_full(&self) -> bool {
        self.items.len() >= self.capacity
    }

    /// 剩餘空間
    pub fn remaining(&self) -> usize {
        self.capacity - self.items.len()
    }
}

/// 牌型等級追蹤器
#[derive(Clone, Debug)]
pub struct HandLevels {
    /// 每種牌型的等級（0-indexed）
    levels: [u32; 12],
}

impl Default for HandLevels {
    fn default() -> Self {
        Self::new()
    }
}

impl HandLevels {
    pub fn new() -> Self {
        Self { levels: [1; 12] } // 所有牌型起始等級 1
    }

    /// 獲取牌型等級
    pub fn get(&self, hand_type: usize) -> u32 {
        self.levels.get(hand_type).copied().unwrap_or(1)
    }

    /// 升級牌型
    pub fn upgrade(&mut self, hand_type: usize) {
        if let Some(level) = self.levels.get_mut(hand_type) {
            *level += 1;
        }
    }

    /// 全部升級（Black Hole 效果）
    pub fn upgrade_all(&mut self) {
        for level in &mut self.levels {
            *level += 1;
        }
    }

    /// 降級牌型（TheArm Boss Blind 效果）
    /// 等級最低為 1
    pub fn downgrade(&mut self, hand_type: usize) {
        if let Some(level) = self.levels.get_mut(hand_type) {
            *level = (*level).saturating_sub(1).max(1);
        }
    }

    /// 獲取等級加成（每級 +chips 和 +mult）
    pub fn bonus(&self, hand_type: usize) -> (i64, i64) {
        let level = self.get(hand_type);
        let extra_levels = level.saturating_sub(1) as i64;
        // 每升一級：+10 chips, +1 mult（簡化版）
        (extra_levels * 10, extra_levels)
    }
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tarot_indices() {
        for (i, tarot) in TarotId::all().iter().enumerate() {
            assert_eq!(tarot.to_index(), i);
            assert_eq!(TarotId::from_index(i), Some(*tarot));
        }
    }

    #[test]
    fn test_planet_indices() {
        for (i, planet) in PlanetId::all().iter().enumerate() {
            assert_eq!(planet.to_index(), i);
            assert_eq!(PlanetId::from_index(i), Some(*planet));
        }
    }

    #[test]
    fn test_spectral_indices() {
        for (i, spectral) in SpectralId::all().iter().enumerate() {
            assert_eq!(spectral.to_index(), i);
            assert_eq!(SpectralId::from_index(i), Some(*spectral));
        }
    }

    #[test]
    fn test_consumable_global_index() {
        // Test Tarot range
        for i in 0..TAROT_COUNT {
            let c = Consumable::from_global_index(i);
            assert!(matches!(c, Some(Consumable::Tarot(_))));
            assert_eq!(c.unwrap().to_global_index(), i);
        }

        // Test Planet range
        for i in TAROT_COUNT..(TAROT_COUNT + PLANET_COUNT) {
            let c = Consumable::from_global_index(i);
            assert!(matches!(c, Some(Consumable::Planet(_))));
            assert_eq!(c.unwrap().to_global_index(), i);
        }

        // Test Spectral range
        for i in (TAROT_COUNT + PLANET_COUNT)..CONSUMABLE_COUNT {
            let c = Consumable::from_global_index(i);
            assert!(matches!(c, Some(Consumable::Spectral(_))));
            assert_eq!(c.unwrap().to_global_index(), i);
        }
    }

    #[test]
    fn test_consumable_slots() {
        let mut slots = ConsumableSlots::new();
        assert!(!slots.is_full());
        assert_eq!(slots.remaining(), 2);

        let mut rng = StdRng::seed_from_u64(42);
        assert!(slots.add(Consumable::random_tarot(&mut rng)));
        assert!(slots.add(Consumable::random_planet(&mut rng)));
        assert!(slots.is_full());
        assert!(!slots.add(Consumable::random_spectral(&mut rng)));

        let used = slots.use_item(0);
        assert!(used.is_some());
        assert!(!slots.is_full());
    }

    #[test]
    fn test_hand_levels() {
        let mut levels = HandLevels::new();
        assert_eq!(levels.get(0), 1);
        assert_eq!(levels.bonus(0), (0, 0));

        levels.upgrade(0);
        assert_eq!(levels.get(0), 2);
        assert_eq!(levels.bonus(0), (10, 1));

        levels.upgrade_all();
        assert_eq!(levels.get(0), 3);
        assert_eq!(levels.get(5), 2);
    }

    #[test]
    fn test_planet_hand_type_mapping() {
        // Mercury -> Pair (index 1)
        assert_eq!(PlanetId::Mercury.hand_type_index(), 1);
        // Mars -> Four of a Kind (index 7)
        assert_eq!(PlanetId::Mars.hand_type_index(), 7);
        // Pluto -> High Card (index 0)
        assert_eq!(PlanetId::Pluto.hand_type_index(), 0);
    }
}
