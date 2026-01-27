//! 獎勵計算系統
//!
//! 為 RL 訓練提供形狀良好的獎勵信號，支持完整遊戲（Ante 1-8）
//!
//! ## 獎勵範圍設計（v3.0 - 多專家分析後調整）
//!
//! | 模組                     | 範圍             | 說明                           |
//! |--------------------------|------------------|--------------------------------|
//! | 遊戲結束 (game_end)      | -0.5 ~ 1.0       | 勝利=1.0，失敗依進度懲罰       |
//! | 過關 (blind_clear)       | 0.3 ~ 1.0        | 含 Boss 難度加成、效率獎勵     |
//! | 出牌 (play_reward)       | 0.0 ~ 0.35       | 含超額獎勵                     |
//! | 棄牌 (discard_reward)    | 0.0 ~ 0.07       | 鼓勵精準棄牌                   |
//! | 購買 Joker               | -0.4 ~ 0.6       | 含階段權重、非線性經濟懲罰     |
//! | 購買 Voucher             | -0.3 ~ 0.5       | 早期購買更有價值               |
//! | Skip Blind/Tag           | -0.15 ~ 0.55     | Tag 價值 - 機會成本 × 風險調整 |
//! | 消耗品使用               | 0.0 ~ 0.35       | Spectral 後期乘數更強          |
//! | 卡牌增強                 | 0.0 ~ 0.3        | 加總 enhance + seal + edition  |
//! | 金幣狀態 (money_reward)  | 0.0 ~ 0.25       | 利息閾值階梯獎勵               |
//! | Reroll 決策              | -0.15 ~ 0.15     | 考慮利息損失                   |
//! | 出售 Joker               | -0.25 ~ 0.25     | 槽位壓力獎勵、相對損失懲罰     |
//! | Ante 進度                | 0.0 ~ 0.4        | 非線性，後期更有價值           |
//!
//! ## v3.0 關鍵改進
//!
//! 1. **Baseline 修正**：空 Joker 返回 baseline 而非 0，避免第一個 Joker delta 異常
//! 2. **壓縮函數**：改用 sqrt 壓縮，保留更多 xMult 區分度
//! 3. **Joker 完整映射**：100+ Jokers 分類評估（含傳奇、S-tier）
//! 4. **階段權重**：所有模組支持 Ante 1-8，早/晚期策略不同
//! 5. **非線性經濟**：花超過 50% 資金加重懲罰
//! 6. **利息閾值**：$5/$10/$15/$20/$25 各級獎勵
//! 7. **槽位壓力**：滿槽時出售有額外獎勵
//!
//! ## 設計原則
//!
//! - **稀疏獎勵緩解**：小步驟也有獎勵（棄牌、金幣狀態）
//! - **無衝突**：購買決策懲罰與金幣獎勵獨立計算
//! - **可解釋性**：獎勵組成可追蹤（delta、penalty、bonus）

use super::blinds::{Ante, BlindType, BossBlind, GameEnd};
use super::joker::{JokerId, JokerSlot};
use super::tags::TagId;
use super::consumables::Consumable;
use super::vouchers::VoucherId;
use super::cards::{Enhancement, Seal, Edition};

/// 基礎戰力（無 Joker 時的基礎分數）
const BASELINE_SCORE: f32 = 150.0; // 50 chips * 3 mult

