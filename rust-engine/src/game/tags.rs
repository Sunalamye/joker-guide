//! Tag 系統
//!
//! 跳過 Blind 時獲得的獎勵
//!
//! # 架構
//!
//! 使用聲明式 `TAG_DEFS` 表定義所有 Tag 的元數據。

use rand::prelude::*;
use rand::rngs::StdRng;

// ============================================================================
// Tag 定義系統
// ============================================================================

/// Tag 定義結構
#[derive(Clone, Copy)]
pub struct TagDef {
    pub immediate_money: i64,
    pub gives_free_pack: bool,
    pub doubles_next_tag: bool,
}

/// Tag 定義表（順序與 TagId 枚舉一致）
pub static TAG_DEFS: [TagDef; 25] = [
    // 0: UncommonTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 1: RareTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 2: NegativeTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 3: FoilTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 4: HolographicTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 5: PolychromeTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 6: InvestmentTag - gives money at end of round, not immediate
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 7: VoucherTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 8: BossTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 9: StandardTag - free Standard Pack
    TagDef { immediate_money: 0, gives_free_pack: true, doubles_next_tag: false },
    // 10: BuffoonTag - free Buffoon Pack
    TagDef { immediate_money: 0, gives_free_pack: true, doubles_next_tag: false },
    // 11: MeteorTag - free Meteor Pack
    TagDef { immediate_money: 0, gives_free_pack: true, doubles_next_tag: false },
    // 12: EtherealTag - free Ethereal Pack
    TagDef { immediate_money: 0, gives_free_pack: true, doubles_next_tag: false },
    // 13: CelestialTag - free Celestial Pack
    TagDef { immediate_money: 0, gives_free_pack: true, doubles_next_tag: false },
    // 14: CouponTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 15: DoubleTag - doubles next tag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: true },
    // 16: JuggleTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 17: D6Tag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 18: TopUpTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 19: SpeedTag - +$25
    TagDef { immediate_money: 25, gives_free_pack: false, doubles_next_tag: false },
    // 20: OrbitalTag
    TagDef { immediate_money: 0, gives_free_pack: false, doubles_next_tag: false },
    // 21: EconomyTag - +$10
    TagDef { immediate_money: 10, gives_free_pack: false, doubles_next_tag: false },
    // 22: HandyTag - $1 per hand (base: 4 hands)
    TagDef { immediate_money: 4, gives_free_pack: false, doubles_next_tag: false },
    // 23: GarbageTag - $1 per discard (base: 3 discards)
    TagDef { immediate_money: 3, gives_free_pack: false, doubles_next_tag: false },
    // 24: CharmTag - free Mega Arcana Pack
    TagDef { immediate_money: 0, gives_free_pack: true, doubles_next_tag: false },
];

/// Tag 類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagId {
    // 經濟類
    UncommonTag,    // 免費獲得一張 Uncommon Joker
    RareTag,        // 免費獲得一張 Rare Joker
    NegativeTag,    // 下一個 Joker 變成 Negative
    FoilTag,        // 下一個 Joker 變成 Foil
    HolographicTag, // 下一個 Joker 變成 Holographic
    PolychromeTag,  // 下一個 Joker 變成 Polychrome
    InvestmentTag,  // +$25 at end of round
    VoucherTag,     // 免費獲得一張 Voucher
    BossTag,        // 重抽 Boss Blind
    StandardTag,    // 獲得免費 Standard Pack
    BuffoonTag,     // 獲得免費 Buffoon Pack
    MeteorTag,      // 獲得免費 Meteor Pack
    EtherealTag,    // 獲得免費 Ethereal Pack
    CelestialTag,   // 獲得免費 Celestial Pack
    // 商店類
    CouponTag,  // 商店物品 50% off
    DoubleTag,  // 複製下一個選擇的 Tag
    JuggleTag,  // +3 手牌大小
    D6Tag,      // 免費 Reroll 整個商店
    TopUpTag,   // 建立 2 個常見消耗品
    SpeedTag,   // +$25 並跳過商店
    OrbitalTag, // 升級一種牌型
    EconomyTag, // +$10 (簡單經濟獎勵)
    // 動態獎勵類
    HandyTag,   // $1 per hand played (跳過時根據剩餘出牌次數給錢)
    GarbageTag, // $1 per unused discard (跳過時根據剩餘棄牌次數給錢)
    // Arcana 卡包類
    CharmTag, // 獲得免費 Mega Arcana Pack
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
            TagId::HandyTag,
            TagId::GarbageTag,
            TagId::CharmTag,
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
            TagId::HandyTag,
            TagId::GarbageTag,
        ]
    }

    /// Tag 的金幣即時獎勵
    pub fn immediate_money(&self) -> i64 {
        TAG_DEFS[self.to_index()].immediate_money
    }

    /// Tag 的 to_index 用於 observation
    pub fn to_index(&self) -> usize {
        Self::all().iter().position(|t| t == self).unwrap_or(0)
    }

    /// 是否給予免費卡包
    pub fn gives_free_pack(&self) -> bool {
        TAG_DEFS[self.to_index()].gives_free_pack
    }

    /// 是否複製下一個 Tag (DoubleTag)
    pub fn doubles_next_tag(&self) -> bool {
        TAG_DEFS[self.to_index()].doubles_next_tag
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
pub const TAG_COUNT: usize = 25;

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
