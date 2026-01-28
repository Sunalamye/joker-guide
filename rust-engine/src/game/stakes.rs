//! Stake 難度系統
//!
//! 定義 8 種難度等級，每種都有不同的懲罰
//!
//! # 架構
//!
//! 使用聲明式 `STAKE_DEFS` 表定義所有 Stake 的元數據。

// ============================================================================
// Stake 定義系統
// ============================================================================

/// Stake 定義結構
#[derive(Clone, Copy)]
pub struct StakeDef {
    pub name: &'static str,
    pub score_multiplier: f32,
    pub discard_modifier: i32,
    pub hand_modifier: i32,
    pub hand_size_modifier: i32,
    pub small_blind_gives_reward: bool,
    pub booster_cost_modifier: i64,
    pub has_eternal_jokers: bool,
    pub has_perishable_jokers: bool,
}

/// Stake 定義表（順序與 Stake 枚舉值一致）
pub static STAKE_DEFS: [StakeDef; 8] = [
    // 0: White - 預設，無修正
    StakeDef { name: "White Stake", score_multiplier: 1.0, discard_modifier: 0, hand_modifier: 0, hand_size_modifier: 0, small_blind_gives_reward: true, booster_cost_modifier: 0, has_eternal_jokers: false, has_perishable_jokers: false },
    // 1: Red - Small Blind 無獎勵
    StakeDef { name: "Red Stake", score_multiplier: 1.0, discard_modifier: 0, hand_modifier: 0, hand_size_modifier: 0, small_blind_gives_reward: false, booster_cost_modifier: 0, has_eternal_jokers: false, has_perishable_jokers: false },
    // 2: Green - 基礎分數 +25%
    StakeDef { name: "Green Stake", score_multiplier: 1.25, discard_modifier: 0, hand_modifier: 0, hand_size_modifier: 0, small_blind_gives_reward: false, booster_cost_modifier: 0, has_eternal_jokers: false, has_perishable_jokers: false },
    // 3: Black - 永恆 Joker
    StakeDef { name: "Black Stake", score_multiplier: 1.25, discard_modifier: 0, hand_modifier: 0, hand_size_modifier: 0, small_blind_gives_reward: false, booster_cost_modifier: 0, has_eternal_jokers: true, has_perishable_jokers: false },
    // 4: Blue - -1 棄牌
    StakeDef { name: "Blue Stake", score_multiplier: 1.25, discard_modifier: -1, hand_modifier: 0, hand_size_modifier: 0, small_blind_gives_reward: false, booster_cost_modifier: 0, has_eternal_jokers: true, has_perishable_jokers: false },
    // 5: Purple - -1 手牌大小
    StakeDef { name: "Purple Stake", score_multiplier: 1.25, discard_modifier: -1, hand_modifier: 0, hand_size_modifier: -1, small_blind_gives_reward: false, booster_cost_modifier: 0, has_eternal_jokers: true, has_perishable_jokers: false },
    // 6: Orange - 補充包 +$1, 易腐 Joker
    StakeDef { name: "Orange Stake", score_multiplier: 1.25, discard_modifier: -1, hand_modifier: 0, hand_size_modifier: -1, small_blind_gives_reward: false, booster_cost_modifier: 1, has_eternal_jokers: true, has_perishable_jokers: true },
    // 7: Gold - -1 出牌
    StakeDef { name: "Gold Stake", score_multiplier: 1.25, discard_modifier: -1, hand_modifier: -1, hand_size_modifier: -1, small_blind_gives_reward: false, booster_cost_modifier: 1, has_eternal_jokers: true, has_perishable_jokers: true },
];

/// Stake 難度等級
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stake {
    #[default]
    White = 0, // 預設，無修正
    Red = 1,    // Small Blind 無獎勵
    Green = 2,  // 基礎分數要求 +25%
    Black = 3,  // 商店可能出現永恆 Joker（不能出售）
    Blue = 4,   // 每回合 -1 棄牌
    Purple = 5, // 起始手牌 -1
    Orange = 6, // 補充包費用 +$1
    Gold = 7,   // 每回合 -1 出牌
}

