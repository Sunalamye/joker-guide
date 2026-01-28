//! Blind 和 Boss Blind 定義

use super::constants::PLAYS_PER_BLIND;

/// 遊戲階段
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    PreBlind,
    Blind,
    PostBlind,
    Shop,
    End(GameEnd),
}

/// 遊戲結束狀態
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameEnd {
    Win,
    Lose,
}

/// Blind 類型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlindType {
    Small,
    Big,
    Boss,
}

impl BlindType {
    pub fn reward(&self) -> i64 {
        match self {
            BlindType::Small => 3,
            BlindType::Big => 4,
            BlindType::Boss => 5,
        }
    }

    pub fn score_multiplier(&self) -> f32 {
        match self {
            BlindType::Small => 1.0,
            BlindType::Big => 1.5,
            BlindType::Boss => 2.0,
        }
    }

    pub fn next(&self) -> Option<BlindType> {
        match self {
            BlindType::Small => Some(BlindType::Big),
            BlindType::Big => Some(BlindType::Boss),
            BlindType::Boss => None, // Boss 之後進入下一個 Ante
        }
    }

    pub fn to_int(&self) -> i32 {
        match self {
            BlindType::Small => 0,
            BlindType::Big => 1,
            BlindType::Boss => 2,
        }
    }
}

/// Boss Blind 類型 - 每個有獨特的 debuff 效果
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossBlind {
    // 分數修改類
    TheHook,  // 每手開始時隨機棄 2 張
    TheWall,  // 需要 4x 分數 (而非 2x)
    TheWheel, // 1/7 的牌面朝下
    TheArm,   // 降低出過的牌型等級
    TheFlint, // 基礎 chips 和 mult 減半

    // 花色禁用類
    TheClub,    // 梅花牌不計分
    TheDiamond, // 方塊牌不計分 (又名 The Window)
    TheHeart,   // 紅心牌不計分
    TheSpade,   // 黑桃牌不計分 (又名 The Goad - 只有 Spade 計分的反面)

    // 強制行為類
    ThePsychic, // 必須出 5 張牌
    TheMouth,   // 只能出一種牌型
    TheEye,     // 不能重複出同一種牌型
    ThePlant,   // Face Card 不計分
    TheSerpent, // 每次出牌後抽 3 棄 3

    // 經濟懲罰類
    TheOx,    // 出 #(當前ante) 牌型時失去 $1
    TheHouse, // 第一手是面朝下的
    TheMark,  // 所有 Face Card 面朝下
    TheFish,  // 開始時面朝下的牌打亂順序

    // 特殊類
    TheManacle, // 手牌上限 -1
    ThePillar,  // 已打過的牌不再計分
    TheNeedle,  // 只有 1 次出牌機會
    TheHead,    // 紅心牌只能在第一手出

    // Ante 8 專屬
    VioletVessel, // 需要 6x 分數
    Crimson,      // 每輪 hand 數 -1
    Cerulean,     // 強制在開始時使用消耗品
    Amber,        // 無法使用消耗品
    Verdant,      // 所有牌在回合開始時面朝下
}

impl BossBlind {
    /// Boss Blind 的分數倍數 (大部分是 2x)
    pub fn score_multiplier(&self) -> f32 {
        match self {
            BossBlind::TheWall => 4.0,
            BossBlind::VioletVessel => 6.0,
            _ => 2.0,
        }
    }

    /// 轉換為整數 ID (用於 observation)
    pub fn to_int(&self) -> i32 {
        match self {
            BossBlind::TheHook => 0,
            BossBlind::TheWall => 1,
            BossBlind::TheWheel => 2,
            BossBlind::TheArm => 3,
            BossBlind::TheFlint => 4,
            BossBlind::TheClub => 5,
            BossBlind::TheDiamond => 6,
            BossBlind::TheHeart => 7,
            BossBlind::TheSpade => 8,
            BossBlind::ThePsychic => 9,
            BossBlind::TheMouth => 10,
            BossBlind::TheEye => 11,
            BossBlind::ThePlant => 12,
            BossBlind::TheSerpent => 13,
            BossBlind::TheOx => 14,
            BossBlind::TheHouse => 15,
            BossBlind::TheMark => 16,
            BossBlind::TheFish => 17,
            BossBlind::TheManacle => 18,
            BossBlind::ThePillar => 19,
            BossBlind::TheNeedle => 20,
            BossBlind::TheHead => 21,
            BossBlind::VioletVessel => 22,
            BossBlind::Crimson => 23,
            BossBlind::Cerulean => 24,
            BossBlind::Amber => 25,
            BossBlind::Verdant => 26,
        }
    }

