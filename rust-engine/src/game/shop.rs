//! 商店系統
//!
//! 處理商店 Joker 生成、購買和 Reroll

use rand::prelude::*;
use rand::rngs::StdRng;

use super::joker::{JokerId, JokerSlot};

/// 商店物品
#[derive(Clone, Debug)]
pub struct ShopItem {
    pub joker: JokerSlot,
    pub cost: i64,
}

impl ShopItem {
    pub fn new(id: JokerId, cost: i64) -> Self {
        Self {
            joker: JokerSlot::new(id).with_sell_value(cost / 2),
            cost,
        }
    }
}

/// 商店
#[derive(Clone, Debug)]
pub struct Shop {
    pub items: Vec<ShopItem>,
    pub reroll_cost: i64,
    pub reroll_count: i32,
}

impl Shop {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            reroll_cost: 5,
            reroll_count: 0,
        }
    }

    /// 刷新商店物品
    ///
    /// Only generates Jokers that have implemented game effects (scoring, trigger,
    /// or rule modifier). No-op Jokers are excluded to avoid wasting slots.
    pub fn refresh(&mut self, rng: &mut StdRng, item_count: usize) {
        self.items.clear();

        let available_jokers = JokerId::all_shop_eligible();
        for _ in 0..item_count {
            if let Some(&id) = available_jokers.choose(rng) {
                let base_cost = id.base_cost();
                // 根據稀有度調整價格
                let cost = match id.rarity() {
                    1 => base_cost + rng.gen_range(0..=2),
                    2 => base_cost + rng.gen_range(1..=3),
                    3 => base_cost + rng.gen_range(2..=4),
                    _ => base_cost,
                };
                self.items.push(ShopItem::new(id, cost));
            }
        }
    }

    /// Reroll 商店（需要支付費用）
    pub fn reroll(&mut self, rng: &mut StdRng, item_count: usize) -> i64 {
        let cost = self.current_reroll_cost();
        self.reroll_count += 1;
        self.refresh(rng, item_count);
        cost
    }

    /// 獲取當前 reroll 費用
    pub fn current_reroll_cost(&self) -> i64 {
        // 每次 reroll 費用增加 1
        self.reroll_cost + self.reroll_count as i64
    }

    /// 購買指定索引的物品
    pub fn buy(&mut self, index: usize) -> Option<ShopItem> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    /// 重置 reroll 計數（新回合開始時）
    pub fn reset_reroll(&mut self) {
        self.reroll_count = 0;
    }
}

