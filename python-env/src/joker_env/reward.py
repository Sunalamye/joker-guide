"""
獎勵計算系統 - Python 端

從 Rust reward.rs 移植，為 RL 訓練提供形狀良好的獎勵信號，支持完整遊戲（Ante 1-8）

獎勵範圍設計（v5.0 - 平衡 Ante 進度與 Skip 決策）：
設計原則：
- 終端獎勵主導：勝利=5.0，確保長期目標壓過短期收益
- 漸進式 Ante 進度：平衡的獎勵增長（0.15×a^1.5+0.1×a）
- 提高 Clear 獎勵：確保 Clear Blind 比 Skip 更有吸引力
- 調整 Skip 風險：早期 Skip 更保守，上限低於 Clear 獎勵

| 模組                     | 範圍             | 說明                              |
|--------------------------|------------------|-----------------------------------|
| 遊戲結束 (game_end)      | -2.0 ~ 5.0       | 勝利=5.0，失敗依進度懲罰          |
| Ante 進度                | 0.48 ~ 2.27      | 漸進式增長（0.15×a^1.5+0.1×a）   |
| 過關 (blind_clear)       | 0.25 ~ 0.75      | 基礎值提升，Ante 調整加成         |
| 出牌 (play_reward)       | 0.0 ~ 0.15       | 降低以突出終端獎勵                |
| 棄牌 (discard_reward)    | -0.05 ~ 0.05     | 空棄牌懲罰 + 精準棄牌獎勵        |
| 購買 Joker               | -0.3 ~ 0.3       | 含階段權重、非線性經濟懲罰        |
| Skip Blind/Tag           | -0.20 ~ 0.25     | 提高機會成本，調整風險係數        |
| 消耗品使用               | 0.0 ~ 0.25       | Spectral 後期乘數更強             |
| 金幣狀態 (money_reward)  | 0.0 ~ 0.2        | 利息閾值階梯獎勵                  |
| Reroll 決策              | -0.15 ~ 0.0      | 考慮利息損失（純經濟懲罰）        |
| 出售 Joker               | -0.2 ~ 0.2       | 槽位壓力獎勵、相對損失懲罰        |
| Voucher 購買             | -0.25 ~ 0.3      | 含階段權重、經濟懲罰              |
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
TAG_HANDY = 22
TAG_GARBAGE = 23
TAG_CHARM = 24

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
    TAG_HANDY: 0.10,         # $1 per hand（基礎）
    TAG_GARBAGE: 0.08,       # $1 per discard（基礎）
    TAG_CHARM: 0.18,         # Mega Arcana Pack
}

# 計算加權平均 Tag 價值（假設均勻分布）
AVG_TAG_VALUE = sum(TAG_VALUES.values()) / len(TAG_VALUES)  # ≈ 0.20

# ============================================================================
# Hand types / build tracking (aligned with Rust HandId order)
# ============================================================================

HAND_HIGH_CARD = 0
HAND_PAIR = 1
HAND_TWO_PAIR = 2
HAND_THREE_KIND = 3
HAND_STRAIGHT = 4
HAND_FLUSH = 5
HAND_FULL_HOUSE = 6
HAND_FOUR_KIND = 7
HAND_STRAIGHT_FLUSH = 8
HAND_ROYAL_FLUSH = 9
HAND_FIVE_KIND = 10
HAND_FLUSH_HOUSE = 11
HAND_FLUSH_FIVE = 12

BUILD_PAIRS = 0
BUILD_STRAIGHT = 1
BUILD_FLUSH = 2

_HAND_STRENGTH_ORDER = {
    HAND_HIGH_CARD: 0,
    HAND_PAIR: 1,
    HAND_TWO_PAIR: 2,
    HAND_THREE_KIND: 3,
    HAND_STRAIGHT: 4,
    HAND_FLUSH: 5,
    HAND_FULL_HOUSE: 6,
    HAND_FOUR_KIND: 7,
    HAND_STRAIGHT_FLUSH: 8,
    HAND_ROYAL_FLUSH: 9,
    HAND_FIVE_KIND: 10,
    HAND_FLUSH_HOUSE: 11,
    HAND_FLUSH_FIVE: 12,
}

_BUILD_HANDS = {
    BUILD_PAIRS: {
        HAND_PAIR,
        HAND_TWO_PAIR,
        HAND_THREE_KIND,
        HAND_FULL_HOUSE,
        HAND_FOUR_KIND,
        HAND_FIVE_KIND,
        HAND_FLUSH_HOUSE,
    },
    BUILD_STRAIGHT: {HAND_STRAIGHT, HAND_STRAIGHT_FLUSH, HAND_ROYAL_FLUSH},
    BUILD_FLUSH: {HAND_FLUSH, HAND_STRAIGHT_FLUSH, HAND_ROYAL_FLUSH, HAND_FLUSH_HOUSE, HAND_FLUSH_FIVE},
}

# Joker build support (minimal mapping for reward shaping)
JOKER_BUILD_SUPPORT = {
    5: BUILD_PAIRS,  # JollyJoker (pair-oriented)
    9: BUILD_FLUSH,  # DrollJoker (flush-oriented)
}


def blind_progress_signal(
    score_before: int,
    score_after: int,
    blind_target: int,
    plays_left: int,
    total_plays: int = 4,
) -> float:
    """
    Blind 內進度獎勵（細粒度）

    - 進度獎勵：Δscore / target
    - 里程碑：跨過 80% 增加小獎勵
    - 節奏：在預期節奏之前達到更高進度，給額外獎勵
    """
    if blind_target <= 0:
        return 0.0
    if score_after <= score_before:
        return 0.0

    before_ratio = score_before / blind_target
    after_ratio = score_after / blind_target
    progress = max(0.0, after_ratio - before_ratio)

    reward = progress * 0.03

    # 80% 里程碑
    if before_ratio < 0.8 <= after_ratio:
        reward += 0.01

    # 節奏獎勵：越早達標越好
    total_plays = max(1, total_plays)
    plays_used = max(0, total_plays - max(0, plays_left))
    expected_ratio = plays_used / total_plays
    if after_ratio > expected_ratio + 0.2:
        reward += 0.005

    return min(reward, 0.05)


def hand_setup_reward(prev_hand: int, new_hand: int, had_discard: bool) -> float:
    """
    棄牌後牌型改善獎勵（小型 shaping）
    """
    if not had_discard:
        return 0.0
    if prev_hand < 0 or new_hand < 0:
        return 0.0

    prev_strength = _HAND_STRENGTH_ORDER.get(prev_hand, -1)
    new_strength = _HAND_STRENGTH_ORDER.get(new_hand, -1)
    if prev_strength < 0 or new_strength < 0 or new_strength <= prev_strength:
        return 0.0

    diff = new_strength - prev_strength
    if diff <= 2:
        reward = 0.02
    elif diff <= 4:
        reward = 0.04
    else:
        reward = 0.06 + min(0.02, (diff - 5) * 0.01)

    return min(reward, 0.08)


class BuildTracker:
    """追蹤主導 build（pairs / straight / flush）"""

    def __init__(self) -> None:
        self.reset()

    def reset(self) -> None:
        self._counts = {BUILD_PAIRS: 0, BUILD_STRAIGHT: 0, BUILD_FLUSH: 0}
        self._total_hands = 0

    def record_hand(self, hand_type: int) -> None:
        self._total_hands += 1
        for build, hands in _BUILD_HANDS.items():
            if hand_type in hands:
                self._counts[build] += 1

    def get_build_weights(self) -> dict[str, float]:
        if self._total_hands == 0:
            return {"pairs": 1.0 / 3, "straight": 1.0 / 3, "flush": 1.0 / 3}
        return {
            "pairs": self._counts[BUILD_PAIRS] / self._total_hands,
            "straight": self._counts[BUILD_STRAIGHT] / self._total_hands,
            "flush": self._counts[BUILD_FLUSH] / self._total_hands,
        }

    def get_dominant_build(self) -> Optional[int]:
        if self._total_hands < 5:
            return None
        weights = self.get_build_weights()
        best = max(
            (BUILD_PAIRS, weights["pairs"]),
            (BUILD_STRAIGHT, weights["straight"]),
            (BUILD_FLUSH, weights["flush"]),
            key=lambda x: x[1],
        )
        return best[0] if best[1] >= 0.6 else None

    def joker_build_bonus(self, joker_id: int, joker_slots: int) -> float:
        dominant = self.get_dominant_build()
        if dominant is None:
            return 0.0
        support = JOKER_BUILD_SUPPORT.get(joker_id)
        if support is None:
            return 0.0
        if support == dominant:
            return 0.02 + 0.01 * min(joker_slots, 5) / 5.0
        return -0.02

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

    # Skip Blind 相關
    tag_id: int = -1  # -1 = 無

    # 消耗品相關
    consumable_id: int = -1  # -1 = 無（Tarot: 0-21, Planet: 22-33, Spectral: 34-51）

    # Joker 交易相關
    joker_sold_id: int = -1  # 賣出的 Joker ID (-1 = 無)
    best_shop_joker_cost: int = 0  # 商店中最強 Joker 的成本


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
        tag_id=info.get("tag_id", -1),
        consumable_id=info.get("consumable_id", -1),
        joker_sold_id=info.get("joker_sold_id", -1),
        best_shop_joker_cost=info.get("best_shop_joker_cost", 0),
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
    出牌獎勵：正規化到 0~0.15（v4.0 - 降低密集獎勵）

    設計原則：
    - 降低中間獎勵以突出終端獎勵
    - 仍保持正向獎勵以引導學習方向
    """
    if required <= 0 or score_gained <= 0:
        return 0.0

    ratio = score_gained / required

    if ratio >= 1.0:
        # 超額獎勵
        base = 0.12
        overkill_bonus = min((ratio - 1.0) * 0.03, 0.03)
        reward = base + overkill_bonus
    else:
        # 未達標：線性獎勵進度
        reward = ratio * 0.12

    return min(reward, 0.15)