    /// 常規 Boss 列表 (Ante 1-7 使用)
    pub fn regular_bosses() -> &'static [BossBlind] {
        &[
            BossBlind::TheHook,
            BossBlind::TheWall,
            BossBlind::TheWheel,
            BossBlind::TheArm,
            BossBlind::TheFlint,
            BossBlind::TheClub,
            BossBlind::TheDiamond,
            BossBlind::TheHeart,
            BossBlind::TheSpade,
            BossBlind::ThePsychic,
            BossBlind::TheMouth,
            BossBlind::TheEye,
            BossBlind::ThePlant,
            BossBlind::TheSerpent,
            BossBlind::TheOx,
            BossBlind::TheHouse,
            BossBlind::TheMark,
            BossBlind::TheFish,
            BossBlind::TheManacle,
            BossBlind::ThePillar,
            BossBlind::TheNeedle,
            BossBlind::TheHead,
        ]
    }

    /// Ante 8 專屬 Boss 列表
    pub fn showdown_bosses() -> &'static [BossBlind] {
        &[
            BossBlind::VioletVessel,
            BossBlind::Crimson,
            BossBlind::Cerulean,
            BossBlind::Amber,
            BossBlind::Verdant,
        ]
    }

    /// 檢查是否禁用指定花色
    pub fn disables_suit(&self, suit: u8) -> bool {
        match (self, suit) {
            (BossBlind::TheClub, 0) => true,    // Clubs = 0
            (BossBlind::TheDiamond, 1) => true, // Diamonds = 1
            (BossBlind::TheHeart, 2) => true,   // Hearts = 2
            (BossBlind::TheSpade, 3) => true,   // Spades = 3
            _ => false,
        }
    }

    /// 檢查是否禁用 Face Card
    pub fn disables_face_cards(&self) -> bool {
        matches!(self, BossBlind::ThePlant)
    }

    /// 檢查是否需要剛好 5 張牌
    pub fn requires_five_cards(&self) -> bool {
        matches!(self, BossBlind::ThePsychic)
    }

    /// 獲取最大出牌次數 (某些 Boss 會限制)
    pub fn max_plays(&self) -> Option<i32> {
        match self {
            BossBlind::TheNeedle => Some(1),
            BossBlind::Crimson => Some(PLAYS_PER_BLIND - 1),
            _ => None,
        }
    }
}

pub const BOSS_BLIND_COUNT: usize = 27;

/// Ante 定義
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ante {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
}

impl Ante {
    pub fn base_score(&self) -> i64 {
        // 正確的 Balatro 基礎分數 (White Stake)
        match self {
            Ante::One => 300,
            Ante::Two => 800,
            Ante::Three => 2_000,
            Ante::Four => 5_000,
            Ante::Five => 11_000,
            Ante::Six => 20_000,
            Ante::Seven => 35_000,
            Ante::Eight => 50_000,
        }
    }

    pub fn next(&self) -> Option<Ante> {
        match self {
            Ante::One => Some(Ante::Two),
            Ante::Two => Some(Ante::Three),
            Ante::Three => Some(Ante::Four),
            Ante::Four => Some(Ante::Five),
            Ante::Five => Some(Ante::Six),
            Ante::Six => Some(Ante::Seven),
            Ante::Seven => Some(Ante::Eight),
            Ante::Eight => None,
        }
    }

    pub fn to_int(&self) -> i32 {
        match self {
            Ante::One => 1,
            Ante::Two => 2,
            Ante::Three => 3,
            Ante::Four => 4,
            Ante::Five => 5,
            Ante::Six => 6,
            Ante::Seven => 7,
            Ante::Eight => 8,
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
    fn test_blind_type_basics() {
        assert_eq!(BlindType::Small.reward(), 3);
        assert_eq!(BlindType::Big.reward(), 4);
        assert_eq!(BlindType::Boss.reward(), 5);
        assert_eq!(BlindType::Small.score_multiplier(), 1.0);
        assert_eq!(BlindType::Boss.score_multiplier(), 2.0);
        assert_eq!(BlindType::Small.next(), Some(BlindType::Big));
        assert_eq!(BlindType::Big.next(), Some(BlindType::Boss));
        assert_eq!(BlindType::Boss.next(), None);
        assert_eq!(BlindType::Big.to_int(), 1);
    }

    #[test]
    fn test_boss_blind_rules() {
        assert_eq!(BossBlind::TheWall.score_multiplier(), 4.0);
        assert_eq!(BossBlind::VioletVessel.score_multiplier(), 6.0);
        assert_eq!(BossBlind::TheFlint.score_multiplier(), 2.0);

        assert!(BossBlind::ThePlant.disables_face_cards());
        assert!(!BossBlind::TheHook.disables_face_cards());

        assert!(BossBlind::ThePsychic.requires_five_cards());
        assert!(!BossBlind::TheNeedle.requires_five_cards());

        assert_eq!(BossBlind::TheClub.disables_suit(0), true);
        assert_eq!(BossBlind::TheDiamond.disables_suit(1), true);
        assert_eq!(BossBlind::TheHeart.disables_suit(2), true);
        assert_eq!(BossBlind::TheSpade.disables_suit(3), true);
        assert_eq!(BossBlind::TheSpade.disables_suit(2), false);
    }

    #[test]
    fn test_boss_blind_lists_and_limits() {
        assert_eq!(BossBlind::regular_bosses().len(), 22);
        assert_eq!(BossBlind::showdown_bosses().len(), 5);
        assert_eq!(BOSS_BLIND_COUNT, 27);

        assert_eq!(BossBlind::TheNeedle.max_plays(), Some(1));
        assert_eq!(BossBlind::Crimson.max_plays(), Some(PLAYS_PER_BLIND - 1));
        assert_eq!(BossBlind::TheHook.max_plays(), None);
    }

    #[test]
    fn test_ante_progression() {
        assert_eq!(Ante::One.base_score(), 300);
        assert_eq!(Ante::Five.base_score(), 11_000);
        assert_eq!(Ante::One.next(), Some(Ante::Two));
        assert_eq!(Ante::Seven.next(), Some(Ante::Eight));
        assert_eq!(Ante::Eight.next(), None);
        assert_eq!(Ante::Four.to_int(), 4);
    }
}
