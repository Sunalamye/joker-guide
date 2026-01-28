//! Voucher 永久升級系統
//!
//! 每個商店最多 1 個 Voucher 可購買
//! 永久效果持續整局遊戲
//!
//! # 架構
//!
//! 使用聲明式 `VOUCHER_DEFS` 表定義所有 Voucher 的元數據，
//! 取代原有的多個 match 語句。

use rand::prelude::*;
use rand::rngs::StdRng;

/// Voucher 數量
pub const VOUCHER_COUNT: usize = 36;

// ============================================================================
// Voucher 定義系統
// ============================================================================

/// Voucher 定義結構
#[derive(Clone, Copy, Debug)]
pub struct VoucherDef {
    /// Voucher ID
    pub id: VoucherId,
    /// 顯示名稱
    pub name: &'static str,
    /// 購買價格
    pub cost: i64,
    /// 前置需求（升級版需要基礎版）
    pub prerequisite: Option<VoucherId>,
    /// 升級版本（基礎版可升級到）
    pub upgrade: Option<VoucherId>,
}

/// 獲取 Voucher 定義
pub fn get_voucher_def(index: usize) -> &'static VoucherDef {
    &VOUCHER_DEFS[index]
}

/// Voucher 定義表（順序與原有 to_index() 一致）
pub static VOUCHER_DEFS: [VoucherDef; VOUCHER_COUNT] = [
    // ========== 基礎 Voucher (0-15) ==========
    // 0
    VoucherDef { id: VoucherId::Overstock, name: "Overstock", cost: 10, prerequisite: None, upgrade: Some(VoucherId::OverstockPlus) },
    // 1
    VoucherDef { id: VoucherId::ClearanceSale, name: "Clearance Sale", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Liquidation) },
    // 2
    VoucherDef { id: VoucherId::Hone, name: "Hone", cost: 10, prerequisite: None, upgrade: Some(VoucherId::GlowUp) },
    // 3
    VoucherDef { id: VoucherId::RerollSurplus, name: "Reroll Surplus", cost: 10, prerequisite: None, upgrade: Some(VoucherId::RerollGlut) },
    // 4
    VoucherDef { id: VoucherId::CrystalBall, name: "Crystal Ball", cost: 10, prerequisite: None, upgrade: Some(VoucherId::OmenGlobe) },
    // 5
    VoucherDef { id: VoucherId::Telescope, name: "Telescope", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Nadir) },
    // 6
    VoucherDef { id: VoucherId::Grabber, name: "Grabber", cost: 10, prerequisite: None, upgrade: Some(VoucherId::GrabberPlus) },
    // 7
    VoucherDef { id: VoucherId::Wasteful, name: "Wasteful", cost: 10, prerequisite: None, upgrade: Some(VoucherId::WastefulPlus) },
    // 8
    VoucherDef { id: VoucherId::SeedMoney, name: "Seed Money", cost: 10, prerequisite: None, upgrade: Some(VoucherId::MoneyTree) },
    // 9
    VoucherDef { id: VoucherId::Blank, name: "Blank", cost: 10, prerequisite: None, upgrade: Some(VoucherId::BlankPlus) },
    // 10
    VoucherDef { id: VoucherId::PaintBrush, name: "Paint Brush", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Palette) },
    // 11
    VoucherDef { id: VoucherId::Tarot_Merchant, name: "Tarot Merchant", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Tarot_Tycoon) },
    // 12
    VoucherDef { id: VoucherId::Planet_Merchant, name: "Planet Merchant", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Planet_Tycoon) },
    // 13
    VoucherDef { id: VoucherId::Magic_Trick, name: "Magic Trick", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Illusion) },
    // 14
    VoucherDef { id: VoucherId::Antimatter, name: "Antimatter", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Antimatter_Plus) },
    // 15
    VoucherDef { id: VoucherId::Hieroglyph, name: "Hieroglyph", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Petroglyph) },
    // ========== 升級 Voucher (16-31) ==========
    // 16
    VoucherDef { id: VoucherId::OverstockPlus, name: "Overstock Plus", cost: 10, prerequisite: Some(VoucherId::Overstock), upgrade: None },
    // 17
    VoucherDef { id: VoucherId::Liquidation, name: "Liquidation", cost: 10, prerequisite: Some(VoucherId::ClearanceSale), upgrade: None },
    // 18
    VoucherDef { id: VoucherId::GlowUp, name: "Glow Up", cost: 10, prerequisite: Some(VoucherId::Hone), upgrade: None },
    // 19
    VoucherDef { id: VoucherId::RerollGlut, name: "Reroll Glut", cost: 10, prerequisite: Some(VoucherId::RerollSurplus), upgrade: None },
    // 20
    VoucherDef { id: VoucherId::OmenGlobe, name: "Omen Globe", cost: 10, prerequisite: Some(VoucherId::CrystalBall), upgrade: None },
    // 21
    VoucherDef { id: VoucherId::Nadir, name: "Nadir", cost: 10, prerequisite: Some(VoucherId::Telescope), upgrade: None },
    // 22
    VoucherDef { id: VoucherId::GrabberPlus, name: "Grabber Plus", cost: 10, prerequisite: Some(VoucherId::Grabber), upgrade: None },
    // 23
    VoucherDef { id: VoucherId::WastefulPlus, name: "Wasteful Plus", cost: 10, prerequisite: Some(VoucherId::Wasteful), upgrade: None },
    // 24
    VoucherDef { id: VoucherId::MoneyTree, name: "Money Tree", cost: 10, prerequisite: Some(VoucherId::SeedMoney), upgrade: None },
    // 25
    VoucherDef { id: VoucherId::Palette, name: "Palette", cost: 10, prerequisite: Some(VoucherId::PaintBrush), upgrade: None },
    // 26
    VoucherDef { id: VoucherId::Tarot_Tycoon, name: "Tarot Tycoon", cost: 10, prerequisite: Some(VoucherId::Tarot_Merchant), upgrade: None },
    // 27
    VoucherDef { id: VoucherId::Planet_Tycoon, name: "Planet Tycoon", cost: 10, prerequisite: Some(VoucherId::Planet_Merchant), upgrade: None },
    // 28
    VoucherDef { id: VoucherId::Illusion, name: "Illusion", cost: 10, prerequisite: Some(VoucherId::Magic_Trick), upgrade: None },
    // 29
    VoucherDef { id: VoucherId::Antimatter_Plus, name: "Antimatter Plus", cost: 10, prerequisite: Some(VoucherId::Antimatter), upgrade: None },
    // 30
    VoucherDef { id: VoucherId::Petroglyph, name: "Petroglyph", cost: 10, prerequisite: Some(VoucherId::Hieroglyph), upgrade: None },
    // 31
    VoucherDef { id: VoucherId::BlankPlus, name: "Blank Plus", cost: 10, prerequisite: Some(VoucherId::Blank), upgrade: None },
    // ========== 後期新增 Voucher (32-35) ==========
    // 32
    VoucherDef { id: VoucherId::Observatory, name: "Observatory", cost: 10, prerequisite: None, upgrade: Some(VoucherId::ObservatoryPlus) },
    // 33
    VoucherDef { id: VoucherId::ObservatoryPlus, name: "Observatory Plus", cost: 10, prerequisite: Some(VoucherId::Observatory), upgrade: None },
    // 34
    VoucherDef { id: VoucherId::DirectorsCut, name: "Director's Cut", cost: 10, prerequisite: None, upgrade: Some(VoucherId::Retcon) },
    // 35
    VoucherDef { id: VoucherId::Retcon, name: "Retcon", cost: 10, prerequisite: Some(VoucherId::DirectorsCut), upgrade: None },
];