/// 計算 Joker 組合的戰力分數（模擬實際得分公式）
///
/// v3.0 改進：
/// - 空 Joker 返回 baseline 而非 0，避免第一個 Joker delta 異常
/// - 改用 sqrt 壓縮，保留更多 xMult 區分度
/// - 添加缺失的 S-tier Jokers
/// - 調整條件性 Jokers 的期望值
pub fn combo_score(jokers: &[JokerSlot]) -> f32 {
    // 修正：空 Joker 返回 baseline 的壓縮值，而非 0
    // 這避免了第一個 Joker 的 delta 異常（原本是 8.45，現在是 ~1.0）
    if jokers.is_empty() {
        return (BASELINE_SCORE.sqrt() - 12.0) / 10.0;
    }

    let mut chip_power = 0.0;
    let mut mult_power = 0.0;
    let mut x_mult_power = 1.0;

    for j in jokers.iter().filter(|j| j.enabled) {
        match j.id {
            // ================================================================
            // Chip Jokers（基礎，穩定）
            // ================================================================
            JokerId::SlyJoker => chip_power += 50.0,
            JokerId::WilyJoker | JokerId::DeviousJoker => chip_power += 100.0,
            JokerId::CleverJoker | JokerId::CraftyJoker => chip_power += 80.0,
            JokerId::Banner => chip_power += 60.0,
            JokerId::ScaryFace => chip_power += 60.0,
            JokerId::OddTodd => chip_power += 50.0,
            JokerId::Arrowhead => chip_power += 80.0,
            JokerId::Stuntman => chip_power += 200.0,
            JokerId::Runner => chip_power += 40.0,
            JokerId::Stone => chip_power += 75.0,
            JokerId::Bull => chip_power += 40.0,

            // ================================================================
            // Mult Jokers（中階，靈活）
            // ================================================================
            JokerId::Joker => mult_power += 4.0,
            JokerId::JollyJoker => mult_power += 8.0,
            JokerId::ZanyJoker | JokerId::CrazyJoker => mult_power += 12.0,
            JokerId::MadJoker | JokerId::DrollJoker => mult_power += 10.0,
            JokerId::MysticSummit => mult_power += 15.0,
            JokerId::Misprint => mult_power += 12.0,
            JokerId::AbstractJoker => mult_power += 9.0,
            JokerId::GreenJoker => mult_power += 6.0,
            JokerId::Fibonacci => mult_power += 10.0,
            JokerId::EvenSteven => mult_power += 6.0,
            JokerId::Scholar => mult_power += 8.0,
            JokerId::Supernova => mult_power += 8.0,
            JokerId::Smiley => mult_power += 8.0,
            JokerId::FortuneTeller => mult_power += 6.0,
            JokerId::ShootTheMoon => mult_power += 10.0,
            JokerId::Walkie => mult_power += 8.0,
            JokerId::Spare_Trousers => mult_power += 6.0,
            JokerId::Trousers => mult_power += 8.0,
            JokerId::Bootstraps => mult_power += 8.0,
            JokerId::Flash => mult_power += 6.0,
            JokerId::RedCard => mult_power += 6.0,
            JokerId::Erosion => mult_power += 8.0,

            // 條件性 Mult（降低期望值，因為不總是觸發）
            JokerId::HalfJoker => mult_power += 10.0,  // 原 20，條件：≤3 張
            JokerId::RideTheBus => mult_power += 8.0,  // 可 scaling

            // ================================================================
            // xMult Jokers（高階，後期強）
            // ================================================================
            // 基礎 xMult
            JokerId::SteelJoker => x_mult_power *= 1.3,
            JokerId::GlassJoker => x_mult_power *= 1.4,
            JokerId::Hologram => x_mult_power *= 1.35,
            JokerId::Photograph => x_mult_power *= 1.5,
            JokerId::DuskJoker => x_mult_power *= 1.4,
            JokerId::Acrobat => x_mult_power *= 1.8,
            JokerId::Bloodstone => x_mult_power *= 1.25,
            JokerId::Opal => x_mult_power *= 1.3,
            JokerId::Lucky_Cat => x_mult_power *= 1.25,

            // 條件性 xMult（調低期望值）
            JokerId::The_Duo => x_mult_power *= 1.6,      // 原 2.0，需要 Pair
            JokerId::The_Trio => x_mult_power *= 1.8,     // 原 3.0，需要 Three of a Kind
            JokerId::The_Family => x_mult_power *= 2.2,   // 原 4.0，需要 Four of a Kind
            JokerId::The_Order => x_mult_power *= 1.8,    // 原 3.0，需要 Straight
            JokerId::The_Tribe => x_mult_power *= 1.6,    // 原 2.0，需要 Flush
            JokerId::Seeing_Double => x_mult_power *= 1.5,
            JokerId::Flower_Pot => x_mult_power *= 1.8,
            JokerId::DriversLicense => x_mult_power *= 1.8,
            JokerId::Card_Sharp => x_mult_power *= 1.8,

            // ================================================================
            // S-Tier Jokers（頂級，被嚴重低估的）
            // ================================================================
            JokerId::Throwback => x_mult_power *= 1.8,    // 可 scaling 到 x4+
            JokerId::Cavendish => x_mult_power *= 2.8,    // 無條件 x3，極強
            JokerId::Vampire => x_mult_power *= 1.6,      // scaling xMult
            JokerId::Baron => x_mult_power *= 1.8,        // 每張 King x1.5
            JokerId::Constellation => x_mult_power *= 1.5,
            JokerId::Campfire => x_mult_power *= 1.4,
            JokerId::Obelisk => x_mult_power *= 1.4,
            JokerId::Hit_The_Road => x_mult_power *= 1.5,
            JokerId::SteakJoker => x_mult_power *= 1.8,
            JokerId::Ramen => x_mult_power *= 1.7,
            JokerId::Madness => x_mult_power *= 1.3,

            // ================================================================
            // 傳奇 Jokers（遊戲改變級）
            // ================================================================
            JokerId::Canio | JokerId::Caino => x_mult_power *= 2.5,
            JokerId::Triboulet => x_mult_power *= 2.2,
            JokerId::Yorick => x_mult_power *= 1.8,
            JokerId::Chicot => mult_power += 25.0,  // 禁用 Boss = 大幅降低難度
            JokerId::Perkeo => x_mult_power *= 2.0,

            // ================================================================
            // 複製類（動態價值，取保守估計）
            // ================================================================
            JokerId::Blueprint => x_mult_power *= 1.6,
            JokerId::Brainstorm => x_mult_power *= 1.5,

            // ================================================================
            // 重觸發類（間接增強）
            // ================================================================
            JokerId::SockAndBuskin => mult_power += 12.0,
            JokerId::Hack => mult_power += 10.0,
            JokerId::HangingChad => mult_power += 8.0,
            JokerId::Mime => mult_power += 10.0,
            JokerId::DNA => mult_power += 15.0,
            JokerId::Selzer => mult_power += 8.0,

            // ================================================================
            // 經濟類（間接價值，給中等評分）
            // ================================================================
            JokerId::GoldenJoker => chip_power += 30.0,
            JokerId::Egg => chip_power += 20.0,
            JokerId::Rocket => chip_power += 25.0,
            JokerId::ToTheMoon => chip_power += 30.0,
            JokerId::CloudNine | JokerId::Cloud9 => chip_power += 25.0,
            JokerId::Satellite => chip_power += 25.0,
            JokerId::Delayed => chip_power += 20.0,
            JokerId::Matador => chip_power += 30.0,
            JokerId::Golden_Ticket => chip_power += 25.0,
            JokerId::Certificate => chip_power += 20.0,
            JokerId::Ticket => chip_power += 20.0,
            JokerId::RoughGem => chip_power += 20.0,
            JokerId::Faceless => chip_power += 20.0,
            JokerId::CreditCard => chip_power += 15.0,

            // ================================================================
            // 工具類（改變規則，難以量化）
            // ================================================================
            JokerId::FourFingers => mult_power += 8.0,
            JokerId::Shortcut => mult_power += 6.0,
            JokerId::Splash => mult_power += 10.0,
            JokerId::Pareidolia => mult_power += 8.0,
            JokerId::Smeared => mult_power += 6.0,
            JokerId::MrBones => chip_power += 50.0,  // 防死機制，極有價值
            JokerId::Stencil => x_mult_power *= 1.3,
            JokerId::Ring_Master => mult_power += 5.0,

            // ================================================================
            // 消耗品生成類
            // ================================================================
            JokerId::Cartomancer => chip_power += 35.0,
            JokerId::Astronomer => chip_power += 40.0,
            JokerId::Vagabond => chip_power += 30.0,
            JokerId::SpaceJoker => chip_power += 35.0,
            JokerId::Seance => chip_power += 30.0,
            JokerId::Sixth => chip_power += 25.0,

            // ================================================================
            // 其他/未分類（基於稀有度給予合理默認值）
            // ================================================================
            JokerId::IceCream => chip_power += 60.0,
            JokerId::BlueJoker => chip_power += 40.0,
            JokerId::Hiker => chip_power += 30.0,
            JokerId::Popcorn => mult_power += 12.0,
            JokerId::AncientJoker => x_mult_power *= 1.3,
            JokerId::Castle => chip_power += 40.0,
            JokerId::Swashbuckler => mult_power += 6.0,
            JokerId::Troubadour => chip_power += 30.0,
            JokerId::Ceremonial => chip_power += 20.0,
            JokerId::Wee => chip_power += 60.0,
            JokerId::Merry => mult_power += 10.0,
            JokerId::Square => chip_power += 50.0,
            JokerId::RiffRaff => chip_power += 30.0,
            JokerId::InvisibleJoker => chip_power += 40.0,
            JokerId::Gros_Michel => mult_power += 12.0,
            JokerId::Even_Steven => x_mult_power *= 1.4,
            JokerId::Odd_Todd_2 => x_mult_power *= 1.4,
            JokerId::Juggler => chip_power += 25.0,
            JokerId::Courier => chip_power += 40.0,
            JokerId::Drunkard => chip_power += 20.0,
            JokerId::BusinessCard => chip_power += 20.0,

            // 套利/經濟 Jokers
            JokerId::GreedyJoker | JokerId::LustyJoker |
            JokerId::WrathfulJoker | JokerId::GluttonousJoker => chip_power += 25.0,
            JokerId::Onyx => mult_power += 10.0,

            // 經濟類 Joker (產金幣) - TODO: Add when Jokers are implemented
            // JokerId::ReservedParking | JokerId::MailInRebate => chip_power += 15.0,
            // 創建卡牌類 Joker - TODO: Add when Jokers are implemented
            // JokerId::TradingCard | JokerId::MarbleJoker => chip_power += 20.0,

            // 預留/未使用
            JokerId::BluePrint | JokerId::Perkeo_2 | JokerId::Stuntman_2 |
            JokerId::Rough_Gem_2 => chip_power += 10.0,

            // 真正的默認（應該很少觸發）
            #[allow(unreachable_patterns)]
            _ => chip_power += 20.0,
        }
    }

    // 模擬實際得分公式
    let simulated: f32 = (50.0 + chip_power) * (3.0 + mult_power) * x_mult_power;

    // v3.0: 改用 sqrt 壓縮，保留更多區分度
    // 正規化到約 0~4 範圍，與其他獎勵尺度一致
    (simulated.max(BASELINE_SCORE).sqrt() - 12.0) / 10.0
}

