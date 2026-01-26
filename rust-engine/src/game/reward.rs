//! 獎勵計算系統
//!
//! 為 RL 訓練提供形狀良好的獎勵信號
//!
//! 獎勵範圍設計（v2.0 - 專家分析後調整）：
//! - 遊戲結束獎勵: -0.5 ~ 1.0
//! - 過關獎勵: 0.3 ~ 1.0
//! - 出牌獎勵: 0.0 ~ 0.3
//! - 購買決策: -0.3 ~ 0.5
//! - Skip Blind: -0.15 ~ 0.55（頂級 Tag 需要足夠獎勵空間）
//! - 其他小獎勵: -0.2 ~ 0.3

use super::blinds::{Ante, BlindType, BossBlind, GameEnd};
use super::joker::{JokerId, JokerSlot};
use super::tags::TagId;
use super::consumables::Consumable;
use super::vouchers::VoucherId;
use super::cards::{Card, Enhancement, Seal, Edition};

/// 計算 Joker 組合的戰力分數（模擬實際得分公式）
pub fn combo_score(jokers: &[JokerSlot]) -> f32 {
    if jokers.is_empty() {
        return 0.0;
    }

    let mut chip_power = 0.0;
    let mut mult_power = 0.0;
    let mut x_mult_power = 1.0;

    for j in jokers.iter().filter(|j| j.enabled) {
        match j.id {
            JokerId::SlyJoker => chip_power += 50.0,
            JokerId::WilyJoker | JokerId::DeviousJoker => chip_power += 100.0,
            JokerId::CleverJoker | JokerId::CraftyJoker => chip_power += 80.0,
            JokerId::Banner => chip_power += 60.0, // 假設 2 次棄牌
            JokerId::Joker => mult_power += 4.0,
            JokerId::JollyJoker => mult_power += 8.0,
            JokerId::ZanyJoker | JokerId::CrazyJoker => mult_power += 12.0,
            JokerId::MadJoker | JokerId::DrollJoker => mult_power += 10.0,
            JokerId::HalfJoker => mult_power += 20.0,
            JokerId::MysticSummit => mult_power += 15.0,
            JokerId::Misprint => mult_power += 12.0, // 平均值
            JokerId::AbstractJoker => mult_power += 6.0, // 假設 2 個 Joker
            JokerId::RideTheBus => mult_power += 3.0,
            JokerId::SteelJoker => x_mult_power *= 1.2,
            JokerId::GlassJoker => x_mult_power *= 1.5,
            JokerId::Hologram => x_mult_power *= 1.25,
            // Tier 2+ Jokers with higher impact
            JokerId::Blueprint => x_mult_power *= 1.5,
            JokerId::Brainstorm => x_mult_power *= 1.3,
            JokerId::The_Duo => x_mult_power *= 2.0,
            JokerId::The_Trio => x_mult_power *= 3.0,
            JokerId::The_Family => x_mult_power *= 4.0,
            JokerId::The_Order => x_mult_power *= 3.0,
            JokerId::The_Tribe => x_mult_power *= 2.0,
            _ => chip_power += 10.0,
        }
    }

    // 模擬實際得分公式：(base_chips + chip_bonus) * (base_mult + mult_bonus) * x_mult
    // 假設基礎牌型給 50 chips, 3 mult
    let simulated: f32 = (50.0 + chip_power) * (3.0 + mult_power) * x_mult_power;

    // 用 log2 壓縮範圍，避免極端值
    simulated.max(1.0).log2()
}

/// 買 Joker 獎勵：基於組合分數變化
pub fn joker_buy_reward(
    old_jokers: &[JokerSlot],
    new_jokers: &[JokerSlot],
    cost: i64,
    money_before: i64,
) -> f32 {
    let score_delta = combo_score(new_jokers) - combo_score(old_jokers);

    // 考慮經濟成本：花費佔總資金的比例
    let cost_ratio = if money_before > 0 {
        (cost as f32 / money_before as f32).min(1.0)
    } else {
        1.0
    };

    // 獎勵 = 分數提升 - 經濟成本懲罰
    // 分數提升大且花費佔比小 = 好的購買
    let reward = score_delta * 0.3 - cost_ratio * 0.1;
    reward.clamp(-0.3, 0.5)
}

