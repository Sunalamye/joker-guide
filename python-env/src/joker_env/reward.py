"""
獎勵計算系統 - Python 端

從 Rust reward.rs 移植，為 RL 訓練提供形狀良好的獎勵信號，支持完整遊戲（Ante 1-8）

獎勵範圍設計（v3.1 - 統一尺度）：
| 模組                     | 範圍             | 說明                           |
|--------------------------|------------------|--------------------------------|
| 遊戲結束 (game_end)      | -0.5 ~ 1.0       | 勝利=1.0，失敗依進度懲罰（終端獎勵）|
| 過關 (blind_clear)       | 0.2 ~ 0.6        | 含 Boss 難度加成、效率獎勵     |
| 出牌 (play_reward)       | 0.0 ~ 0.3        | 含超額獎勵                     |
| 棄牌 (discard_reward)    | 0.0 ~ 0.05       | 鼓勵精準棄牌                   |
| 購買 Joker               | -0.3 ~ 0.3       | 含階段權重、非線性經濟懲罰     |
| Skip Blind/Tag           | -0.15 ~ 0.35     | Tag 價值 - 機會成本 × 風險調整 |
| 消耗品使用               | 0.0 ~ 0.25       | Spectral 後期乘數更強          |
| 金幣狀態 (money_reward)  | 0.0 ~ 0.2        | 利息閾值階梯獎勵               |
| Reroll 決策              | -0.15 ~ 0.0      | 考慮利息損失（純經濟懲罰）     |
| 出售 Joker               | -0.2 ~ 0.2       | 槽位壓力獎勵、相對損失懲罰     |
| Ante 進度                | 0.0 ~ 0.3        | 非線性，後期更有價值           |
| Voucher 購買             | -0.25 ~ 0.3      | 含階段權重、經濟懲罰           |
"""

from dataclasses import dataclass
from typing import Optional
import math

# Action types (與 env.py 一致)
ACTION_TYPE_SELECT = 0
ACTION_TYPE_PLAY = 1
ACTION_TYPE_DISCARD = 2
ACTION_TYPE_SELECT_BLIND = 3
ACTION_TYPE_CASH_OUT = 4
ACTION_TYPE_BUY_JOKER = 5
ACTION_TYPE_NEXT_ROUND = 6
ACTION_TYPE_REROLL = 7
ACTION_TYPE_SELL_JOKER = 8
ACTION_TYPE_SKIP_BLIND = 9
ACTION_TYPE_USE_CONSUMABLE = 10
ACTION_TYPE_BUY_VOUCHER = 11
ACTION_TYPE_BUY_PACK = 12

# Stage constants
STAGE_PRE_BLIND = 0
STAGE_BLIND = 1
STAGE_POST_BLIND = 2
STAGE_SHOP = 3
STAGE_END = 4

# Blind type constants
BLIND_SMALL = 0
BLIND_BIG = 1
BLIND_BOSS = 2

# Game end constants
GAME_END_NONE = 0
GAME_END_WIN = 1
GAME_END_LOSE = 2

# Tag ID constants (對應 Rust tags.rs)
TAG_UNCOMMON = 0
TAG_RARE = 1
TAG_NEGATIVE = 2
TAG_FOIL = 3
TAG_HOLOGRAPHIC = 4
TAG_POLYCHROME = 5
TAG_INVESTMENT = 6
TAG_VOUCHER = 7
TAG_BOSS = 8
TAG_STANDARD = 9
TAG_BUFFOON = 10
TAG_METEOR = 11
TAG_ETHEREAL = 12
TAG_CELESTIAL = 13
TAG_COUPON = 14
TAG_DOUBLE = 15
TAG_JUGGLE = 16
TAG_D6 = 17
TAG_TOP_UP = 18
TAG_SPEED = 19
TAG_ORBITAL = 20
TAG_ECONOMY = 21