/// Voucher ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum VoucherId {
    // ========== 基礎 Voucher ==========
    /// 商店 +1 Joker 槽
    Overstock,
    /// 所有物品 -25%
    ClearanceSale,
    /// Foil/Holo/Poly 出現率 x2
    Hone,
    /// Reroll 費用 -$2
    RerollSurplus,
    /// 消耗品槽 +1
    CrystalBall,
    /// 天體牌出現率 x2
    Telescope,
    /// +1 hand per round
    Grabber,
    /// +1 discard per round
    Wasteful,
    /// 利息計算上限 +$25
    SeedMoney,
    /// 不做任何事
    Blank,
    /// +$3 售價給所有 Joker
    PaintBrush,
    /// 抽牌時多抽 1 張
    Tarot_Merchant,
    /// Planet 出現率提升
    Planet_Merchant,
    /// 商店多 1 個卡槽
    Magic_Trick,
    /// 抽塔羅牌時可能出現負面牌
    Antimatter,
    /// -1 Ante 需求, -1 hand per round
    Hieroglyph,
    /// 計分牌符合 Planet 時 X1.5 Mult
    Observatory,
    /// Boss Blind reroll 一次 ($10)
    DirectorsCut,

    // ========== 升級 Voucher（需要先買基礎版）==========
    /// 再 +1 Joker 槽（需要 Overstock）
    OverstockPlus,
    /// 所有物品 -50%（需要 ClearanceSale）
    Liquidation,
    /// Foil/Holo/Poly 出現率 x4（需要 Hone）
    GlowUp,
    /// Reroll 費用 -$4（需要 RerollSurplus）
    RerollGlut,
    /// 消耗品槽 +1（需要 CrystalBall）
    OmenGlobe,
    /// 商店總有 Planet（需要 Telescope）
    Nadir,
    /// 再 +1 hand（需要 Grabber）
    GrabberPlus,
    /// 再 +1 discard（需要 Wasteful）
    WastefulPlus,
    /// 利息計算上限 +$50（需要 SeedMoney）
    MoneyTree,
    /// +$6 售價給所有 Joker（需要 PaintBrush）
    Palette,
    /// 抽牌時多抽 2 張（需要 Tarot_Merchant）
    Tarot_Tycoon,
    /// Planet 出現率大幅提升（需要 Planet_Merchant）
    Planet_Tycoon,
    /// 商店多 2 個卡槽（需要 Magic_Trick）
    Illusion,
    /// 所有 Joker 可以是負面（需要 Antimatter）
    Antimatter_Plus,
    /// -1 Ante 需求, -1 discard per round（需要 Hieroglyph）
    Petroglyph,
    /// 空白升級（需要 Blank）
    BlankPlus,
    /// 計分牌符合 Planet 時 X2 Mult (升級版)
    ObservatoryPlus,
    /// 無限免費 Boss Blind reroll（需要 DirectorsCut）
    Retcon,
}