/// 出牌獎勵：正規化到 0~0.3
pub fn play_reward(score_gained: i64, required: i64) -> f32 {
    if required <= 0 || score_gained <= 0 {
        return 0.0;
    }
    let ratio = (score_gained as f32 / required as f32).min(1.0);
    ratio * 0.3
}

/// 過關獎勵：正規化到 0.3~1.0
/// 考慮 Boss Blind debuff 增加獎勵
pub fn blind_clear_reward(plays_left: i32, blind_type: BlindType, boss_blind: Option<BossBlind>) -> f32 {
    let base = match blind_type {
        BlindType::Small => 0.3,
        BlindType::Big => 0.5,
        BlindType::Boss => 0.7,
    };

    // Boss Blind 難度加成
    let boss_bonus = if blind_type == BlindType::Boss {
        boss_blind.map(|b| boss_difficulty_bonus(b)).unwrap_or(0.0)
    } else {
        0.0
    };

    // 效率獎勵：剩餘出牌次數越多越好
    let efficiency = plays_left as f32 * 0.05;

    (base + boss_bonus + efficiency).min(1.0)
}

/// Boss Blind 難度獎勵加成
fn boss_difficulty_bonus(boss: BossBlind) -> f32 {
    match boss {
        // 高難度 Boss
        BossBlind::TheNeedle | BossBlind::ThePillar | BossBlind::TheManacle => 0.15,
        BossBlind::TheHead | BossBlind::ThePlant | BossBlind::TheFlint => 0.12,
        BossBlind::TheWall | BossBlind::TheWheel | BossBlind::TheMark => 0.1,
        // 中等難度
        BossBlind::TheEye | BossBlind::TheMouth | BossBlind::TheClub => 0.08,
        BossBlind::TheDiamond | BossBlind::TheArm | BossBlind::ThePsychic => 0.07,
        // 一般難度
        BossBlind::TheHook | BossBlind::TheFish | BossBlind::TheSerpent => 0.05,
        BossBlind::TheHeart | BossBlind::TheSpade | BossBlind::TheOx => 0.05,
        BossBlind::TheHouse => 0.04,
        // Showdown Bosses (高難度)
        BossBlind::VioletVessel => 0.25,
        BossBlind::Crimson | BossBlind::Cerulean | BossBlind::Amber | BossBlind::Verdant => 0.2,
    }
}

/// Ante 進度獎勵：非線性，中後期更有價值
pub fn ante_progress_reward(old_ante: Ante, new_ante: Ante) -> f32 {
    let ante_value = |a: Ante| -> f32 {
        match a {
            Ante::One => 0.0,
            Ante::Two => 0.05,
            Ante::Three => 0.1,
            Ante::Four => 0.15,
            Ante::Five => 0.25,   // 關鍵轉型期
            Ante::Six => 0.4,
            Ante::Seven => 0.6,
            Ante::Eight => 1.0,   // 勝利在望
        }
    };
    ante_value(new_ante) - ante_value(old_ante)
}

/// 遊戲結束獎勵：正規化到 -0.5~1.0
pub fn game_end_reward(end: GameEnd, ante: Ante) -> f32 {
    match end {
        GameEnd::Win => 1.0,
        GameEnd::Lose => {
            // 越早失敗懲罰越重
            let progress = ante.to_int() as f32 / 8.0;
            -0.5 * (1.0 - progress) // Ante 1: -0.44, Ante 8: 0
        }
    }
}

/// 金幣獎勵：考慮遊戲階段
pub fn money_reward(money: i64, ante: Ante) -> f32 {
    // 利息潛力：維持 $25+ 可獲最大利息
    let interest_potential = (money as f32 / 25.0).min(1.0);

    // 階段權重：早期存錢更重要
    let stage_weight = match ante {
        Ante::One | Ante::Two => 1.5,
        Ante::Three | Ante::Four => 1.0,
        Ante::Five | Ante::Six => 0.7,
        Ante::Seven | Ante::Eight => 0.3, // 後期錢不重要了
    };

    interest_potential * 0.15 * stage_weight
}

// ============================================================================
// 新增獎勵函數
// ============================================================================