/// 買 Joker 獎勵：基於組合分數變化
///
/// v3.0 改進：
/// - 非線性經濟懲罰（花超過 50% 資金加重懲罰）
/// - 階段權重（早期購買更重要）
/// - 利息潛力損失懲罰
/// - 擴展範圍到 -0.4 ~ 0.6
pub fn joker_buy_reward(
    old_jokers: &[JokerSlot],
    new_jokers: &[JokerSlot],
    cost: i64,
    money_before: i64,
) -> f32 {
    joker_buy_reward_with_ante(old_jokers, new_jokers, cost, money_before, Ante::One)
}

/// 帶階段參數的 Joker 購買獎勵
pub fn joker_buy_reward_with_ante(
    old_jokers: &[JokerSlot],
    new_jokers: &[JokerSlot],
    cost: i64,
    money_before: i64,
    ante: Ante,
) -> f32 {
    let score_delta = combo_score(new_jokers) - combo_score(old_jokers);

    // 非線性經濟懲罰：花越多比例，懲罰越重
    let cost_ratio = if money_before > 0 {
        (cost as f32 / money_before as f32).min(1.0)
    } else {
        1.0
    };

    // 花超過 50% 資金開始加重懲罰
    let economic_penalty = if cost_ratio > 0.5 {
        0.1 + (cost_ratio - 0.5) * 0.3  // 0.1 ~ 0.25
    } else {
        cost_ratio * 0.2  // 0 ~ 0.1
    };

    // 利息潛力損失（保持 $25 可獲最大利息）
    let interest_loss = if money_before >= 25 && (money_before - cost) < 25 {
        0.05
    } else {
        0.0
    };

    // 階段調整：早期購買 Joker 更重要（有更多時間發揮效果）
    let stage_mult = match ante {
        Ante::One | Ante::Two => 1.3,
        Ante::Three | Ante::Four => 1.1,
        Ante::Five | Ante::Six => 1.0,
        Ante::Seven | Ante::Eight => 0.8,
    };

    // 獎勵 = 戰力提升 × 階段權重 - 經濟懲罰 - 利息損失
    let reward = score_delta * 0.4 * stage_mult - economic_penalty - interest_loss;
    reward.clamp(-0.4, 0.6)
}

/// 出牌獎勵：正規化到 0~0.35
///
/// v3.0 改進：
/// - 超額獎勵（得分超過目標有額外獎勵）
/// - 非線性獎勵曲線（獎勵高效出牌）
pub fn play_reward(score_gained: i64, required: i64) -> f32 {
    if required <= 0 || score_gained <= 0 {
        return 0.0;
    }

    let ratio = score_gained as f32 / required as f32;

    let reward = if ratio >= 1.0 {
        // 超額獎勵：一次出牌達標或超標
        // ratio = 1.0 → 0.3, ratio = 2.0 → 0.35（有上限避免極端值）
        let base = 0.3;
        let overkill_bonus = ((ratio - 1.0) * 0.05).min(0.05);
        base + overkill_bonus
    } else {
        // 未達標：線性獎勵進度
        ratio * 0.3
    };

    // 確保浮點精度不導致超出範圍
    reward.min(0.35)
}

/// 棄牌獎勵：策略性棄牌可獲得小獎勵
///
/// 設計原則：
/// - 棄牌是策略的一部分，但不應過度鼓勵
/// - 改善手牌質量的棄牌是好的
/// - 浪費棄牌機會是不好的
pub fn discard_reward(cards_discarded: usize, discards_left: i32) -> f32 {
    if cards_discarded == 0 {
        return 0.0;
    }

    // 棄牌越少（更精準），獎勵越高
    // 但完全不棄牌不在這裡處理
    let efficiency = match cards_discarded {
        1..=2 => 0.05,  // 精準棄牌
        3..=4 => 0.03,  // 中等
        _ => 0.01,      // 大量棄牌
    };

    // 如果這是最後的棄牌機會，給予小獎勵（鼓勵使用）
    let urgency_bonus = if discards_left == 0 { 0.02 } else { 0.0 };

    efficiency + urgency_bonus
}