impl VoucherId {
    /// 所有基礎 Voucher
    pub fn base_vouchers() -> &'static [VoucherId] {
        &[
            VoucherId::Overstock,
            VoucherId::ClearanceSale,
            VoucherId::Hone,
            VoucherId::RerollSurplus,
            VoucherId::CrystalBall,
            VoucherId::Telescope,
            VoucherId::Grabber,
            VoucherId::Wasteful,
            VoucherId::SeedMoney,
            VoucherId::Blank,
            VoucherId::PaintBrush,
            VoucherId::Tarot_Merchant,
            VoucherId::Planet_Merchant,
            VoucherId::Magic_Trick,
            VoucherId::Antimatter,
            VoucherId::Hieroglyph,
            VoucherId::Observatory,
            VoucherId::DirectorsCut,
        ]
    }

    /// 獲取此 Voucher 的前置需求（使用 VOUCHER_DEFS 表查詢）
    pub fn prerequisite(&self) -> Option<VoucherId> {
        get_voucher_def(self.to_index()).prerequisite
    }

    /// 獲取此 Voucher 的升級版本（使用 VOUCHER_DEFS 表查詢）
    pub fn upgrade(&self) -> Option<VoucherId> {
        get_voucher_def(self.to_index()).upgrade
    }

    /// Voucher 名稱（使用 VOUCHER_DEFS 表查詢）
    pub fn name(&self) -> &'static str {
        get_voucher_def(self.to_index()).name
    }

    /// 購買價格（使用 VOUCHER_DEFS 表查詢）
    pub fn cost(&self) -> i64 {
        get_voucher_def(self.to_index()).cost
    }

    /// 轉換為索引
    pub fn to_index(&self) -> usize {
        match self {
            VoucherId::Overstock => 0,
            VoucherId::ClearanceSale => 1,
            VoucherId::Hone => 2,
            VoucherId::RerollSurplus => 3,
            VoucherId::CrystalBall => 4,
            VoucherId::Telescope => 5,
            VoucherId::Grabber => 6,
            VoucherId::Wasteful => 7,
            VoucherId::SeedMoney => 8,
            VoucherId::Blank => 9,
            VoucherId::PaintBrush => 10,
            VoucherId::Tarot_Merchant => 11,
            VoucherId::Planet_Merchant => 12,
            VoucherId::Magic_Trick => 13,
            VoucherId::Antimatter => 14,
            VoucherId::Hieroglyph => 15,
            VoucherId::OverstockPlus => 16,
            VoucherId::Liquidation => 17,
            VoucherId::GlowUp => 18,
            VoucherId::RerollGlut => 19,
            VoucherId::OmenGlobe => 20,
            VoucherId::Nadir => 21,
            VoucherId::GrabberPlus => 22,
            VoucherId::WastefulPlus => 23,
            VoucherId::MoneyTree => 24,
            VoucherId::Palette => 25,
            VoucherId::Tarot_Tycoon => 26,
            VoucherId::Planet_Tycoon => 27,
            VoucherId::Illusion => 28,
            VoucherId::Antimatter_Plus => 29,
            VoucherId::Petroglyph => 30,
            VoucherId::BlankPlus => 31,
            VoucherId::Observatory => 32,
            VoucherId::ObservatoryPlus => 33,
            VoucherId::DirectorsCut => 34,
            VoucherId::Retcon => 35,
        }
    }

    /// 從索引創建（使用 VOUCHER_DEFS 表查詢）
    pub fn from_index(index: usize) -> Option<Self> {
        VOUCHER_DEFS.get(index).map(|def| def.id)
    }

    /// 隨機選擇一個可用的 Voucher（考慮已購買的 Voucher）
    pub fn random_available(rng: &mut StdRng, owned: &[VoucherId]) -> Option<VoucherId> {
        let mut available = Vec::new();

        // 檢查基礎 Voucher
        for &voucher in Self::base_vouchers() {
            if !owned.contains(&voucher) {
                available.push(voucher);
            }
        }

        // 檢查升級 Voucher（需要擁有前置）
        for &base in Self::base_vouchers() {
            if owned.contains(&base) {
                if let Some(upgrade) = base.upgrade() {
                    if !owned.contains(&upgrade) {
                        available.push(upgrade);
                    }
                }
            }
        }

        available.choose(rng).copied()
    }
}