/// Skip Blind 獎勵：考慮 Tag 價值、機會成本和風險
///
/// 設計原則（來自專家分析）：
/// 1. Tag 價值是核心，但需考慮 opportunity cost（跳過的金幣獎勵）
/// 2. 跳過 Big Blind 犧牲更多金幣，風險因子應更低
/// 3. 後期跳過風險更高（已經累積資源，需要打過關），降低獎勵
/// 4. 獎勵上限提升至 0.55，讓頂級 Tag 有合理空間
pub fn skip_blind_reward(tag: TagId, blind_type: BlindType, ante: Ante) -> f32 {
    // Tag 基礎價值
    let tag_value = tag_base_value(tag);

    // 機會成本：跳過 Blind 放棄的金幣獎勵
    // Small: $3 base, Big: $5 base（正常情況）
    let opportunity_cost = match blind_type {
        BlindType::Small => 0.05,  // 放棄較少金幣
        BlindType::Big => 0.12,    // 放棄較多金幣 + 可能的 Boss 獎金
        BlindType::Boss => 1.0,    // 不能跳過 Boss（設為極高懲罰）
    };

    // 風險調整：後期跳過更危險
    // 早期：還有時間累積，跳過問題不大
    // 後期：需要打過關才能贏，跳過失去練習機會
    let risk_adjustment = match ante {
        Ante::One | Ante::Two => 1.0,   // 早期風險低
        Ante::Three | Ante::Four => 0.9,
        Ante::Five | Ante::Six => 0.75, // 中後期謹慎
        Ante::Seven | Ante::Eight => 0.5, // 後期非常謹慎
    };

    // 最終計算：Tag 價值 × 風險調整 - 機會成本
    let reward = (tag_value * risk_adjustment) - opportunity_cost;
    reward.clamp(-0.15, 0.55)
}

/// Tag 基礎價值評估
///
/// 價值調整依據 Balatro 專家分析：
/// - NegativeTag: 額外 Joker 槽位是遊戲中最強效果之一
/// - EtherealTag: Spectral 卡能大幅改變牌組或 Joker
/// - BuffoonTag: 免費選擇 Joker，比單一 Joker 更有價值
/// - SpeedTag: +$25 但跳過商店是嚴重懲罰，淨價值低
fn tag_base_value(tag: TagId) -> f32 {
    match tag {
        // 頂級 Tags（改變遊戲狀態）
        TagId::NegativeTag => 0.52,     // 額外 Joker 槽位極強
        TagId::PolychromeTag => 0.38,   // x1.5 乘數強效
        TagId::RareTag => 0.32,         // 稀有 Joker 通常很強
        TagId::VoucherTag => 0.30,      // 免費 Voucher 永久加成

        // 高價值 Tags
        TagId::EtherealTag => 0.30,     // Spectral 卡強效（修正：0.15→0.30）
        TagId::BuffoonTag => 0.27,      // 免費 Joker 選擇（修正：0.15→0.27）
        TagId::HolographicTag => 0.26,  // +10 Mult 穩定
        TagId::DoubleTag => 0.25,       // 複製 Tag 價值取決於下一個
        TagId::CelestialTag => 0.22,    // Planet 升級重要（修正：0.15→0.22）

        // 中等價值 Tags
        TagId::FoilTag => 0.20,
        TagId::UncommonTag => 0.20,
        TagId::OrbitalTag => 0.20,      // 升級牌型（修正：0.15→0.20）
        TagId::BossTag => 0.20,         // 重抽 Boss 可避開高難度
        TagId::InvestmentTag => 0.20,   // +$25 回合結束
        TagId::MeteorTag => 0.18,       // 卡包價值（修正：0.12→0.18）

        // 經濟類
        TagId::EconomyTag => 0.15,      // +$10 即時
        TagId::CouponTag => 0.15,       // 50% off（取決於商店物品）
        TagId::D6Tag => 0.12,           // 免費 Reroll（情境性）

        // 低價值 Tags
        TagId::StandardTag => 0.10,     // 普通卡包
        TagId::JuggleTag => 0.10,       // +3 手牌大小（情境性）
        TagId::TopUpTag => 0.10,        // 常見消耗品價值低

        // 陷阱類（看似有價值但有嚴重缺點）
        TagId::SpeedTag => 0.06,        // +$25 但跳過商店是災難（修正：0.15→0.06）
    }
}