def discard_reward(cards_discarded: int, discards_left: int) -> float:
    """
    棄牌獎勵：棄牌本身不應有正向獎勵

    - 空棄牌（cards_discarded==0）懲罰 -0.05（阻斷 no-op 漏洞）
    - 有棄牌：輕微懲罰 -0.01（消耗有限資源）

    Joker 連動效果（Yorick、Castle、Faceless 等）會通過後續的
    score_delta/money_delta 自動體現，AI 會學習「棄牌是手段而非目標」
    """
    if cards_discarded == 0:
        return -0.05  # 懲罰空棄牌（no-op exploit 防護）

    # 棄牌本身不應有正向獎勵，輕微懲罰消耗有限資源
    return -0.01


def blind_clear_reward(
    plays_left: int,
    blind_type: int,
    ante: int,
    boss_blind_id: Optional[int] = None
) -> float:
    """
    過關獎勵：正規化到 0.15~0.5（v4.0 - 階段里程碑）

    設計原則：
    - 過關獎勵隨 Ante 增加（後期過關更有價值）
    - Boss 過關獎勵最高（觸發 Ante 進度）
    - 適度獎勵以配合終端獎勵
    """
    # 基礎獎勵（v5.0 提升以確保 Clear > Skip）
    base = {
        BLIND_SMALL: 0.25,  # +0.10
        BLIND_BIG: 0.35,    # +0.15
        BLIND_BOSS: 0.50,   # +0.20
    }.get(blind_type, 0.25)

    # Boss 難度加成
    boss_bonus = 0.0
    if blind_type == BLIND_BOSS and boss_blind_id is not None:
        boss_bonus = 0.05

    # 效率獎勵（剩餘出牌次數）
    efficiency = plays_left * 0.01

    # Ante 階段權重（後期過關獎勵更高）
    # Ante 1: 1.0, Ante 8: 1.5
    ante_mult = 1.0 + (ante - 1) * 0.07

    return clamp((base + boss_bonus + efficiency) * ante_mult, 0.25, 0.75)