# Tag 價值映射（基於遊戲影響力估算）
# 高價值：Joker 強化 / 稀有 Joker
# 中價值：經濟 / 商店優惠
# 低價值：跳過商店 / 小獎勵
TAG_VALUES = {
    TAG_UNCOMMON: 0.25,      # 免費 Uncommon Joker
    TAG_RARE: 0.40,          # 免費 Rare Joker
    TAG_NEGATIVE: 0.52,      # Negative 版本 = 額外槽位，最高價值
    TAG_FOIL: 0.20,          # Foil 版本 +50 chips
    TAG_HOLOGRAPHIC: 0.25,   # Holo 版本 +10 mult
    TAG_POLYCHROME: 0.35,    # Polychrome 版本 ×1.5 mult
    TAG_INVESTMENT: 0.18,    # +$25 回合結束（延遲）
    TAG_VOUCHER: 0.30,       # 免費 Voucher
    TAG_BOSS: 0.15,          # 重抽 Boss（情境性）
    TAG_STANDARD: 0.12,      # Standard Pack
    TAG_BUFFOON: 0.20,       # Buffoon Pack (Joker)
    TAG_METEOR: 0.15,        # Meteor Pack
    TAG_ETHEREAL: 0.18,      # Ethereal Pack (Spectral)
    TAG_CELESTIAL: 0.15,     # Celestial Pack (Planet)
    TAG_COUPON: 0.20,        # 商店 50% off
    TAG_DOUBLE: 0.22,        # 複製下一個 Tag（依賴下一個）
    TAG_JUGGLE: 0.10,        # +3 手牌大小（暫時）
    TAG_D6: 0.15,            # 免費 Reroll
    TAG_TOP_UP: 0.12,        # 2 個消耗品
    TAG_SPEED: 0.06,         # $25 但跳過商店（風險）
    TAG_ORBITAL: 0.20,       # 升級牌型
    TAG_ECONOMY: 0.12,       # +$10
}

# 計算加權平均 Tag 價值（假設均勻分布）
AVG_TAG_VALUE = sum(TAG_VALUES.values()) / len(TAG_VALUES)  # ≈ 0.20

# ============================================================================
# Joker Tier 評估系統（簡化版）
# ============================================================================

# Joker 稀有度基礎價值（基於對遊戲勝率的影響）
# 1=Common, 2=Uncommon, 3=Rare, 4=Legendary
JOKER_RARITY_VALUES = {
    1: 0.08,   # Common: 基礎加成，穩定但不強
    2: 0.15,   # Uncommon: 條件加成，更有潛力
    3: 0.25,   # Rare: 強力效果，顯著提升
    4: 0.40,   # Legendary: 遊戲改變級別
}

# 基於成本估算稀有度（成本越高通常稀有度越高）
# Balatro Joker 成本範圍：2-10
def estimate_joker_rarity_from_cost(cost: int) -> int:
    """
    從 Joker 成本估算稀有度

    Args:
        cost: Joker 購買成本

    Returns:
        估算的稀有度 (1-4)
    """
    if cost <= 4:
        return 1  # Common
    elif cost <= 6:
        return 2  # Uncommon
    elif cost <= 8:
        return 3  # Rare
    else:
        return 4  # Legendary


def estimate_joker_value(cost: int, joker_id: Optional[int] = None) -> float:
    """
    估算 Joker 的價值

    Args:
        cost: Joker 購買成本
        joker_id: Joker ID（可選，用於精確評估）

    Returns:
        估算的 Joker 價值 (0.08 ~ 0.40)
    """
    # 未來可以添加基於 joker_id 的精確評估
    # 目前基於成本估算
    rarity = estimate_joker_rarity_from_cost(cost)
    base_value = JOKER_RARITY_VALUES.get(rarity, 0.1)

    # 成本調整：高成本通常意味著更強效果
    cost_bonus = min((cost - 4) * 0.01, 0.05) if cost > 4 else 0.0

    return base_value + cost_bonus


@dataclass
class StepInfo:
    """從 EnvInfo 解析的狀態信息"""
    # 基本狀態
    episode_step: int = 0
    chips: int = 0  # 當前分數
    blind_target: int = 0

    # 擴展狀態
    ante: int = 1
    stage: int = STAGE_PRE_BLIND
    blind_type: int = -1  # -1 = None
    plays_left: int = 0
    discards_left: int = 0
    money: int = 0

    # 事件追蹤
    score_delta: int = 0
    money_delta: int = 0
    last_action_type: int = -1
    last_action_cost: int = 0

    # Joker 狀態
    joker_count: int = 0
    joker_slot_limit: int = 5

    # 遊戲結束狀態
    game_end: int = GAME_END_NONE
    blind_cleared: bool = False

    # 動作細節
    cards_played: int = 0
    cards_discarded: int = 0
    hand_type: int = -1  # -1 = 無