/// 使用消耗品獎勵
pub fn consumable_use_reward(consumable: &Consumable, ante: Ante) -> f32 {
    use super::consumables::{TarotId, PlanetId, SpectralId};

    let base_value = match consumable {
        Consumable::Tarot(id) => tarot_value(*id),
        Consumable::Planet(id) => planet_value(*id, ante),
        Consumable::Spectral(id) => spectral_value(*id),
    };

    base_value.clamp(0.0, 0.3)
}

/// Tarot 卡價值評估
fn tarot_value(id: super::consumables::TarotId) -> f32 {
    use super::consumables::TarotId;

    match id {
        // 高價值（創造資源或強效增強）
        TarotId::Judgement => 0.25,        // 創造 Joker
        TarotId::TheChariot => 0.2,        // Steel 增強
        TarotId::Justice => 0.18,          // Glass 增強
        TarotId::TheDevil => 0.18,         // Gold 增強
        TarotId::TheEmpress => 0.15,       // Mult 增強

        // 中等價值
        TarotId::TheHermit => 0.15,        // 金幣翻倍
        TarotId::Temperance => 0.12,       // 獲得 Joker 售價
        TarotId::TheWheelOfFortune => 0.15, // 可能加版本
        TarotId::Strength => 0.1,          // +1 點數
        TarotId::TheHighPriestess => 0.12, // 創造 Planet
        TarotId::TheEmperor => 0.1,        // 創造 Tarot

        // 花色轉換
        TarotId::TheWorld | TarotId::TheStar | TarotId::TheMoon | TarotId::TheSun => 0.1,

        // 其他增強
        TarotId::TheMagician | TarotId::TheHierophant | TarotId::TheLovers | TarotId::TheTower => 0.1,

        // 風險類
        TarotId::TheHangedMan => 0.05,     // 銷毀牌
        TarotId::Death => 0.08,            // 複製牌

        // 特殊
        TarotId::TheFool => 0.1,           // 複製上次使用的
    }
}

/// Planet 卡價值評估（考慮遊戲階段）
fn planet_value(id: super::consumables::PlanetId, ante: Ante) -> f32 {
    use super::consumables::PlanetId;

    // Planet 卡升級牌型，早期更有價值（可以累積）
    let stage_mult = match ante {
        Ante::One | Ante::Two => 1.3,
        Ante::Three | Ante::Four => 1.1,
        Ante::Five | Ante::Six => 1.0,
        Ante::Seven | Ante::Eight => 0.8,
    };

    let base = match id {
        // 常用牌型優先
        PlanetId::Jupiter => 0.15,   // Flush
        PlanetId::Earth => 0.15,     // Full House
        PlanetId::Neptune => 0.18,   // Straight Flush
        PlanetId::Mars => 0.15,      // Four of a Kind
        PlanetId::Venus => 0.12,     // Three of a Kind
        PlanetId::Mercury => 0.1,    // Pair
        PlanetId::Saturn => 0.12,    // Straight
        PlanetId::Uranus => 0.08,    // Two Pair
        PlanetId::Pluto => 0.05,     // High Card（很少用）

        // 進階牌型
        PlanetId::PlanetX => 0.2,    // Five of a Kind
        PlanetId::Ceres => 0.2,      // Flush House
        PlanetId::Eris => 0.22,      // Flush Five
    };

    base * stage_mult
}

/// Spectral 卡價值評估
fn spectral_value(id: super::consumables::SpectralId) -> f32 {
    use super::consumables::SpectralId;

    match id {
        // 高價值（強效效果）
        SpectralId::TheSoul => 0.3,       // 創造傳奇 Joker
        SpectralId::BlackHole => 0.25,    // 全部牌型升級
        SpectralId::Wraith => 0.2,        // 創造稀有 Joker
        SpectralId::Aura => 0.18,         // 加版本
        SpectralId::Ectoplasm => 0.2,     // Negative

        // 中等價值
        SpectralId::DejaVu => 0.15,       // Red Seal
        SpectralId::Trance => 0.15,       // Blue Seal
        SpectralId::Medium => 0.12,       // Purple Seal
        SpectralId::Talisman => 0.15,     // Gold Seal
        SpectralId::Cryptid => 0.12,      // 複製牌

        // 風險類（可能有負面效果）
        SpectralId::Immolate => 0.1,      // 銷毀 5 張，得 $20
        SpectralId::Ankh => 0.08,         // 複製 1 個 Joker，銷毀其他
        SpectralId::Hex => 0.1,           // Poly 但銷毀其他

        // 牌組修改
        SpectralId::Sigil => 0.1,         // 統一花色
        SpectralId::Ouija => 0.08,        // 統一點數
        SpectralId::Familiar | SpectralId::Grim | SpectralId::Incantation => 0.1,
    }
}