def ante_progress_reward(old_ante: int, new_ante: int) -> float:
    """
    Ante 進度獎勵：漸進式增長（0.48~2.27）

    設計原則（v5.0 - 平衡 Ante 進度）：
    - 漸進式增長：避免後期過度激進的獎勵
    - 公式：reward = 0.15 × a^1.5 + 0.1 × a
    - 累積獎勵：Ante 1→2: 0.48, 1→3: 0.97, 7→8: 2.27
    """
    def ante_value(a: int) -> float:
        # 漸進式增長：0.15 × a^1.5 + 0.1 × a
        # Ante 1: 0.25, Ante 2: 0.62, Ante 3: 1.08, ..., Ante 8: 4.19
        if a < 1:
            return 0.0
        return 0.15 * (a ** 1.5) + 0.1 * a

    return ante_value(new_ante) - ante_value(old_ante)


def game_end_reward(game_end: int, ante: int) -> float:
    """
    遊戲結束獎勵（v4.0 - 強化終端信號）

    設計原則：
    - 勝利獎勵大幅提升到 5.0（超過所有中間獎勵累積）
    - 失敗懲罰根據進度調整：-2.0（Ante 1 失敗）到 -0.5（Ante 8 失敗）
    - 這確保終端目標（勝利）是最重要的信號
    """
    if game_end == GAME_END_WIN:
        return 5.0  # 大幅提升勝利獎勵
    elif game_end == GAME_END_LOSE:
        # 進度越高，懲罰越小（鼓勵嘗試更高 Ante）
        progress = ante / 8.0
        # Ante 1 失敗: -2.0, Ante 8 失敗: -0.5
        return -2.0 + 1.5 * progress
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