/// Voucher 效果追蹤器
#[derive(Clone, Debug, Default)]
pub struct VoucherEffects {
    /// 已購買的 Voucher 列表
    pub owned: Vec<VoucherId>,

    // ========== 累計效果 ==========
    /// 額外 Joker 商店槽位
    pub extra_shop_joker_slots: i32,
    /// 商品折扣比例 (0.0 - 1.0)
    pub discount_rate: f32,
    /// 版本出現率倍數
    pub edition_rate_mult: f32,
    /// Reroll 費用減免
    pub reroll_discount: i64,
    /// 額外消耗品槽位
    pub extra_consumable_slots: i32,
    /// 天體牌出現率倍數
    pub planet_rate_mult: f32,
    /// 每回合額外出牌數
    pub extra_hands: i32,
    /// 每回合額外棄牌數
    pub extra_discards: i32,
    /// 利息上限增加
    pub interest_cap_bonus: i64,
    /// Joker 售價加成
    pub joker_sell_bonus: i64,
    /// 抽塔羅牌額外數量
    pub extra_tarot_draw: i32,
    /// 額外商店卡槽
    pub extra_shop_slots: i32,
    /// 額外 Joker 槽位 (Blank/Antimatter)
    pub joker_slot_bonus: i32,
    /// Ante 減免 (Hieroglyph/Petroglyph)
    pub ante_reduction: i32,
    /// Observatory 計分牌 Planet 對應牌型 X Mult 倍數
    pub observatory_x_mult: f32,
    /// Boss Blind reroll 可用次數 (DirectorsCut: 1，Retcon: 無限)
    pub boss_rerolls_available: i32,
    /// Boss Blind reroll 是否免費 (Retcon)
    pub free_boss_reroll: bool,
}