/// Voucher 購買獎勵（長期價值）
pub fn voucher_buy_reward(voucher: VoucherId, cost: i64, money_before: i64, ante: Ante) -> f32 {
    // Voucher 價值評估
    let voucher_value = voucher_base_value(voucher);

    // 經濟成本
    let cost_ratio = if money_before > 0 {
        (cost as f32 / money_before as f32).min(1.0)
    } else {
        1.0
    };

    // 階段考慮：早期買 Voucher 更有價值（效果累積時間長）
    let stage_mult = match ante {
        Ante::One | Ante::Two => 1.5,
        Ante::Three | Ante::Four => 1.2,
        Ante::Five | Ante::Six => 1.0,
        Ante::Seven | Ante::Eight => 0.7,
    };

    let reward = voucher_value * stage_mult - cost_ratio * 0.1;
    reward.clamp(-0.2, 0.4)
}

/// Voucher 基礎價值
fn voucher_base_value(voucher: VoucherId) -> f32 {
    match voucher {
        // 高價值（直接戰力提升或效率提升）
        VoucherId::Grabber | VoucherId::GrabberPlus => 0.3,  // +1/+2 出牌
        VoucherId::Wasteful | VoucherId::WastefulPlus => 0.2, // +1/+2 棄牌
        VoucherId::Overstock | VoucherId::OverstockPlus => 0.25, // +1/+2 Joker 槽
        VoucherId::CrystalBall | VoucherId::OmenGlobe => 0.2, // +1/+2 消耗品槽

        // 經濟類
        VoucherId::ClearanceSale | VoucherId::Liquidation => 0.2, // 折扣
        VoucherId::RerollSurplus | VoucherId::RerollGlut => 0.15, // Reroll 折扣
        VoucherId::SeedMoney | VoucherId::MoneyTree => 0.15, // 利息上限

        // 稀有度提升
        VoucherId::Hone | VoucherId::GlowUp => 0.15,    // 版本出現率
        VoucherId::Telescope | VoucherId::Nadir => 0.12, // Planet 出現率

        // 其他
        VoucherId::PaintBrush | VoucherId::Palette => 0.1, // Joker 售價
        VoucherId::Tarot_Merchant | VoucherId::Tarot_Tycoon => 0.12,
        VoucherId::Planet_Merchant | VoucherId::Planet_Tycoon => 0.1,
        VoucherId::Magic_Trick | VoucherId::Illusion => 0.1,
        VoucherId::Antimatter | VoucherId::Antimatter_Plus => 0.15,
        VoucherId::Hieroglyph | VoucherId::Petroglyph => 0.12,
        VoucherId::Blank | VoucherId::BlankPlus => 0.0, // 無效果
    }
}

/// 卡牌增強獎勵
pub fn card_enhancement_reward(
    enhancement: Enhancement,
    seal: Seal,
    edition: Edition,
) -> f32 {
    let enhance_value = match enhancement {
        Enhancement::None => 0.0,
        Enhancement::Bonus => 0.05,
        Enhancement::Mult => 0.08,
        Enhancement::Wild => 0.1,
        Enhancement::Glass => 0.12,
        Enhancement::Steel => 0.15,
        Enhancement::Stone => 0.03,
        Enhancement::Gold => 0.08,
        Enhancement::Lucky => 0.1,
    };

    let seal_value = match seal {
        Seal::None => 0.0,
        Seal::Gold => 0.1,
        Seal::Red => 0.08,
        Seal::Blue => 0.08,
        Seal::Purple => 0.05,
    };

    let edition_value = match edition {
        Edition::Base => 0.0,
        Edition::Foil => 0.05,
        Edition::Holographic => 0.08,
        Edition::Polychrome => 0.12,
        Edition::Negative => 0.15,
    };

    let total: f32 = enhance_value + seal_value + edition_value;
    total.clamp(0.0, 0.3)
}