def smooth_economic_penalty(cost_ratio: float) -> float:
    """
    平滑的經濟懲罰曲線

    使用對數函數確保平滑過渡，避免在任何點產生陡峭跳變。

    範圍：0 (cost_ratio=0) → ~0.10 (cost_ratio=1.0)
    - cost_ratio=0.25: ~0.03
    - cost_ratio=0.50: ~0.05
    - cost_ratio=0.75: ~0.06
    - cost_ratio=1.00: ~0.07
    """
    if cost_ratio <= 0:
        return 0.0
    # log1p(x) = log(1+x)，確保 x=0 時結果為 0
    return 0.05 * math.log1p(cost_ratio * 3)


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
    - 經濟懲罰（成本占比，使用平滑曲線）
    - 利息損失
    - 槽位壓力
    - 階段權重
    """
    # 是否成功購買
    if joker_count_after <= joker_count_before:
        return 0.0

    # Joker 價值估算（基於成本和可選的 joker_id）
    joker_value = estimate_joker_value(cost, joker_id)

    # 平滑經濟懲罰（使用對數函數）
    cost_ratio = min(cost / money_before, 1.0) if money_before > 0 else 1.0
    economic_penalty = smooth_economic_penalty(cost_ratio)

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
    # 機會成本（跳過 Blind 放棄的獎金 + 過關獎勵差距 + 商店訪問價值）
    # v5.0: 提高成本以反映商店機會損失
    opportunity_cost = {
        BLIND_SMALL: 0.18,  # 放棄 $3 + 過關獎勵 + 商店訪問價值
        BLIND_BIG: 0.25,    # 放棄 $5 + 更高分數獎勵 + 商店訪問
        BLIND_BOSS: 2.0,    # 實質禁止跳過 Boss
    }.get(blind_type, 0.18)

    # 風險調整（v5.0: 早期更保守以累積 Joker）
    risk_adjustments = {
        1: 0.7, 2: 0.8,     # 早期：需要商店累積 Joker，更保守
        3: 0.9, 4: 1.0,     # 中期：可以適度 skip
        5: 0.85, 6: 0.7,    # 中後期：趨於保守
        7: 0.5, 8: 0.3,     # 後期：幾乎不該 skip
    }
    risk_adjustment = risk_adjustments.get(ante, 0.7)

    # Tag 價值（使用具體值或平均值）
    tag_value = get_tag_value(tag_id)

    reward = (tag_value * risk_adjustment) - opportunity_cost
    # v5.0: 上限低於 Clear 最小獎勵 0.25，確保 Clear 優於 Skip
    return clamp(reward, -0.20, 0.25)


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
    joker_slot_limit: int,
    joker_sold_id: int = -1
) -> float:
    """
    出售 Joker 獎勵（-0.2~0.2）

    Args:
        money_gained: 賣出獲得的金幣
        ante: 當前 Ante
        joker_count_before: 賣出前的 Joker 數量
        joker_slot_limit: Joker 槽位上限
        joker_sold_id: 賣出的 Joker ID（用於精確評估損失）
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

    # 根據 Joker ID 估算損失（基於稀有度）
    if joker_sold_id >= 0:
        # 使用賣價估算稀有度（賣價約為成本的一半）
        estimated_cost = money_gained * 2
        rarity = estimate_joker_rarity_from_cost(estimated_cost)
        loss_penalty = JOKER_RARITY_VALUES.get(rarity, 0.08)
    else:
        # 未知 Joker，使用平均損失
        loss_penalty = 0.08

    reward = (money_value + slot_pressure_bonus - loss_penalty) * stage_mult
    return clamp(reward, -0.2, 0.2)