impl VoucherEffects {
    pub fn new() -> Self {
        Self {
            owned: Vec::new(),
            extra_shop_joker_slots: 0,
            discount_rate: 0.0,
            edition_rate_mult: 1.0,
            reroll_discount: 0,
            extra_consumable_slots: 0,
            planet_rate_mult: 1.0,
            extra_hands: 0,
            extra_discards: 0,
            interest_cap_bonus: 0,
            joker_sell_bonus: 0,
            extra_tarot_draw: 0,
            extra_shop_slots: 0,
            joker_slot_bonus: 0,
            ante_reduction: 0,
            observatory_x_mult: 1.0,
            boss_rerolls_available: 0,
            free_boss_reroll: false,
        }
    }

    /// 購買 Voucher
    pub fn buy(&mut self, voucher: VoucherId) -> bool {
        // 檢查是否已擁有
        if self.owned.contains(&voucher) {
            return false;
        }

        // 檢查前置需求
        if let Some(prereq) = voucher.prerequisite() {
            if !self.owned.contains(&prereq) {
                return false;
            }
        }

        // 應用效果
        self.apply_effect(voucher);
        self.owned.push(voucher);
        true
    }

    /// 應用 Voucher 效果
    fn apply_effect(&mut self, voucher: VoucherId) {
        match voucher {
            VoucherId::Overstock => self.extra_shop_joker_slots += 1,
            VoucherId::OverstockPlus => self.extra_shop_joker_slots += 1,
            VoucherId::ClearanceSale => self.discount_rate += 0.25,
            VoucherId::Liquidation => self.discount_rate += 0.25,
            VoucherId::Hone => self.edition_rate_mult *= 2.0,
            VoucherId::GlowUp => self.edition_rate_mult *= 2.0,
            VoucherId::RerollSurplus => self.reroll_discount += 2,
            VoucherId::RerollGlut => self.reroll_discount += 2,
            VoucherId::CrystalBall => self.extra_consumable_slots += 1,
            VoucherId::OmenGlobe => self.extra_consumable_slots += 1,
            VoucherId::Telescope => self.planet_rate_mult *= 2.0,
            VoucherId::Nadir => self.planet_rate_mult *= 2.0, // 簡化：提高出現率
            VoucherId::Grabber => self.extra_hands += 1,
            VoucherId::GrabberPlus => self.extra_hands += 1,
            VoucherId::Wasteful => self.extra_discards += 1,
            VoucherId::WastefulPlus => self.extra_discards += 1,
            VoucherId::SeedMoney => self.interest_cap_bonus += 25,
            VoucherId::MoneyTree => self.interest_cap_bonus += 25,
            VoucherId::PaintBrush => self.joker_sell_bonus += 3,
            VoucherId::Palette => self.joker_sell_bonus += 3,
            VoucherId::Tarot_Merchant => self.extra_tarot_draw += 1,
            VoucherId::Tarot_Tycoon => self.extra_tarot_draw += 1,
            VoucherId::Magic_Trick => self.extra_shop_slots += 1,
            VoucherId::Illusion => self.extra_shop_slots += 1,
            VoucherId::Blank => self.joker_slot_bonus += 1,
            VoucherId::BlankPlus => self.joker_slot_bonus += 1,
            VoucherId::Antimatter => self.joker_slot_bonus += 1,
            VoucherId::Antimatter_Plus => self.joker_slot_bonus += 1,
            // Hieroglyph: -1 Ante, -1 hand per round
            VoucherId::Hieroglyph => {
                self.ante_reduction += 1;
                self.extra_hands -= 1;
            }
            // Petroglyph: -1 Ante, -1 discard per round
            VoucherId::Petroglyph => {
                self.ante_reduction += 1;
                self.extra_discards -= 1;
            }
            // Observatory: 計分牌 Planet 對應牌型時 X1.5 Mult
            VoucherId::Observatory => {
                self.observatory_x_mult = 1.5;
            }
            // ObservatoryPlus: 計分牌 Planet 對應牌型時 X2 Mult
            VoucherId::ObservatoryPlus => {
                self.observatory_x_mult = 2.0;
            }
            // DirectorsCut: 可以 reroll Boss Blind 一次 ($10)
            VoucherId::DirectorsCut => {
                self.boss_rerolls_available = 1;
            }
            // Retcon: 無限免費 Boss Blind reroll
            VoucherId::Retcon => {
                self.boss_rerolls_available = i32::MAX;
                self.free_boss_reroll = true;
            }
            // 其他 Voucher 的效果較為複雜，暫時不實作
            _ => {}
        }
    }