/// 過關獎勵：正規化到 0.3~1.0
/// 考慮 Boss Blind debuff 增加獎勵
pub fn blind_clear_reward(plays_left: i32, blind_type: BlindType, boss_blind: Option<BossBlind>) -> f32 {
    blind_clear_reward_with_ante(plays_left, blind_type, boss_blind, Ante::One)
}

/// 帶階段參數的過關獎勵
///
/// v3.0 改進：
/// - 後期過關獎勵更高（接近勝利）
/// - Boss 難度加成與階段結合
pub fn blind_clear_reward_with_ante(
    plays_left: i32,
    blind_type: BlindType,
    boss_blind: Option<BossBlind>,
    ante: Ante,
) -> f32 {
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

    // 階段權重：後期過關更有價值（接近勝利）
    let stage_mult = match ante {
        Ante::One | Ante::Two => 0.9,
        Ante::Three | Ante::Four => 1.0,
        Ante::Five | Ante::Six => 1.1,
        Ante::Seven | Ante::Eight => 1.2,
    };

    ((base + boss_bonus + efficiency) * stage_mult).min(1.0)
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

/// 金幣獎勵：考慮遊戲階段和利息最優化
///
/// v3.0 改進：
/// - 考慮利息閾值（$5, $10, $15, $20, $25 各級）
/// - 接近下一閾值時給予額外獎勵（鼓勵湊整）
/// - 後期保持適度存款仍有價值（緊急情況需要資金）
pub fn money_reward(money: i64, ante: Ante) -> f32 {
    // 利息閾值獎勵：每 $5 一級，最高 $25
    let interest_tier = ((money as f32 / 5.0).floor() as i32).clamp(0, 5);
    let base_interest = interest_tier as f32 * 0.03;  // 0~0.15

    // 接近下一閾值獎勵（鼓勵存到整數）
    let next_threshold = (interest_tier + 1) * 5;
    let gap_to_next = (next_threshold as i64 - money).max(0) as f32;
    let threshold_bonus = if interest_tier < 5 && gap_to_next <= 2.0 {
        0.02 // 距離下一閾值 $2 內，給小獎勵
    } else {
        0.0
    };

    // 階段權重：早期存錢很重要，後期仍有價值（但降低）
    let stage_weight = match ante {
        Ante::One | Ante::Two => 1.4,
        Ante::Three | Ante::Four => 1.1,
        Ante::Five | Ante::Six => 0.8,
        Ante::Seven | Ante::Eight => 0.5, // 後期仍保留一些價值
    };

    ((base_interest + threshold_bonus) * stage_weight).min(0.25)
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
///
/// v3.1 改進（基於多專家分析）：
/// - 動態 Tag 價值：HandyTag/GarbageTag 根據剩餘次數計算
/// - Ante-aware 機會成本：後期金幣更有價值
/// - 改進風險曲線：減緩後期下降，頂級 Tag 保底
pub fn skip_blind_reward(tag: TagId, blind_type: BlindType, ante: Ante) -> f32 {
    // 使用預設的 plays/discards（向後兼容）
    skip_blind_reward_v2(tag, blind_type, ante, 4, 3)
}

/// v3.1 完整版 Skip Blind 獎勵
///
/// 新增參數：
/// - plays_left: 剩餘出牌次數（用於 HandyTag 動態計算）
/// - discards_left: 剩餘棄牌次數（用於 GarbageTag 動態計算）
pub fn skip_blind_reward_v2(
    tag: TagId,
    blind_type: BlindType,
    ante: Ante,
    plays_left: i32,
    discards_left: i32,
) -> f32 {
    // v3.1: 動態 Tag 價值
    let tag_value = tag_dynamic_value(tag, plays_left, discards_left);

    // v3.1: Ante-aware 機會成本
    let opportunity_cost = skip_opportunity_cost(blind_type, ante);

    // v3.1: 改進的風險調整（含頂級 Tag 保底）
    let risk_adjustment = skip_risk_adjustment(tag_value, ante);

    // 最終計算：Tag 價值 × 風險調整 - 機會成本
    let reward = (tag_value * risk_adjustment) - opportunity_cost;
    reward.clamp(-0.15, 0.55)
}

/// 動態 Tag 價值計算
///
/// HandyTag 和 GarbageTag 的實際價值取決於剩餘次數
fn tag_dynamic_value(tag: TagId, plays_left: i32, discards_left: i32) -> f32 {
    match tag {
        // 動態計算：$1 per hand/discard
        TagId::HandyTag => 0.02 * plays_left as f32,      // 4 hands = 0.08
        TagId::GarbageTag => 0.02 * discards_left as f32, // 3 discards = 0.06
        // 其他 Tag 使用靜態基礎價值
        _ => tag_base_value(tag),
    }
}

/// Ante-aware 機會成本
///
/// 後期金幣更有價值，機會成本應更高
fn skip_opportunity_cost(blind_type: BlindType, ante: Ante) -> f32 {
    let base = match blind_type {
        BlindType::Small => 0.05,  // 放棄較少金幣
        BlindType::Big => 0.12,    // 放棄較多金幣 + 可能的 Boss 獎金
        BlindType::Boss => 1.0,    // 不能跳過 Boss
    };

    // 後期金幣更有價值
    let ante_mult = match ante {
        Ante::One | Ante::Two => 1.0,
        Ante::Three | Ante::Four => 1.1,
        Ante::Five | Ante::Six => 1.25,
        Ante::Seven | Ante::Eight => 1.4,
    };

    base * ante_mult
}

/// 改進的風險調整曲線
///
/// v3.1 改進：
/// - 減緩後期下降（0.5 → 0.70）
/// - 頂級 Tag（價值 > 0.35）保底 0.75，確保後期仍有吸引力
fn skip_risk_adjustment(tag_value: f32, ante: Ante) -> f32 {
    let base_risk: f32 = match ante {
        Ante::One | Ante::Two => 1.0,
        Ante::Three | Ante::Four => 0.95,
        Ante::Five | Ante::Six => 0.85,   // 減緩下降（原 0.75）
        Ante::Seven | Ante::Eight => 0.70, // 減緩下降（原 0.5）
    };

    // 頂級 Tag 保底：價值 > 0.35 的 Tag 風險係數不低於 0.75
    // 確保 NegativeTag、PolychromeTag 等在後期仍有吸引力
    if tag_value > 0.35 {
        base_risk.max(0.75)
    } else {
        base_risk
    }
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

        // 動態經濟類（基於 hands/discards）
        TagId::HandyTag => 0.08,        // $4 (1 per hand) - 小額但穩定
        TagId::GarbageTag => 0.06,      // $3 (1 per discard) - 小額穩定

        // Arcana Pack 類
        TagId::CharmTag => 0.24,        // Mega Arcana Pack - 5 張 Tarot 選 1
    }
}

/// 使用消耗品獎勵
///
/// v3.0 改進：
/// - 加入階段權重（Spectral 後期更強）
/// - 擴展上限到 0.35（頂級 Spectral 需要空間）
pub fn consumable_use_reward(consumable: &Consumable, ante: Ante) -> f32 {
    let base_value = match consumable {
        Consumable::Tarot(id) => tarot_value(*id, ante),
        Consumable::Planet(id) => planet_value(*id, ante),
        Consumable::Spectral(id) => spectral_value(*id, ante),
    };

    base_value.clamp(0.0, 0.35)
}

/// Tarot 卡價值評估（考慮遊戲階段）
///
/// v3.0 改進：
/// - 加入階段權重（增強類早期更有價值）
/// - 調整 Death 價值（複製強牌可以非常強）
fn tarot_value(id: super::consumables::TarotId, ante: Ante) -> f32 {
    use super::consumables::TarotId;

    // 增強類 Tarot 早期更有價值（有更多時間受益）
    let enhance_mult = match ante {
        Ante::One | Ante::Two => 1.2,
        Ante::Three | Ante::Four => 1.1,
        Ante::Five | Ante::Six => 1.0,
        Ante::Seven | Ante::Eight => 0.9,
    };

    let base = match id {
        // 高價值（創造資源或強效增強）
        TarotId::Judgement => 0.28,        // 創造 Joker（提升：0.25→0.28）
        TarotId::TheChariot => 0.22,       // Steel 增強（提升：0.2→0.22）
        TarotId::Justice => 0.18,          // Glass 增強
        TarotId::TheDevil => 0.16,         // Gold 增強
        TarotId::TheEmpress => 0.15,       // Mult 增強

        // 中等價值
        TarotId::TheHermit => 0.15,        // 金幣翻倍
        TarotId::Temperance => 0.12,       // 獲得 Joker 售價
        TarotId::TheWheelOfFortune => 0.18, // 可能加版本（提升）
        TarotId::Strength => 0.12,         // +1 點數（對特定 build 有用）
        TarotId::TheHighPriestess => 0.14, // 創造 Planet（提升）
        TarotId::TheEmperor => 0.12,       // 創造 Tarot

        // 花色轉換（對 Flush build 重要）
        TarotId::TheWorld | TarotId::TheStar | TarotId::TheMoon | TarotId::TheSun => 0.12,

        // 其他增強
        TarotId::TheMagician => 0.1,       // Lucky
        TarotId::TheHierophant => 0.08,    // Bonus（較弱）
        TarotId::TheLovers => 0.1,         // Wild
        TarotId::TheTower => 0.08,         // Stone（較弱）

        // 風險類（需要謹慎使用但潛力大）
        TarotId::TheHangedMan => 0.06,     // 銷毀牌
        TarotId::Death => 0.15,            // 複製牌（提升：0.08→0.15，複製強牌可以極強）

        // 特殊
        TarotId::TheFool => 0.12,          // 複製上次使用的（提升）
    };

    // 增強類 Tarot 應用階段權重
    match id {
        TarotId::TheChariot | TarotId::Justice | TarotId::TheDevil |
        TarotId::TheEmpress | TarotId::TheMagician | TarotId::TheHierophant |
        TarotId::TheLovers | TarotId::TheTower | TarotId::Death => base * enhance_mult,
        _ => base,
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

/// Spectral 卡價值評估（考慮遊戲階段）
///
/// v3.0 改進：
/// - 加入階段權重（xMult 類後期更強）
/// - 提升頂級 Spectral 價值（TheSoul、BlackHole）
fn spectral_value(id: super::consumables::SpectralId, ante: Ante) -> f32 {
    use super::consumables::SpectralId;

    // Spectral 的 xMult 效果後期更強（與 Joker 組合）
    let late_game_mult = match ante {
        Ante::One | Ante::Two => 0.9,
        Ante::Three | Ante::Four => 1.0,
        Ante::Five | Ante::Six => 1.1,
        Ante::Seven | Ante::Eight => 1.2,
    };

    let base = match id {
        // 頂級價值（遊戲改變級）
        SpectralId::TheSoul => 0.35,       // 創造傳奇 Joker（提升：0.3→0.35）
        SpectralId::BlackHole => 0.30,     // 全部牌型升級（提升：0.25→0.30）
        SpectralId::Wraith => 0.22,        // 創造稀有 Joker（提升）
        SpectralId::Aura => 0.20,          // 加版本（提升）
        SpectralId::Ectoplasm => 0.25,     // Negative（提升：非常強，增加槽位）

        // Seal 類（穩定價值）
        SpectralId::DejaVu => 0.15,        // Red Seal（重觸發）
        SpectralId::Trance => 0.16,        // Blue Seal（Planet 牌）
        SpectralId::Medium => 0.12,        // Purple Seal（Tarot 牌）
        SpectralId::Talisman => 0.14,      // Gold Seal（$3）
        SpectralId::Cryptid => 0.15,       // 複製牌（提升：對好牌非常強）

        // 風險類（需要謹慎但潛力大）
        SpectralId::Immolate => 0.12,      // 銷毀 5 張，得 $20（提升：可啟用 Glass 等）
        SpectralId::Ankh => 0.10,          // 複製 1 個 Joker（提升：對好 Joker 極強）
        SpectralId::Hex => 0.12,           // Poly 但銷毀其他（提升）

        // 牌組修改
        SpectralId::Sigil => 0.12,         // 統一花色（對 Flush build 重要）
        SpectralId::Ouija => 0.10,         // 統一點數（對特定 build）
        SpectralId::Familiar => 0.12,      // 增強人頭牌（提升）
        SpectralId::Grim => 0.10,          // 增強 Ace
        SpectralId::Incantation => 0.12,   // 增強數字牌（提升）
    };

    // xMult 相關的 Spectral 應用後期權重
    match id {
        SpectralId::Aura | SpectralId::Ectoplasm | SpectralId::Hex => base * late_game_mult,
        _ => base,
    }
}

/// Voucher 購買獎勵（長期價值）
///
/// v3.0 改進：
/// - 非線性經濟懲罰（與 Joker 購買一致）
/// - 擴展範圍到 -0.3 ~ 0.5
pub fn voucher_buy_reward(voucher: VoucherId, cost: i64, money_before: i64, ante: Ante) -> f32 {
    // Voucher 價值評估
    let voucher_value = voucher_base_value(voucher);

    // 非線性經濟懲罰（與 Joker 購買一致）
    let cost_ratio = if money_before > 0 {
        (cost as f32 / money_before as f32).min(1.0)
    } else {
        1.0
    };

    let economic_penalty = if cost_ratio > 0.5 {
        0.1 + (cost_ratio - 0.5) * 0.25
    } else {
        cost_ratio * 0.15
    };

    // 階段考慮：早期買 Voucher 更有價值（效果累積時間長）
    let stage_mult = match ante {
        Ante::One | Ante::Two => 1.5,
        Ante::Three | Ante::Four => 1.2,
        Ante::Five | Ante::Six => 1.0,
        Ante::Seven | Ante::Eight => 0.7,
    };

    let reward = voucher_value * stage_mult - economic_penalty;
    reward.clamp(-0.3, 0.5)
}

/// Voucher 基礎價值
///
/// v3.0 改進：
/// - 提升核心 Voucher 價值（Grabber、Overstock）
/// - Plus 版本應該比基礎版本更有價值
fn voucher_base_value(voucher: VoucherId) -> f32 {
    match voucher {
        // 頂級價值（直接戰力提升）
        VoucherId::Grabber => 0.28,           // +1 出牌
        VoucherId::GrabberPlus => 0.35,       // +2 出牌（提升：Plus 更強）
        VoucherId::Overstock => 0.25,         // +1 Joker 槽
        VoucherId::OverstockPlus => 0.32,     // +2 Joker 槽（提升）

        // 高價值
        VoucherId::Wasteful => 0.18,          // +1 棄牌
        VoucherId::WastefulPlus => 0.22,      // +2 棄牌
        VoucherId::CrystalBall => 0.18,       // +1 消耗品槽
        VoucherId::OmenGlobe => 0.22,         // +2 消耗品槽

        // 經濟類（長期回報）
        VoucherId::ClearanceSale => 0.2,      // 25% 折扣
        VoucherId::Liquidation => 0.25,       // 50% 折扣（提升）
        VoucherId::SeedMoney => 0.15,         // 利息上限 +$10
        VoucherId::MoneyTree => 0.2,          // 利息上限 +$20（提升）
        VoucherId::RerollSurplus => 0.12,     // Reroll -$1
        VoucherId::RerollGlut => 0.15,        // Reroll -$2

        // 稀有度提升
        VoucherId::Hone => 0.15,              // 版本出現率
        VoucherId::GlowUp => 0.2,             // 更高版本率（提升）
        VoucherId::Telescope => 0.12,         // Planet 出現率
        VoucherId::Nadir => 0.15,             // 更高 Planet 率

        // 消耗品商店
        VoucherId::Tarot_Merchant => 0.12,
        VoucherId::Tarot_Tycoon => 0.15,
        VoucherId::Planet_Merchant => 0.1,
        VoucherId::Planet_Tycoon => 0.12,
        VoucherId::Magic_Trick => 0.1,        // 牌可從商店購買
        VoucherId::Illusion => 0.12,

        // 其他
        VoucherId::PaintBrush => 0.08,        // Joker 售價提升
        VoucherId::Palette => 0.1,
        VoucherId::Antimatter => 0.18,        // +1 Joker 槽（提升：實際上很強）
        VoucherId::Antimatter_Plus => 0.22,   // +2 Joker 槽
        VoucherId::Hieroglyph => 0.1,         // -1 Ante，+1 hand
        VoucherId::Petroglyph => 0.12,        // -1 Ante，+1 discard
        VoucherId::Blank => 0.0,              // 無效果
        VoucherId::BlankPlus => 0.0,
        VoucherId::Observatory => 0.2,        // Planet 對應牌型 X1.5 Mult
        VoucherId::ObservatoryPlus => 0.25,   // Planet 對應牌型 X2 Mult
        VoucherId::DirectorsCut => 0.12,      // Boss Blind reroll 一次 ($10)
        VoucherId::Retcon => 0.18,            // 無限免費 Boss reroll
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
///
/// v3.0 改進：
/// - 更細緻的結果評估（好/一般/差）
/// - 考慮 reroll 對利息的影響
/// - 考慮遊戲階段（早期探索更有價值）
pub fn reroll_reward(
    found_good_joker: bool,
    reroll_cost: i64,
    money: i64,
) -> f32 {
    reroll_reward_with_ante(found_good_joker, reroll_cost, money, Ante::One)
}

/// 帶階段參數的 Reroll 獎勵
pub fn reroll_reward_with_ante(
    found_good_joker: bool,
    reroll_cost: i64,
    money: i64,
    ante: Ante,
) -> f32 {
    let cost_ratio = if money > 0 {
        (reroll_cost as f32 / money as f32).min(1.0)
    } else {
        1.0
    };

    // 利息損失懲罰：reroll 導致跌破 $5 閾值
    let money_after = money - reroll_cost;
    let interest_before = (money / 5).min(5);
    let interest_after = (money_after / 5).min(5);
    let interest_loss_penalty = if interest_after < interest_before {
        0.03 * (interest_before - interest_after) as f32
    } else {
        0.0
    };

    // 階段權重：早期 reroll 探索更有價值
    let stage_mult = match ante {
        Ante::One | Ante::Two => 1.2,
        Ante::Three | Ante::Four => 1.0,
        Ante::Five | Ante::Six => 0.8,
        Ante::Seven | Ante::Eight => 0.6,
    };

    let base_reward = if found_good_joker {
        // 找到好 Joker，根據花費給予獎勵
        0.12 - cost_ratio * 0.04  // 0.08 ~ 0.12
    } else {
        // 沒找到，懲罰與花費成比例
        -0.03 - cost_ratio * 0.05  // -0.03 ~ -0.08
    };

    (base_reward * stage_mult - interest_loss_penalty).clamp(-0.15, 0.15)
}

/// 出售 Joker 獎勵（考慮時機和槽位壓力）
///
/// v3.0 改進：
/// - 考慮槽位壓力（5/5 時出售騰出空間有額外價值）
/// - 考慮 Joker 間的協同效應損失
/// - 考慮階段（後期整理弱牌是好策略）
pub fn sell_joker_reward(
    sold_joker: &JokerSlot,
    remaining_jokers: &[JokerSlot],
    money_gained: i64,
    ante: Ante,
) -> f32 {
    sell_joker_reward_with_slots(sold_joker, remaining_jokers, money_gained, ante, 5, 5)
}

/// 帶槽位資訊的出售 Joker 獎勵
pub fn sell_joker_reward_with_slots(
    sold_joker: &JokerSlot,
    remaining_jokers: &[JokerSlot],
    money_gained: i64,
    ante: Ante,
    joker_slots: usize,
    current_jokers: usize,
) -> f32 {
    let old_jokers: Vec<JokerSlot> = remaining_jokers
        .iter()
        .cloned()
        .chain(std::iter::once(sold_joker.clone()))
        .collect();

    // 戰力損失（使用 delta 而非絕對值）
    let power_before = combo_score(&old_jokers);
    let power_after = combo_score(remaining_jokers);
    let power_loss = power_before - power_after;

    // 相對戰力損失（與當前戰力相比）
    let relative_loss = if power_before > 0.0 {
        power_loss / power_before
    } else {
        0.0
    };

    // 槽位壓力獎勵：滿槽時出售騰出空間有價值
    let slot_pressure_bonus = if current_jokers >= joker_slots {
        0.1  // 滿槽時出售有額外價值
    } else if current_jokers >= joker_slots - 1 {
        0.03  // 接近滿槽
    } else {
        0.0
    };

    // 金幣收益價值
    let money_value = (money_gained as f32 / 10.0).min(0.15);  // $10 = 0.1, 上限 0.15

    // 利息潛力考量（出售後是否跨過利息閾值）
    // 這裡簡化處理，實際需要知道出售前的金額
    let interest_bonus = if money_gained >= 5 {
        0.02  // 賣掉可能幫助達到利息閾值
    } else {
        0.0
    };

    // 階段調整
    // 早期：Joker 珍貴，不鼓勵出售
    // 後期：整理弱牌、優化組合是好策略
    let stage_mult = match ante {
        Ante::One | Ante::Two => 0.7,   // 早期不鼓勵出售
        Ante::Three | Ante::Four => 0.9,
        Ante::Five | Ante::Six => 1.0,
        Ante::Seven | Ante::Eight => 1.2, // 後期整理是好的
    };

    // 出售弱 Joker（相對損失小）應該獎勵
    // 出售強 Joker（相對損失大）應該懲罰
    let loss_penalty = relative_loss * 0.3;

    let reward = (money_value + interest_bonus + slot_pressure_bonus - loss_penalty) * stage_mult;
    reward.clamp(-0.25, 0.25)
}

// ============================================================================
// 單元測試
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combo_score_empty() {
        // v3.0: 空 Joker 返回 baseline 而非 0，避免第一個 Joker delta 異常
        let empty_score = combo_score(&[]);
        let expected = (BASELINE_SCORE.sqrt() - 12.0) / 10.0;
        assert!((empty_score - expected).abs() < 0.001);
    }

    #[test]
    fn test_combo_score_single() {
        let jokers = vec![JokerSlot::new(JokerId::Joker)];
        let score = combo_score(&jokers);
        let empty_score = combo_score(&[]);
        // 單個 Joker 應該比空狀態得分更高
        assert!(score > empty_score);
    }

    #[test]
    fn test_combo_score_delta_normalized() {
        // 驗證第一個 Joker 的 delta 已被正規化（不再是異常的 8.45）
        let empty_score = combo_score(&[]);
        let single_score = combo_score(&vec![JokerSlot::new(JokerId::Joker)]);
        let delta = single_score - empty_score;
        // delta 應該在合理範圍內（約 0.5~2.0），而非原本的 8.45
        assert!(delta > 0.0 && delta < 3.0, "delta = {}", delta);
    }

    #[test]
    fn test_combo_score_xmult_scaling() {
        // 驗證 xMult Jokers 的區分度
        let duo = vec![JokerSlot::new(JokerId::The_Duo)];
        let family = vec![JokerSlot::new(JokerId::The_Family)];
        let duo_score = combo_score(&duo);
        let family_score = combo_score(&family);
        // The_Family (x2.2) 應該明顯高於 The_Duo (x1.6)
        assert!(family_score > duo_score);
    }

    #[test]
    fn test_combo_score_legendary() {
        // 驗證傳奇 Joker 得到正確評估
        let canio = vec![JokerSlot::new(JokerId::Canio)];
        let basic = vec![JokerSlot::new(JokerId::Joker)];
        let empty = combo_score(&[]);
        let canio_score = combo_score(&canio);
        let basic_score = combo_score(&basic);
        // 傳奇 Joker 應該高於基礎 Joker
        // 注意：xMult Jokers (Canio x2.5) 在基礎 mult 較低時效果不如 +mult Jokers
        // 但傳奇 Joker 的 delta 應該明顯更大
        let canio_delta = canio_score - empty;
        let basic_delta = basic_score - empty;
        assert!(canio_delta > basic_delta, "canio_delta={}, basic_delta={}", canio_delta, basic_delta);
    }

    #[test]
    fn test_play_reward_basic() {
        // 未達標：線性獎勵
        assert!((play_reward(100, 1000) - 0.03).abs() < 0.001);

        // 剛好達標
        assert!((play_reward(1000, 1000) - 0.3).abs() < 0.001);

        // 超額：有額外獎勵但有上限
        let overkill = play_reward(2000, 1000);
        assert!(overkill > 0.3);      // 超額應該比達標高
        assert!(overkill <= 0.35);    // 但不超過上限
    }

    #[test]
    fn test_discard_reward() {
        // 精準棄牌
        assert!(discard_reward(1, 2) > 0.0);
        assert!(discard_reward(2, 2) > 0.0);

        // 大量棄牌獎勵較低
        assert!(discard_reward(1, 2) > discard_reward(5, 2));

        // 不棄牌沒有獎勵
        assert_eq!(discard_reward(0, 2), 0.0);
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
    fn test_skip_blind_reward_v2_dynamic_tags() {
        // HandyTag 價值應該隨 plays_left 變化
        let handy_4 = skip_blind_reward_v2(TagId::HandyTag, BlindType::Small, Ante::One, 4, 3);
        let handy_2 = skip_blind_reward_v2(TagId::HandyTag, BlindType::Small, Ante::One, 2, 3);
        assert!(handy_4 > handy_2, "More plays = higher HandyTag value");

        // GarbageTag 價值應該隨 discards_left 變化
        let garbage_3 = skip_blind_reward_v2(TagId::GarbageTag, BlindType::Small, Ante::One, 4, 3);
        let garbage_1 = skip_blind_reward_v2(TagId::GarbageTag, BlindType::Small, Ante::One, 4, 1);
        assert!(garbage_3 > garbage_1, "More discards = higher GarbageTag value");
    }

    #[test]
    fn test_skip_blind_reward_v2_top_tier_floor() {
        // 頂級 Tag（NegativeTag）在後期仍應有較高價值
        let negative_early = skip_blind_reward_v2(TagId::NegativeTag, BlindType::Small, Ante::One, 4, 3);
        let negative_late = skip_blind_reward_v2(TagId::NegativeTag, BlindType::Small, Ante::Eight, 4, 3);

        // 後期價值應該 >= 0.25（保底機制）
        assert!(negative_late >= 0.25, "Top-tier Tag should have floor: {}", negative_late);

        // 後期價值不應下降超過 40%（相比早期）
        let ratio = negative_late / negative_early;
        assert!(ratio >= 0.6, "Late game ratio should be >= 0.6: {}", ratio);
    }

    #[test]
    fn test_skip_blind_reward_v2_ante_aware_cost() {
        // 相同 Tag，後期機會成本更高
        let early = skip_blind_reward_v2(TagId::EconomyTag, BlindType::Big, Ante::One, 4, 3);
        let late = skip_blind_reward_v2(TagId::EconomyTag, BlindType::Big, Ante::Eight, 4, 3);

        // 後期獎勵應該更低（機會成本更高）
        assert!(early > late, "Late game opportunity cost should be higher");
    }

    #[test]
    fn test_skip_risk_adjustment() {
        // 驗證風險調整曲線
        let low_value_early = skip_risk_adjustment(0.15, Ante::One);
        let low_value_late = skip_risk_adjustment(0.15, Ante::Eight);
        assert!(low_value_early > low_value_late);

        // 頂級 Tag 保底
        let high_value_late = skip_risk_adjustment(0.50, Ante::Eight);
        assert!(high_value_late >= 0.75, "Top-tier should have floor: {}", high_value_late);
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

    #[test]
    fn test_money_reward_interest_tiers() {
        // 測試利息閾值邏輯
        let r0 = money_reward(0, Ante::One);
        let r5 = money_reward(5, Ante::One);
        let r10 = money_reward(10, Ante::One);
        let r25 = money_reward(25, Ante::One);

        // 更多錢應該有更高獎勵
        assert!(r5 > r0);
        assert!(r10 > r5);
        assert!(r25 > r10);

        // 但有上限
        let r100 = money_reward(100, Ante::One);
        assert!(r100 <= 0.25);
    }

    #[test]
    fn test_money_reward_stage_scaling() {
        // 早期存錢更重要
        let early = money_reward(20, Ante::One);
        let late = money_reward(20, Ante::Eight);
        assert!(early > late);
    }

    #[test]
    fn test_reroll_reward_found_good() {
        // 找到好 Joker 應該獲得正獎勵
        let r = reroll_reward(true, 5, 50);
        assert!(r > 0.0);
    }

    #[test]
    fn test_reroll_reward_not_found() {
        // 沒找到應該有小懲罰
        let r = reroll_reward(false, 5, 50);
        assert!(r < 0.0);
    }

    #[test]
    fn test_reroll_reward_cost_matters() {
        // 花費越高，獎勵/懲罰應該越顯著
        let cheap_good = reroll_reward_with_ante(true, 2, 100, Ante::One);
        let expensive_good = reroll_reward_with_ante(true, 10, 100, Ante::One);
        // 便宜找到好的 > 貴的找到好的
        assert!(cheap_good > expensive_good);
    }

    #[test]
    fn test_sell_joker_reward_slot_pressure() {
        let weak_joker = JokerSlot::new(JokerId::Joker);
        let remaining = vec![
            JokerSlot::new(JokerId::Canio),
            JokerSlot::new(JokerId::The_Family),
            JokerSlot::new(JokerId::The_Duo),
            JokerSlot::new(JokerId::The_Trio),
        ];

        // 滿槽時出售弱 Joker 應該有獎勵（騰出空間）
        let full_slot = sell_joker_reward_with_slots(&weak_joker, &remaining, 5, Ante::Five, 5, 5);
        // 有空槽時出售
        let has_space = sell_joker_reward_with_slots(&weak_joker, &remaining, 5, Ante::Five, 5, 4);
        // 滿槽時出售應該有更高獎勵（槽位壓力獎勵）
        assert!(full_slot > has_space);
    }

    #[test]
    fn test_sell_joker_reward_stage_timing() {
        // 使用更強的 remaining 陣容，讓出售弱 Joker 的相對損失更小
        let weak_joker = JokerSlot::new(JokerId::Joker);
        let remaining = vec![
            JokerSlot::new(JokerId::Canio),        // x2.5
            JokerSlot::new(JokerId::The_Family),   // x2.2
            JokerSlot::new(JokerId::The_Duo),      // x1.6
        ];

        // 後期出售弱 Joker 應該比早期更受鼓勵
        // 滿槽情況下，確保有足夠的正向獎勵
        let early = sell_joker_reward_with_slots(&weak_joker, &remaining, 6, Ante::One, 5, 5);
        let late = sell_joker_reward_with_slots(&weak_joker, &remaining, 6, Ante::Eight, 5, 5);

        // 在滿槽且出售價格較高的情況下，後期獎勵應該更高
        // 因為：slot_pressure_bonus + money_value > loss_penalty（弱 Joker 損失小）
        assert!(late > early, "early={}, late={}", early, late);
    }
}