# 消耗品類型範圍（對應 Rust consumables.rs 的 to_global_index）
TAROT_COUNT = 22
PLANET_COUNT = 12
SPECTRAL_COUNT = 18


def consumable_use_reward(ante: int, consumable_id: int = -1) -> float:
    """
    消耗品使用獎勵（0~0.25）

    根據消耗品類型給予不同獎勵：
    - Spectral (34-51): 0.18 基礎（效果最強，包含稀有轉換）
    - Planet (22-33): 0.12 基礎（穩定的牌型升級）
    - Tarot (0-21): 0.10 基礎（基礎卡片增強）

    Args:
        ante: 當前 Ante
        consumable_id: 消耗品全域 ID（-1 表示未知，使用平均值）
    """
    # 根據 consumable_id 確定類型和基礎獎勵
    if consumable_id < 0:
        # 未知類型，使用平均值
        base_value = 0.12
    elif consumable_id < TAROT_COUNT:
        # Tarot (0-21)
        base_value = 0.10
    elif consumable_id < TAROT_COUNT + PLANET_COUNT:
        # Planet (22-33)
        base_value = 0.12
    else:
        # Spectral (34-51)
        base_value = 0.18

    # 階段調整（消耗品後期價值更高）
    stage_mult = stage_weight_late(ante)

    return clamp(base_value * stage_mult, 0.0, 0.25)