def parse_env_info(info: dict) -> StepInfo:
    """從 gRPC EnvInfo 解析狀態"""
    return StepInfo(
        episode_step=info.get("episode_step", 0),
        chips=info.get("chips", 0),
        blind_target=info.get("blind_target", 0),
        ante=info.get("ante", 1),
        stage=info.get("stage", STAGE_PRE_BLIND),
        blind_type=info.get("blind_type", -1),
        plays_left=info.get("plays_left", 0),
        discards_left=info.get("discards_left", 0),
        money=info.get("money", 0),
        score_delta=info.get("score_delta", 0),
        money_delta=info.get("money_delta", 0),
        last_action_type=info.get("last_action_type", -1),
        last_action_cost=info.get("last_action_cost", 0),
        joker_count=info.get("joker_count", 0),
        joker_slot_limit=info.get("joker_slot_limit", 5),
        game_end=info.get("game_end", GAME_END_NONE),
        blind_cleared=info.get("blind_cleared", False),
        cards_played=info.get("cards_played", 0),
        cards_discarded=info.get("cards_discarded", 0),
        hand_type=info.get("hand_type", -1),
    )


def clamp(value: float, min_val: float, max_val: float) -> float:
    """限制值在範圍內"""
    return max(min_val, min(max_val, value))


def stage_weight_early(ante: int) -> float:
    """早期更重要的階段權重"""
    if ante <= 2:
        return 1.3
    elif ante <= 4:
        return 1.1
    elif ante <= 6:
        return 1.0
    else:
        return 0.8


def stage_weight_late(ante: int) -> float:
    """後期更重要的階段權重"""
    if ante <= 2:
        return 0.9
    elif ante <= 4:
        return 1.0
    elif ante <= 6:
        return 1.1
    else:
        return 1.2


# ============================================================================
# 核心獎勵函數
# ============================================================================

def play_reward(score_gained: int, required: int) -> float:
    """
    出牌獎勵：正規化到 0~0.3

    - 超額獎勵：得分超過目標有額外獎勵
    - 非線性獎勵曲線：獎勵高效出牌
    """
    if required <= 0 or score_gained <= 0:
        return 0.0

    ratio = score_gained / required

    if ratio >= 1.0:
        # 超額獎勵：一次出牌達標或超標
        base = 0.25
        overkill_bonus = min((ratio - 1.0) * 0.05, 0.05)
        reward = base + overkill_bonus
    else:
        # 未達標：線性獎勵進度
        reward = ratio * 0.25

    return min(reward, 0.3)


def discard_reward(cards_discarded: int, discards_left: int) -> float:
    """
    棄牌獎勵：策略性棄牌可獲得小獎勵（0~0.05）

    - 棄牌越少（更精準），獎勵越高
    - 最後棄牌機會有小獎勵
    """
    if cards_discarded == 0:
        return 0.0

    # 精準棄牌獎勵
    if cards_discarded <= 2:
        efficiency = 0.04
    elif cards_discarded <= 4:
        efficiency = 0.025
    else:
        efficiency = 0.01

    # 最後棄牌機會獎勵
    urgency_bonus = 0.01 if discards_left == 0 else 0.0

    return min(efficiency + urgency_bonus, 0.05)


def blind_clear_reward(
    plays_left: int,
    blind_type: int,
    ante: int,
    boss_blind_id: Optional[int] = None
) -> float:
    """
    過關獎勵：正規化到 0.2~0.6

    - 後期過關獎勵更高
    - Boss 難度加成
    """
    base = {
        BLIND_SMALL: 0.2,
        BLIND_BIG: 0.3,
        BLIND_BOSS: 0.4,
    }.get(blind_type, 0.2)

    # Boss 難度加成（簡化版）
    boss_bonus = 0.0
    if blind_type == BLIND_BOSS and boss_blind_id is not None:
        # 簡化：所有 Boss 給予 0.05 基礎加成
        boss_bonus = 0.05

    # 效率獎勵（剩餘出牌次數）
    efficiency = plays_left * 0.02

    # 階段權重（後期稍高）
    stage_mult = stage_weight_late(ante)

    return clamp((base + boss_bonus + efficiency) * stage_mult, 0.2, 0.6)