/// Reroll 獎勵（考慮是否值得）
pub fn reroll_reward(
    found_good_joker: bool,
    reroll_cost: i64,
    money: i64,
) -> f32 {
    if found_good_joker {
        // 找到好 Joker，獎勵
        0.1
    } else {
        // 花錢沒找到，小懲罰
        let cost_ratio = if money > 0 {
            (reroll_cost as f32 / money as f32).min(1.0)
        } else {
            1.0
        };
        -0.05 * cost_ratio
    }
}

/// 出售 Joker 獎勵（考慮時機）
pub fn sell_joker_reward(
    sold_joker: &JokerSlot,
    remaining_jokers: &[JokerSlot],
    money_gained: i64,
    ante: Ante,
) -> f32 {
    let old_jokers: Vec<JokerSlot> = remaining_jokers
        .iter()
        .cloned()
        .chain(std::iter::once(sold_joker.clone()))
        .collect();

    // 戰力損失
    let power_loss = combo_score(&old_jokers) - combo_score(remaining_jokers);

    // 金幣收益價值（考慮階段）
    let money_value = money_reward(money_gained, ante);

    // 如果戰力損失小且金幣收益大，是好的出售決策
    let reward = money_value * 2.0 - power_loss * 0.2;
    reward.clamp(-0.2, 0.2)
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combo_score_empty() {
        assert_eq!(combo_score(&[]), 0.0);
    }

    #[test]
    fn test_combo_score_single() {
        let jokers = vec![JokerSlot::new(JokerId::Joker)];
        let score = combo_score(&jokers);
        assert!(score > 0.0);
    }

    #[test]
    fn test_play_reward_basic() {
        assert!((play_reward(100, 1000) - 0.03).abs() < 0.001);
        assert!((play_reward(1000, 1000) - 0.3).abs() < 0.001);
        assert!((play_reward(2000, 1000) - 0.3).abs() < 0.001); // capped
    }

    #[test]
    fn test_blind_clear_reward() {
        let r = blind_clear_reward(2, BlindType::Small, None);
        assert!(r >= 0.3 && r <= 1.0);

        // Boss Blind 有額外獎勵
        let r_boss = blind_clear_reward(2, BlindType::Boss, Some(BossBlind::TheNeedle));
        assert!(r_boss > blind_clear_reward(2, BlindType::Boss, None));
    }

    #[test]
    fn test_game_end_reward() {
        assert_eq!(game_end_reward(GameEnd::Win, Ante::One), 1.0);
        assert!(game_end_reward(GameEnd::Lose, Ante::One) < 0.0);
        assert!(game_end_reward(GameEnd::Lose, Ante::Eight) > -0.1);
    }

    #[test]
    fn test_skip_blind_reward() {
        let r1 = skip_blind_reward(TagId::NegativeTag, BlindType::Small, Ante::One);
        let r2 = skip_blind_reward(TagId::EconomyTag, BlindType::Small, Ante::One);
        assert!(r1 > r2); // NegativeTag 價值更高
    }

    #[test]
    fn test_voucher_buy_reward() {
        let r = voucher_buy_reward(VoucherId::Grabber, 10, 50, Ante::Two);
        assert!(r > 0.0); // Grabber 是好 Voucher
    }

    #[test]
    fn test_card_enhancement_reward() {
        let r_none = card_enhancement_reward(Enhancement::None, Seal::None, Edition::Base);
        let r_steel = card_enhancement_reward(Enhancement::Steel, Seal::Gold, Edition::Polychrome);
        assert_eq!(r_none, 0.0);
        assert!(r_steel > 0.0);
    }

    #[test]
    fn test_consumable_use_reward() {
        use super::super::consumables::{TarotId, PlanetId, SpectralId};

        let tarot_reward = consumable_use_reward(&Consumable::Tarot(TarotId::Judgement), Ante::One);
        let planet_reward = consumable_use_reward(&Consumable::Planet(PlanetId::Jupiter), Ante::One);
        let spectral_reward = consumable_use_reward(&Consumable::Spectral(SpectralId::TheSoul), Ante::One);

        assert!(tarot_reward > 0.0);
        assert!(planet_reward > 0.0);
        assert!(spectral_reward > 0.0);
    }
}