impl Default for Shop {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// v10.0: 商店品質評估系統
// ============================================================================

/// 協同群組定義（與 Python reward.py 保持一致）
const SYNERGY_GROUPS: &[&[u8]] = &[
    &[1, 57, 85],                   // diamond_synergy
    &[5, 10, 111, 6, 11, 112, 113], // pair_power
    &[8, 13, 114, 29, 131],         // straight_masters
    &[9, 14, 115, 30],              // flush_kings
    &[97, 120, 129, 23, 64],        // scaling_xmult
    &[45, 46, 47, 48, 88],          // economy
    &[58, 59, 79, 138],             // face_card
    &[61, 62, 63, 53],              // retrigger
    &[68, 118],                     // boss_killer
];

/// 計算商店品質分數
///
/// 評估商店中 Joker 的整體價值，用於 Reroll 和 Skip 決策。
///
/// 評分因素：
/// - 稀有度 (40%): Common=0.2, Uncommon=0.5, Rare=0.8, Legendary=1.0
/// - 協同效果 (30%): 與已擁有 Joker 的協同關係
/// - 成本效益 (20%): 價格合理性（相對於基礎價格）
/// - 多樣性 (10%): 不同稀有度的 Joker 多樣性
///
/// Returns: 0.0 (差) ~ 1.0 (優)
pub fn calculate_shop_quality(shop: &Shop, owned_jokers: &[JokerSlot]) -> f32 {
    if shop.items.is_empty() {
        return 0.0;
    }

    let mut total_score = 0.0;
    let owned_ids: Vec<u8> = owned_jokers.iter().map(|j| j.id as u8).collect();

    for item in &shop.items {
        let joker_id = item.joker.id as u8;
        let rarity = item.joker.id.rarity();
        let base_cost = item.joker.id.base_cost();

        // 1. 稀有度分數 (40%)
        let rarity_score = match rarity {
            1 => 0.2,  // Common
            2 => 0.5,  // Uncommon
            3 => 0.8,  // Rare
            4 => 1.0,  // Legendary
            _ => 0.3,
        };

        // 2. 協同分數 (30%)
        let synergy_score = calculate_synergy_score(joker_id, &owned_ids);

        // 3. 成本效益分數 (20%)
        // 價格低於基礎價格 = 高分，高於 = 低分
        let cost_ratio = item.cost as f32 / (base_cost as f32).max(1.0);
        let cost_score = (2.0 - cost_ratio).clamp(0.0, 1.0);

        // 4. 特殊加成：xMult Joker 和 Boss Killer 額外加分
        let special_bonus = if is_high_value_joker(joker_id) { 0.2 } else { 0.0 };

        // 加權平均
        let item_score = rarity_score * 0.4
            + synergy_score * 0.3
            + cost_score * 0.2
            + special_bonus * 0.1;

        total_score += item_score;
    }

    // 取平均並正規化
    (total_score / shop.items.len() as f32).clamp(0.0, 1.0)
}

/// 計算單個 Joker 與已擁有 Joker 的協同分數
fn calculate_synergy_score(joker_id: u8, owned_ids: &[u8]) -> f32 {
    if owned_ids.is_empty() {
        return 0.0;
    }

    let mut synergy_count = 0;

    for group in SYNERGY_GROUPS {
        let shop_in_group = group.contains(&joker_id);
        if !shop_in_group {
            continue;
        }

        // 計算已擁有的 Joker 中有多少在同一群組
        let owned_in_group = owned_ids.iter().filter(|id| group.contains(id)).count();
        if owned_in_group > 0 {
            synergy_count += owned_in_group;
        }
    }

    // 每個協同匹配 +0.25，最高 1.0
    (synergy_count as f32 * 0.25).min(1.0)
}

/// 判斷是否為高價值 Joker（xMult, Boss Killer, 強力經濟）
fn is_high_value_joker(joker_id: u8) -> bool {
    // xMult Joker
    const XMULT_JOKERS: &[u8] = &[97, 120, 129, 23, 64, 15, 16, 17];
    // Boss Killer
    const BOSS_KILLERS: &[u8] = &[68, 118];
    // 強力經濟
    const ECONOMY_JOKERS: &[u8] = &[45, 46, 47, 48];

    XMULT_JOKERS.contains(&joker_id)
        || BOSS_KILLERS.contains(&joker_id)
        || ECONOMY_JOKERS.contains(&joker_id)
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shop_refresh() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut shop = Shop::new();
        shop.refresh(&mut rng, 2);

        assert_eq!(shop.items.len(), 2);
        assert!(shop.items[0].cost > 0);
        assert!(shop.items[1].cost > 0);
    }

    #[test]
    fn test_shop_buy() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut shop = Shop::new();
        shop.refresh(&mut rng, 2);

        let item = shop.buy(0);
        assert!(item.is_some());
        assert_eq!(shop.items.len(), 1);
    }

    #[test]
    fn test_shop_only_generates_effective_jokers() {
        use crate::game::joker_def::has_implemented_effect;

        let mut rng = StdRng::seed_from_u64(123);
        let mut shop = Shop::new();

        // Generate many items to sample the pool
        for _ in 0..50 {
            shop.refresh(&mut rng, 2);
            for item in &shop.items {
                let idx = item.joker.id.to_index();
                assert!(
                    has_implemented_effect(idx),
                    "Shop generated no-op Joker: {:?} (index {})",
                    item.joker.id,
                    idx
                );
            }
        }
    }

    #[test]
    fn test_shop_eligible_fewer_than_all() {
        let all = JokerId::all_available();
        let eligible = JokerId::all_shop_eligible();

        assert!(eligible.len() < all.len(),
            "Shop eligible ({}) should be fewer than all available ({})",
            eligible.len(), all.len());
        assert!(eligible.len() > 100,
            "Should have >100 shop eligible Jokers, got {}", eligible.len());
    }

    #[test]
    fn test_shop_reroll_cost() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut shop = Shop::new();
        shop.refresh(&mut rng, 2);

        assert_eq!(shop.current_reroll_cost(), 5);
        shop.reroll(&mut rng, 2);
        assert_eq!(shop.current_reroll_cost(), 6);
        shop.reroll(&mut rng, 2);
        assert_eq!(shop.current_reroll_cost(), 7);

        shop.reset_reroll();
        assert_eq!(shop.current_reroll_cost(), 5);
    }
}