def ante_progress_reward(old_ante: int, new_ante: int) -> float:
    """Ante 進度獎勵：非線性，中後期更有價值（0~0.3）"""
    def ante_value(a: int) -> float:
        # 調整範圍使最大 delta ≈ 0.3
        values = {
            1: 0.0,
            2: 0.03,
            3: 0.07,
            4: 0.12,
            5: 0.18,
            6: 0.26,
            7: 0.36,
            8: 0.5,
        }
        return values.get(a, 0.0)

    return min(ante_value(new_ante) - ante_value(old_ante), 0.3)


def game_end_reward(game_end: int, ante: int) -> float:
    """遊戲結束獎勵：正規化到 -0.5~1.0"""
    if game_end == GAME_END_WIN:
        return 1.0
    elif game_end == GAME_END_LOSE:
        progress = ante / 8.0
        return -0.5 * (1.0 - progress)
    return 0.0


def money_reward(money: int, ante: int) -> float:
    """
    金幣獎勵：考慮利息閾值（0~0.2）

    - 每 $5 一級利息，最高 $25
    - 接近下一閾值有額外獎勵
    """
    # 利息閾值獎勵
    interest_tier = clamp(int(money / 5), 0, 5)
    base_interest = interest_tier * 0.025

    # 接近下一閾值獎勵
    next_threshold = (interest_tier + 1) * 5
    gap_to_next = max(0, next_threshold - money)
    threshold_bonus = 0.015 if interest_tier < 5 and gap_to_next <= 2 else 0.0

    # 階段權重
    stage_weights = {1: 1.4, 2: 1.4, 3: 1.1, 4: 1.1, 5: 0.8, 6: 0.8, 7: 0.5, 8: 0.5}
    stage_weight = stage_weights.get(ante, 1.0)

    return min((base_interest + threshold_bonus) * stage_weight, 0.2)