def voucher_buy_reward(cost: int, money_before: int, ante: int) -> float:
    """
    Voucher 購買獎勵（-0.25~0.3）

    簡化版，使用平滑經濟懲罰曲線
    """
    # 平滑經濟懲罰（Voucher 略高於 Joker，因為 Voucher 成本固定 $10）
    cost_ratio = min(cost / money_before, 1.0) if money_before > 0 else 1.0
    economic_penalty = smooth_economic_penalty(cost_ratio) * 1.2  # Voucher 懲罰係數略高

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
        self._build_tracker = BuildTracker()

    def reset(self):
        """重置內部狀態"""
        self._prev_info = None
        self._build_tracker.reset()

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
            if prev is not None:
                total_plays = max(1, prev.plays_left + 1)
                reward += blind_progress_signal(
                    prev.chips,
                    info.chips,
                    info.blind_target,
                    info.plays_left,
                    total_plays=total_plays,
                )
            if info.hand_type >= 0:
                self._build_tracker.record_hand(info.hand_type)

        elif action_type == ACTION_TYPE_DISCARD:
            # 使用實際棄牌數量（來自 Rust 端）
            reward += discard_reward(info.cards_discarded, info.discards_left)

        elif action_type == ACTION_TYPE_BUY_JOKER:
            # Fix: 檢查購買是否成功（joker_count 增加且 money 減少）
            # 防止失敗的購買嘗試不受懲罰（reward hacking 防護）
            if prev is not None:
                if info.joker_count > prev.joker_count and info.money_delta < 0:
                    reward += joker_buy_reward(
                        info.last_action_cost,
                        prev.money,
                        info.ante,
                        prev.joker_count,
                        info.joker_count,
                        info.joker_slot_limit
                    )
                elif info.joker_count <= prev.joker_count:
                    # 購買失敗：懲罰無效動作
                    reward -= 0.05

        elif action_type == ACTION_TYPE_SELL_JOKER:
            if prev is not None:
                # 直接使用 money_delta（Rust 端已計算）和 joker_sold_id
                reward += sell_joker_reward(
                    info.money_delta,
                    info.ante,
                    prev.joker_count,
                    info.joker_slot_limit,
                    info.joker_sold_id
                )

        elif action_type == ACTION_TYPE_REROLL:
            reward += reroll_reward(
                info.last_action_cost,
                prev.money if prev else info.money,
                info.ante
            )

        elif action_type == ACTION_TYPE_SKIP_BLIND:
            # 使用 tag_id 精確計算 Tag 價值（-1 表示無 Tag，使用平均值）
            tag_id_or_none = info.tag_id if info.tag_id >= 0 else None
            reward += skip_blind_reward(info.blind_type, info.ante, tag_id_or_none)

        elif action_type == ACTION_TYPE_USE_CONSUMABLE:
            # 使用 consumable_id 精確計算獎勵（-1 表示未知，使用平均值）
            reward += consumable_use_reward(info.ante, info.consumable_id)

        elif action_type == ACTION_TYPE_BUY_VOUCHER:
            # Fix: 檢查購買是否成功（cost > 0 且 money 減少）
            # 防止失敗的購買嘗試獲得正向獎勵（reward hacking 防護）
            if prev is not None and info.last_action_cost > 0 and info.money_delta < 0:
                reward += voucher_buy_reward(
                    info.last_action_cost,
                    prev.money,
                    info.ante
                )
            elif info.last_action_cost == 0:
                # 購買失敗：懲罰無效動作
                reward -= 0.05

        elif action_type == ACTION_TYPE_BUY_PACK:
            # Fix: 檢查購買是否成功（cost > 0 且 money 減少）
            # 防止失敗的購買嘗試獲得正向獎勵
            if info.last_action_cost > 0 and info.money_delta < 0:
                reward += 0.05
            elif info.last_action_cost == 0:
                # 購買失敗：懲罰無效動作
                reward -= 0.03

        elif action_type == ACTION_TYPE_CASH_OUT:
            # Cash out 後給予金幣狀態獎勵
            reward += money_reward(info.money, info.ante)

        elif action_type == ACTION_TYPE_NEXT_ROUND:
            # 檢查是否進入新 Ante
            if prev is not None and info.ante > prev.ante:
                reward += ante_progress_reward(prev.ante, info.ante)

        # 通用 no-op 偵測：任何未產生狀態變化的動作施加小懲罰
        # 防止 agent 利用 SELECT/DISCARD/PLAY 的 no-op 漏洞
        if (action_type in (ACTION_TYPE_SELECT, ACTION_TYPE_DISCARD, ACTION_TYPE_PLAY)
                and info.score_delta == 0
                and info.money_delta == 0
                and info.cards_played == 0
                and info.cards_discarded == 0
                and not info.blind_cleared
                and info.game_end == GAME_END_NONE):
            reward -= 0.03

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