impl Stake {
    /// 所有 Stake 等級
    pub fn all() -> &'static [Stake] {
        &[
            Stake::White,
            Stake::Red,
            Stake::Green,
            Stake::Black,
            Stake::Blue,
            Stake::Purple,
            Stake::Orange,
            Stake::Gold,
        ]
    }

    /// Stake 名稱
    pub fn name(&self) -> &'static str {
        STAKE_DEFS[self.to_index()].name
    }

    /// 分數倍數（Green Stake +25%）
    pub fn score_multiplier(&self) -> f32 {
        STAKE_DEFS[self.to_index()].score_multiplier
    }

    /// 棄牌修正（Blue Stake 及以上 -1）
    pub fn discard_modifier(&self) -> i32 {
        STAKE_DEFS[self.to_index()].discard_modifier
    }

    /// 出牌修正（Gold Stake -1）
    pub fn hand_modifier(&self) -> i32 {
        STAKE_DEFS[self.to_index()].hand_modifier
    }

    /// 手牌大小修正（Purple Stake 及以上 -1）
    pub fn hand_size_modifier(&self) -> i32 {
        STAKE_DEFS[self.to_index()].hand_size_modifier
    }

    /// Small Blind 是否給獎勵（Red Stake 及以上不給）
    pub fn small_blind_gives_reward(&self) -> bool {
        STAKE_DEFS[self.to_index()].small_blind_gives_reward
    }

    /// 補充包價格修正（Orange Stake 及以上 +$1）
    pub fn booster_cost_modifier(&self) -> i64 {
        STAKE_DEFS[self.to_index()].booster_cost_modifier
    }

    /// 商店是否有永恆 Joker（Black Stake 及以上）
    pub fn has_eternal_jokers(&self) -> bool {
        STAKE_DEFS[self.to_index()].has_eternal_jokers
    }

    /// 商店是否有易腐 Joker（Orange Stake 及以上）
    pub fn has_perishable_jokers(&self) -> bool {
        STAKE_DEFS[self.to_index()].has_perishable_jokers
    }

    /// to_index 用於 observation
    pub fn to_index(&self) -> usize {
        *self as usize
    }

    /// 從索引創建 Stake
    pub fn from_index(index: usize) -> Option<Self> {
        Self::all().get(index).copied()
    }
}

/// Stake 配置（應用於遊戲狀態）
#[derive(Clone, Debug)]
pub struct StakeConfig {
    pub stake: Stake,
    pub score_multiplier: f32,
    pub discard_modifier: i32,
    pub hand_modifier: i32,
    pub hand_size_modifier: i32,
    pub small_blind_gives_reward: bool,
    pub has_eternal_jokers: bool,
}

impl StakeConfig {
    pub fn from_stake(stake: Stake) -> Self {
        Self {
            stake,
            score_multiplier: stake.score_multiplier(),
            discard_modifier: stake.discard_modifier(),
            hand_modifier: stake.hand_modifier(),
            hand_size_modifier: stake.hand_size_modifier(),
            small_blind_gives_reward: stake.small_blind_gives_reward(),
            has_eternal_jokers: stake.has_eternal_jokers(),
        }
    }
}

impl Default for StakeConfig {
    fn default() -> Self {
        Self::from_stake(Stake::White)
    }
}

/// Stake 總數
pub const STAKE_COUNT: usize = 8;

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_white_stake_default() {
        let config = StakeConfig::from_stake(Stake::White);
        assert_eq!(config.score_multiplier, 1.0);
        assert_eq!(config.discard_modifier, 0);
        assert_eq!(config.hand_modifier, 0);
        assert!(config.small_blind_gives_reward);
    }

    #[test]
    fn test_red_stake_no_small_reward() {
        let config = StakeConfig::from_stake(Stake::Red);
        assert!(!config.small_blind_gives_reward);
    }

    #[test]
    fn test_green_stake_score_multiplier() {
        let config = StakeConfig::from_stake(Stake::Green);
        assert_eq!(config.score_multiplier, 1.25);
    }

    #[test]
    fn test_gold_stake_all_penalties() {
        let config = StakeConfig::from_stake(Stake::Gold);
        assert_eq!(config.score_multiplier, 1.25);
        assert_eq!(config.discard_modifier, -1);
        assert_eq!(config.hand_modifier, -1);
        assert_eq!(config.hand_size_modifier, -1);
        assert!(config.has_eternal_jokers);
    }

    #[test]
    fn test_stake_indices() {
        for stake in Stake::all() {
            let idx = stake.to_index();
            assert!(idx < STAKE_COUNT);
            assert_eq!(Some(*stake), Stake::from_index(idx));
        }
    }
}
