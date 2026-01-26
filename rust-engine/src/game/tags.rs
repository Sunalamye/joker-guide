//! Tag 系統
//!
//! 跳過 Blind 時獲得的獎勵

use rand::prelude::*;
use rand::rngs::StdRng;

/// Tag 類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagId {
    // 經濟類
    UncommonTag,  // 免費獲得一張 Uncommon Joker
    RareTag,      // 免費獲得一張 Rare Joker
    NegativeTag,  // 下一個 Joker 變成 Negative
    FoilTag,      // 下一個 Joker 變成 Foil
    HolographicTag, // 下一個 Joker 變成 Holographic
    PolychromeTag, // 下一個 Joker 變成 Polychrome
    InvestmentTag, // +$25 at end of round
    VoucherTag,   // 免費獲得一張 Voucher
    BossTag,      // 重抽 Boss Blind
    StandardTag,  // 獲得免費 Standard Pack
    BuffoonTag,   // 獲得免費 Buffoon Pack
    MeteorTag,    // 獲得免費 Meteor Pack
    EtherealTag,  // 獲得免費 Ethereal Pack
    CelestialTag, // 獲得免費 Celestial Pack
    // 商店類
    CouponTag,    // 商店物品 50% off
    DoubleTag,    // 複製下一個選擇的 Tag
    JuggleTag,    // +3 手牌大小
    D6Tag,        // 免費 Reroll 整個商店
    TopUpTag,     // 建立 2 個常見消耗品
    SpeedTag,     // +$25 並跳過商店
    OrbitalTag,   // 升級一種牌型
    EconomyTag,   // +$10 (簡單經濟獎勵)
}

impl TagId {
    /// 獲取隨機 Tag（考慮機率權重）
    pub fn random(rng: &mut StdRng) -> Self {
        let tags = Self::all();
        *tags.choose(rng).unwrap_or(&TagId::EconomyTag)
    }

    /// 所有可用的 Tags
    pub fn all() -> &'static [TagId] {
        &[
            TagId::UncommonTag,
            TagId::RareTag,
            TagId::NegativeTag,
            TagId::FoilTag,
            TagId::HolographicTag,
            TagId::PolychromeTag,
            TagId::InvestmentTag,
            TagId::VoucherTag,
            TagId::BossTag,
            TagId::StandardTag,
            TagId::BuffoonTag,
            TagId::MeteorTag,
            TagId::EtherealTag,
            TagId::CelestialTag,
            TagId::CouponTag,
            TagId::DoubleTag,
            TagId::JuggleTag,
            TagId::D6Tag,
            TagId::TopUpTag,
            TagId::SpeedTag,
            TagId::OrbitalTag,
            TagId::EconomyTag,
        ]
    }

    /// 常見的 Tags（用於隨機生成）
    pub fn common() -> &'static [TagId] {
        &[
            TagId::EconomyTag,
            TagId::CouponTag,
            TagId::D6Tag,
            TagId::StandardTag,
            TagId::BuffoonTag,
            TagId::InvestmentTag,
        ]
    }

    /// Tag 的金幣即時獎勵
    pub fn immediate_money(&self) -> i64 {
        match self {
            TagId::EconomyTag => 10,
            TagId::SpeedTag => 25,
            TagId::InvestmentTag => 0, // 回合結束時給
            _ => 0,
        }
    }

    /// Tag 的 to_index 用於 observation
    pub fn to_index(&self) -> usize {
        match self {
            TagId::UncommonTag => 0,
            TagId::RareTag => 1,
            TagId::NegativeTag => 2,
            TagId::FoilTag => 3,
            TagId::HolographicTag => 4,
            TagId::PolychromeTag => 5,
            TagId::InvestmentTag => 6,
            TagId::VoucherTag => 7,
            TagId::BossTag => 8,
            TagId::StandardTag => 9,
            TagId::BuffoonTag => 10,
            TagId::MeteorTag => 11,
            TagId::EtherealTag => 12,
            TagId::CelestialTag => 13,
            TagId::CouponTag => 14,
            TagId::DoubleTag => 15,
            TagId::JuggleTag => 16,
            TagId::D6Tag => 17,
            TagId::TopUpTag => 18,
            TagId::SpeedTag => 19,
            TagId::OrbitalTag => 20,
            TagId::EconomyTag => 21,
        }
    }
}

/// Tag 實例（可能有額外狀態）
#[derive(Clone, Debug)]
pub struct Tag {
    pub id: TagId,
    pub used: bool,
}

impl Tag {
    pub fn new(id: TagId) -> Self {
        Self { id, used: false }
    }

    /// 使用 Tag 並返回金幣獎勵
    pub fn use_tag(&mut self) -> i64 {
        if self.used {
            return 0;
        }
        self.used = true;
        self.id.immediate_money()
    }
}

/// Tag 常量
pub const TAG_COUNT: usize = 22;

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_random() {
        let mut rng = StdRng::seed_from_u64(42);
        let tag = TagId::random(&mut rng);
        assert!(TagId::all().contains(&tag));
    }

    #[test]
    fn test_tag_use() {
        let mut tag = Tag::new(TagId::EconomyTag);
        assert_eq!(tag.use_tag(), 10);
        assert!(tag.used);
        assert_eq!(tag.use_tag(), 0); // 已使用
    }

    #[test]
    fn test_tag_indices() {
        for (i, tag) in TagId::all().iter().enumerate() {
            // 確保索引在範圍內
            assert!(tag.to_index() < TAG_COUNT);
        }
    }
}