    /// 計算折扣後價格
    pub fn apply_discount(&self, base_price: i64) -> i64 {
        let discounted = base_price as f32 * (1.0 - self.discount_rate);
        discounted.ceil() as i64
    }

    /// 計算實際 reroll 費用
    pub fn actual_reroll_cost(&self, base_cost: i64) -> i64 {
        (base_cost - self.reroll_discount).max(0)
    }

    /// 檢查是否擁有某個 Voucher
    pub fn has(&self, voucher: VoucherId) -> bool {
        self.owned.contains(&voucher)
    }

    /// 直接授予 Voucher（不檢查先決條件，用於初始化）
    pub fn grant(&mut self, voucher: VoucherId) {
        if !self.owned.contains(&voucher) {
            self.apply_effect(voucher);
            self.owned.push(voucher);
        }
    }
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voucher_effects_new() {
        let effects = VoucherEffects::new();
        assert_eq!(effects.extra_hands, 0);
        assert_eq!(effects.discount_rate, 0.0);
        assert!(effects.owned.is_empty());
    }

    #[test]
    fn test_buy_base_voucher() {
        let mut effects = VoucherEffects::new();
        assert!(effects.buy(VoucherId::Grabber));
        assert_eq!(effects.extra_hands, 1);
        assert!(effects.has(VoucherId::Grabber));
    }

    #[test]
    fn test_buy_upgrade_requires_base() {
        let mut effects = VoucherEffects::new();
        // 嘗試購買升級版本（沒有基礎版）
        assert!(!effects.buy(VoucherId::GrabberPlus));
        assert_eq!(effects.extra_hands, 0);

        // 先買基礎版
        assert!(effects.buy(VoucherId::Grabber));
        // 再買升級版
        assert!(effects.buy(VoucherId::GrabberPlus));
        assert_eq!(effects.extra_hands, 2);
    }

    #[test]
    fn test_discount_calculation() {
        let mut effects = VoucherEffects::new();
        effects.buy(VoucherId::ClearanceSale);
        assert_eq!(effects.apply_discount(100), 75);

        effects.buy(VoucherId::Liquidation);
        assert_eq!(effects.apply_discount(100), 50);
    }

    #[test]
    fn test_voucher_indices() {
        for i in 0..VOUCHER_COUNT {
            let voucher = VoucherId::from_index(i);
            assert!(voucher.is_some());
            assert_eq!(voucher.unwrap().to_index(), i);
        }
    }

    #[test]
    fn test_random_available_voucher() {
        let mut rng = StdRng::seed_from_u64(42);
        let owned = vec![];
        let voucher = VoucherId::random_available(&mut rng, &owned);
        assert!(voucher.is_some());
        // 沒有擁有任何 voucher，應該只能選到基礎版
        assert!(voucher.unwrap().prerequisite().is_none());
    }

    #[test]
    fn test_random_available_with_base_owned() {
        let mut rng = StdRng::seed_from_u64(42);
        let owned = vec![VoucherId::Grabber];

        // 多次嘗試，應該能選到 GrabberPlus
        let mut found_upgrade = false;
        for _ in 0..100 {
            if let Some(v) = VoucherId::random_available(&mut rng, &owned) {
                if v == VoucherId::GrabberPlus {
                    found_upgrade = true;
                    break;
                }
            }
        }
        assert!(found_upgrade);
    }
}
