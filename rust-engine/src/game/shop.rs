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
    pub fn refresh(&mut self, rng: &mut StdRng, item_count: usize) {
        self.items.clear();

        let available_jokers = JokerId::all_available();
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
