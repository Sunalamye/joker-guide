//! Stake 難度系統
//!
//! 定義 8 種難度等級，每種都有不同的懲罰

/// Stake 難度等級
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stake {
    #[default]
    White = 0,  // 預設，無修正
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
        match self {
            Stake::White => "White Stake",
            Stake::Red => "Red Stake",
            Stake::Green => "Green Stake",
            Stake::Black => "Black Stake",
            Stake::Blue => "Blue Stake",
            Stake::Purple => "Purple Stake",
            Stake::Orange => "Orange Stake",
            Stake::Gold => "Gold Stake",
        }
    }

    /// 分數倍數（Green Stake +25%）
    pub fn score_multiplier(&self) -> f32 {
        match self {
            Stake::Green | Stake::Black | Stake::Blue |
            Stake::Purple | Stake::Orange | Stake::Gold => 1.25,
            _ => 1.0,
        }
    }

    /// 棄牌修正（Blue Stake 及以上 -1）
    pub fn discard_modifier(&self) -> i32 {
        match self {
            Stake::Blue | Stake::Purple | Stake::Orange | Stake::Gold => -1,
            _ => 0,
        }
    }

    /// 出牌修正（Gold Stake -1）
    pub fn hand_modifier(&self) -> i32 {
        match self {
            Stake::Gold => -1,
            _ => 0,
        }
    }

    /// 手牌大小修正（Purple Stake 及以上 -1）
    pub fn hand_size_modifier(&self) -> i32 {
        match self {
            Stake::Purple | Stake::Orange | Stake::Gold => -1,
            _ => 0,
        }
    }

    /// Small Blind 是否給獎勵（Red Stake 及以上不給）
    pub fn small_blind_gives_reward(&self) -> bool {
        matches!(self, Stake::White)
    }

    /// 補充包價格修正（Orange Stake 及以上 +$1）
    pub fn booster_cost_modifier(&self) -> i64 {
        match self {
            Stake::Orange | Stake::Gold => 1,
            _ => 0,
        }
    }

    /// 商店是否有永恆 Joker（Black Stake 及以上）
    pub fn has_eternal_jokers(&self) -> bool {
        matches!(self, Stake::Black | Stake::Blue | Stake::Purple |
                      Stake::Orange | Stake::Gold)
    }

    /// to_index 用於 observation
    pub fn to_index(&self) -> usize {
        *self as usize
    }

    /// 從索引創建 Stake
    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Stake::White),
            1 => Some(Stake::Red),
            2 => Some(Stake::Green),
            3 => Some(Stake::Black),
            4 => Some(Stake::Blue),
            5 => Some(Stake::Purple),
            6 => Some(Stake::Orange),
            7 => Some(Stake::Gold),
            _ => None,
        }
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