def joker_buy_reward(
    cost: int,
    money_before: int,
    ante: int,
    joker_count_before: int,
    joker_count_after: int,
    joker_slot_limit: int,
    joker_id: Optional[int] = None
) -> float:
    """
    購買 Joker 獎勵（-0.3~0.3）

    考慮因素：
    - Joker 價值（基於成本估算稀有度）
    - 經濟懲罰（成本占比）
    - 利息損失
    - 槽位壓力
    - 階段權重
    """
    # 是否成功購買
    if joker_count_after <= joker_count_before:
        return 0.0

    # Joker 價值估算（基於成本和可選的 joker_id）
    joker_value = estimate_joker_value(cost, joker_id)

    # 非線性經濟懲罰
    cost_ratio = min(cost / money_before, 1.0) if money_before > 0 else 1.0

    if cost_ratio > 0.5:
        economic_penalty = 0.06 + (cost_ratio - 0.5) * 0.15
    else:
        economic_penalty = cost_ratio * 0.1

    # 利息損失
    money_after = money_before - cost
    interest_before = min(money_before // 5, 5)
    interest_after = min(money_after // 5, 5)
    interest_loss = 0.02 * (interest_before - interest_after) if interest_after < interest_before else 0.0

    # 階段權重（早期購買 Joker 更有價值）
    stage_mult = stage_weight_early(ante)

    # 槽位考量：接近滿槽時需要更好的 Joker 才值得
    slot_ratio = joker_count_after / joker_slot_limit
    slot_penalty = 0.03 * slot_ratio if slot_ratio > 0.8 else 0.0

    # 獎勵計算：Joker 價值 × 階段權重 - 各種懲罰
    reward = joker_value * stage_mult - economic_penalty - interest_loss - slot_penalty
    return clamp(reward, -0.3, 0.3)


def get_tag_value(tag_id: Optional[int] = None) -> float:
    """
    獲取 Tag 的價值

    Args:
        tag_id: Tag ID，None 則返回平均值
    """
    if tag_id is not None and tag_id in TAG_VALUES:
        return TAG_VALUES[tag_id]
    return AVG_TAG_VALUE


def skip_blind_reward(
    blind_type: int,
    ante: int,
    tag_id: Optional[int] = None
) -> float:
    """
    跳過 Blind 獎勵

    Args:
        blind_type: Blind 類型
        ante: 當前 Ante
        tag_id: 獲得的 Tag ID（可選，None 則使用平均值）

    獎勵設計：
    - Tag 價值基於 TAG_VALUES 映射（0.06 ~ 0.52）
    - 機會成本考慮 Blind 獎金
    - 後期跳過風險更高（風險調整）
    """
    # 機會成本（跳過 Blind 放棄的獎金）
    opportunity_cost = {
        BLIND_SMALL: 0.05,  # 放棄 $3
        BLIND_BIG: 0.12,    # 放棄 $5 + 更高分數獎勵
        BLIND_BOSS: 1.0,    # 不能跳過 Boss
    }.get(blind_type, 0.05)

    # 風險調整（後期跳過風險更高）
    risk_adjustments = {
        1: 1.0, 2: 1.0,     # 早期：Tag 價值最大化
        3: 0.9, 4: 0.9,     # 中期：稍微保守
        5: 0.75, 6: 0.75,   # 中後期：更保守
        7: 0.5, 8: 0.5,     # 後期：非常保守
    }
    risk_adjustment = risk_adjustments.get(ante, 1.0)

    # Tag 價值（使用具體值或平均值）
    tag_value = get_tag_value(tag_id)

    reward = (tag_value * risk_adjustment) - opportunity_cost
    return clamp(reward, -0.15, 0.35)


def reroll_reward(
    reroll_cost: int,
    money_before: int,
    ante: int,
) -> float:
    """
    Reroll 懲罰

    - Reroll 本身是花錢行為，給予小懲罰
    - 考慮利息損失
    - 是否值得由後續購買決策體現
    """
    if money_before <= 0:
        return -0.1  # 沒錢還 reroll 是錯誤決策

    cost_ratio = min(reroll_cost / money_before, 1.0)

    # 利息損失懲罰
    money_after = money_before - reroll_cost
    interest_before = min(money_before // 5, 5)
    interest_after = min(money_after // 5, 5)
    interest_loss_penalty = 0.03 * (interest_before - interest_after) if interest_after < interest_before else 0.0

    # 階段權重（早期 reroll 更可接受）
    stage_mult = stage_weight_early(ante)

    # 基礎懲罰：reroll 花錢但不保證收益
    base_penalty = -0.02 - cost_ratio * 0.03

    return clamp(base_penalty * stage_mult - interest_loss_penalty, -0.15, 0.0)


def sell_joker_reward(
    money_gained: int,
    ante: int,
    joker_count_before: int,
    joker_slot_limit: int
) -> float:
    """
    出售 Joker 獎勵（-0.2~0.2）

    - 槽位壓力獎勵
    - 金幣收益價值
    """
    # 槽位壓力獎勵
    if joker_count_before >= joker_slot_limit:
        slot_pressure_bonus = 0.08
    elif joker_count_before >= joker_slot_limit - 1:
        slot_pressure_bonus = 0.03
    else:
        slot_pressure_bonus = 0.0

    # 金幣收益價值
    money_value = min(money_gained / 12.0, 0.1)

    # 階段調整
    stage_mults = {1: 0.7, 2: 0.7, 3: 0.9, 4: 0.9, 5: 1.0, 6: 1.0, 7: 1.2, 8: 1.2}
    stage_mult = stage_mults.get(ante, 1.0)

    # 簡化版：假設出售的是弱 Joker
    loss_penalty = 0.04

    reward = (money_value + slot_pressure_bonus - loss_penalty) * stage_mult
    return clamp(reward, -0.2, 0.2)


def consumable_use_reward(ante: int) -> float:
    """
    消耗品使用獎勵（0~0.25）

    簡化版：基於 ante
    """
    # 簡化：假設平均價值 0.12
    base_value = 0.12

    # 階段調整（消耗品後期價值更高）
    stage_mult = stage_weight_late(ante)

    return clamp(base_value * stage_mult, 0.0, 0.25)


def voucher_buy_reward(cost: int, money_before: int, ante: int) -> float:
    """
    Voucher 購買獎勵（-0.25~0.3）

    簡化版
    """
    # 經濟懲罰
    cost_ratio = min(cost / money_before, 1.0) if money_before > 0 else 1.0

    if cost_ratio > 0.5:
        economic_penalty = 0.08 + (cost_ratio - 0.5) * 0.2
    else:
        economic_penalty = cost_ratio * 0.12

    # 階段權重（早期更有價值）
    stage_mults = {1: 1.4, 2: 1.4, 3: 1.2, 4: 1.2, 5: 1.0, 6: 1.0, 7: 0.7, 8: 0.7}
    stage_mult = stage_mults.get(ante, 1.0)

    # 假設平均 Voucher 價值 0.15
    avg_voucher_value = 0.15

    reward = avg_voucher_value * stage_mult - economic_penalty
    return clamp(reward, -0.25, 0.3)


# ============================================================================
# 主要獎勵計算函數
# ============================================================================

class RewardCalculator:
    """獎勵計算器：根據遊戲狀態計算獎勵"""

    def __init__(self):
        self._prev_info: Optional[StepInfo] = None

    def reset(self):
        """重置內部狀態"""
        self._prev_info = None

    def calculate(self, info_dict: dict) -> float:
        """
        根據當前和上一步狀態計算獎勵

        Args:
            info_dict: 從 gRPC EnvInfo 解析的字典

        Returns:
            計算的獎勵值
        """
        info = parse_env_info(info_dict)
        prev = self._prev_info

        reward = 0.0
        action_type = info.last_action_type

        # 遊戲結束
        if info.game_end != GAME_END_NONE:
            reward += game_end_reward(info.game_end, info.ante)

        # Blind 過關
        elif info.blind_cleared:
            reward += blind_clear_reward(
                info.plays_left,
                info.blind_type,
                info.ante
            )

        # 根據動作類型計算獎勵
        elif action_type == ACTION_TYPE_PLAY:
            if info.score_delta > 0:
                reward += play_reward(info.score_delta, info.blind_target)

        elif action_type == ACTION_TYPE_DISCARD:
            # 使用實際棄牌數量（來自 Rust 端）
            reward += discard_reward(info.cards_discarded, info.discards_left)

        elif action_type == ACTION_TYPE_BUY_JOKER:
            if prev is not None:
                reward += joker_buy_reward(
                    info.last_action_cost,
                    prev.money,
                    info.ante,
                    prev.joker_count,
                    info.joker_count,
                    info.joker_slot_limit
                )

        elif action_type == ACTION_TYPE_SELL_JOKER:
            if prev is not None:
                money_gained = info.money - prev.money + info.last_action_cost
                reward += sell_joker_reward(
                    money_gained,
                    info.ante,
                    prev.joker_count,
                    info.joker_slot_limit
                )

        elif action_type == ACTION_TYPE_REROLL:
            reward += reroll_reward(
                info.last_action_cost,
                prev.money if prev else info.money,
                info.ante
            )

        elif action_type == ACTION_TYPE_SKIP_BLIND:
            reward += skip_blind_reward(info.blind_type, info.ante)

        elif action_type == ACTION_TYPE_USE_CONSUMABLE:
            reward += consumable_use_reward(info.ante)

        elif action_type == ACTION_TYPE_BUY_VOUCHER:
            if prev is not None:
                reward += voucher_buy_reward(
                    info.last_action_cost,
                    prev.money,
                    info.ante
                )

        elif action_type == ACTION_TYPE_BUY_PACK:
            # 簡化：卡包購買給予小獎勵
            reward += 0.05

        elif action_type == ACTION_TYPE_CASH_OUT:
            # Cash out 後給予金幣狀態獎勵
            reward += money_reward(info.money, info.ante)

        elif action_type == ACTION_TYPE_NEXT_ROUND:
            # 檢查是否進入新 Ante
            if prev is not None and info.ante > prev.ante:
                reward += ante_progress_reward(prev.ante, info.ante)

        # 保存當前狀態作為下一步的參考
        self._prev_info = info

        return reward


# 創建全局計算器實例
_calculator = RewardCalculator()


def calculate_reward(info_dict: dict) -> float:
    """計算獎勵的便捷函數"""
    return _calculator.calculate(info_dict)


def reset_reward_calculator():
    """重置獎勵計算器狀態"""
    _calculator.reset()
