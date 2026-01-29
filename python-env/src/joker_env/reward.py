"""
獎勵計算系統 - Python 端

從 Rust reward.rs 移植，為 RL 訓練提供形狀良好的獎勵信號，支持完整遊戲（Ante 1-8）

獎勵範圍設計（v6.4 - 強化牌型選擇）：
設計原則：
- 終端獎勵主導：勝利=5.0，確保長期目標壓過短期收益
- Joker 保護機制：低 Joker 數量時嚴禁賣出，持有 Joker 給予獎勵
- 牌型品質獎勵：大幅強化牌型差距（3.5x），解決 95% High Card/Pair 問題
- 棄牌改善獎勵：啟用 hand_setup_reward，鼓勵有目的的棄牌
- 效率獎勵：早期過關給予額外獎勵

v6.4 核心修復（針對 95% High Card/Pair 問題）：
1. HAND_TYPE_BONUSES 放大 3.5 倍（對抗 VecNormalize 壓縮）
2. 啟用 hand_setup_reward()（原本存在但未被調用）
3. 棄牌反循環保護：連續棄牌 > 2 次給予累進懲罰
4. 效率獎勵：剩餘出牌次數越多過關，獎勵越高
5. Pair 獎勵降為 0（基線），不再鼓勵安全策略

v6.3 強化 Boss 獎勵（針對 Boss Clear 0% 瓶頸）：
1. Boss 基礎獎勵從 0.50 提升到 0.80
2. Boss 額外加成從 0.05 提升到 0.10
3. 效率獎勵從 0.01/play 提升到 0.02/play
4. Boss 階段每個動作 +0.0001（鼓勵積極面對）

v6.0-6.2 歷史修復：
- sell_joker_reward: Joker <= 2 時賣出重罰
- joker_holding_bonus: 持有 Joker 給予獎勵
- 超額獎勵使用對數縮放

| 模組                     | 範圍             | 說明                              |
|--------------------------|------------------|-----------------------------------|
| 遊戲結束 (game_end)      | -2.0 ~ 5.0       | 勝利=5.0，失敗依進度懲罰          |
| Ante 進度                | 0.56 ~ 1.76+里程碑| 更陡峭曲線 + 里程碑獎勵          |
| 過關 (blind_clear)       | 0.25 ~ 1.50      | Ante 係數 0.15，後期更高          |
| 出牌 (play_reward)       | -0.05 ~ 0.35     | 牌型獎勵 + 效率獎勵（v6.4）       |
| 棄牌 (discard_reward)    | -0.07 ~ 0.06     | 改善獎勵 - 反循環懲罰（v6.4）     |
| 購買 Joker               | -0.3 ~ 0.5       | 早期購買加倍獎勵                  |
| Skip Blind/Tag           | -0.20 ~ 0.25     | 提高機會成本，調整風險係數        |
| 消耗品使用               | 0.0 ~ 0.25       | Spectral 後期乘數更強             |
| 金幣狀態 (money_reward)  | 0.0 ~ 0.2        | 利息閾值階梯獎勵                  |
| Joker 持有獎勵           | -0.05 ~ 0.08     | 持有 Joker 給予獎勵               |
| Reroll 決策              | -0.15 ~ 0.0      | 考慮利息損失（純經濟懲罰）        |
| 出售 Joker               | -0.5 ~ 0.1       | 低數量嚴罰，僅滿槽可正向          |
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

# v6.5: 牌型品質獎勵 — 與過關獎勵對齊
# 核心思路：Four Kind (+0.60) 應接近 Boss 過關 (+0.80)，因為打出 Four Kind 幾乎等於過關
# Pair 改為小懲罰，不再是安全基線
HAND_TYPE_BONUSES = {
    HAND_HIGH_CARD: -0.08,        # 加重懲罰（原 -0.05）
    HAND_PAIR: -0.02,             # 小懲罰，不再是安全基線（原 0.00）
    HAND_TWO_PAIR: 0.05,          # 略微正向
    HAND_THREE_KIND: 0.20,        # 2x 放大（原 0.10）
    HAND_STRAIGHT: 0.28,          # 2x 放大（原 0.14）
    HAND_FLUSH: 0.32,             # 2x 放大（原 0.16）
    HAND_FULL_HOUSE: 0.40,        # 2x 放大（原 0.20）
    HAND_FOUR_KIND: 0.60,         # 關鍵！接近 Boss 過關（原 0.28）
    HAND_STRAIGHT_FLUSH: 0.75,    # 超高獎勵（原 0.35）
    HAND_ROYAL_FLUSH: 0.85,       # 頂級獎勵（原 0.42）
    HAND_FIVE_KIND: 0.75,         # 同 Straight Flush（原 0.35）
    HAND_FLUSH_HOUSE: 0.85,       # 同 Royal Flush（原 0.42）
    HAND_FLUSH_FIVE: 1.00,        # 最強牌型，超過 Boss 過關（原 0.52）
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
    棄牌後牌型改善/變差獎勵（v6.4 修改：加入變差懲罰）

    - 改善：+0.02 ~ +0.08（根據提升幅度）
    - 不變：0
    - 變差：-0.01（輕微懲罰，避免過度保守）
    """
    if not had_discard:
        return 0.0
    if prev_hand < 0 or new_hand < 0:
        return 0.0

    prev_strength = _HAND_STRENGTH_ORDER.get(prev_hand, -1)
    new_strength = _HAND_STRENGTH_ORDER.get(new_hand, -1)
    if prev_strength < 0 or new_strength < 0:
        return 0.0

    # v6.4: 變差懲罰
    if new_strength < prev_strength:
        return -0.01  # 輕微懲罰，避免阻礙策略性棄牌

    # 不變
    if new_strength == prev_strength:
        return 0.0

    # 改善獎勵
    diff = new_strength - prev_strength
    if diff <= 2:
        reward = 0.02
    elif diff <= 4:
        reward = 0.04
    else:
        reward = 0.06 + min(0.02, (diff - 5) * 0.01)

    return min(reward, 0.08)


def potential_change_reward(
    old_flush: float, old_straight: float, old_pairs: float,
    new_flush: float, new_straight: float, new_pairs: float
) -> float:
    """
    v6.4: 手牌潛力變化獎勵（Potential-Based Shaping）

    計算手牌潛力的綜合變化，獎勵提升潛力的棄牌決策。

    潛力權重（根據 Balatro 分數）：
    - Flush: 0.40（Flush 分數高且相對容易達成）
    - Straight: 0.35（Straight 分數高但較難達成）
    - Pairs: 0.25（Pairs 是保底選項）

    返回範圍：約 -0.05 ~ +0.05
    """
    # 加權計算綜合潛力
    old_potential = 0.40 * old_flush + 0.35 * old_straight + 0.25 * old_pairs
    new_potential = 0.40 * new_flush + 0.35 * new_straight + 0.25 * new_pairs

    # 潛力差分，縮放係數 0.1（避免過度主導）
    delta = new_potential - old_potential
    return delta * 0.1  # 範圍約 -0.05 ~ +0.05


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

    # v6.4: 手牌潛力指標（從 Rust 計算）
    flush_potential: float = 0.0    # 同花潛力 [0, 1]
    straight_potential: float = 0.0  # 順子潛力 [0, 1]
    pairs_potential: float = 0.0     # 對子潛力 [0, 1]


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
        # v6.4: 手牌潛力指標
        flush_potential=info.get("flush_potential", 0.0),
        straight_potential=info.get("straight_potential", 0.0),
        pairs_potential=info.get("pairs_potential", 0.0),
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

def hand_type_bonus(hand_type: int, ante: int) -> float:
    """
    牌型品質獎勵（v6.0 新增）

    鼓勵打出更強牌型，而非一直打 Pair
    - High Card: -0.01（輕微懲罰）
    - Pair: 0.0（基線）
    - Three Kind+: 正向獎勵
    - 後期 Ante 需要更強牌型
    """
    if hand_type < 0:
        return 0.0
    base = HAND_TYPE_BONUSES.get(hand_type, 0.0)
    # 後期 Ante 牌型獎勵更高
    ante_mult = 1.0 + (ante - 1) * 0.1
    return base * ante_mult


def play_reward(score_gained: int, required: int, hand_type: int = -1, ante: int = 1, plays_left: int = 3) -> float:
    """
    出牌獎勵：正規化到 -0.05~0.35（v6.4 - 加入效率獎勵）

    設計原則：
    - 基礎獎勵 +0.02：鼓勵模型嘗試出牌
    - 進度獎勵：根據得分比例給予額外獎勵
    - 牌型獎勵：打出更強牌型給予額外獎勵（v6.4 放大 3.5x）
    - 低分懲罰：得分低於預期節奏時懲罰
    - 效率獎勵：剩餘出牌次數越多過關，獎勵越高（v6.4 新增）
    """
    # 基礎出牌獎勵
    base_play_bonus = 0.02

    if required <= 0:
        return base_play_bonus

    # 低分懲罰 — 如果得分遠低於應有節奏
    # 預期每次出牌應達到 (required / 4) 的分數
    expected_per_play = required / 4.0
    if score_gained <= 0:
        # 0 分是嚴重問題
        return -0.03
    elif score_gained < expected_per_play * 0.3:
        # 得分低於預期的 30%：懲罰
        low_score_penalty = -0.02
    elif score_gained < expected_per_play * 0.5:
        # 得分低於預期的 50%：輕微懲罰
        low_score_penalty = -0.01
    else:
        low_score_penalty = 0.0

    ratio = score_gained / required

    if ratio >= 1.0:
        # 超額獎勵（使用對數縮放）
        base = 0.12
        overkill_bonus = 0.05 * math.log1p(ratio - 1.0)
        progress_reward = base + overkill_bonus
    else:
        # 未達標：線性獎勵進度
        progress_reward = ratio * 0.12

    # 牌型品質獎勵（v6.4: 放大 3.5x）
    type_bonus = hand_type_bonus(hand_type, ante)

    # v6.4: 效率獎勵 — 剩餘出牌次數越多過關，獎勵越高
    # 鼓勵早期就打出高分，而非拖到最後一手
    efficiency_bonus = 0.0
    if ratio >= 1.0 and plays_left >= 2:
        # 還剩 2+ 次出牌就過關 = 效率獎勵
        # plays_left=2: +0.02, plays_left=3: +0.04, plays_left=4: +0.06
        efficiency_bonus = 0.02 * (plays_left - 1)

    reward = base_play_bonus + progress_reward + type_bonus + low_score_penalty + efficiency_bonus
    return clamp(reward, -0.10, 1.00)  # v6.5: 上限提高到 1.0（配合強牌型獎勵）


def discard_reward(cards_discarded: int, discards_left: int) -> float:
    """
    棄牌獎勵：加重棄牌成本以防止「棄牌循環」（v5.1）

    - 空棄牌（cards_discarded==0）懲罰 -0.05（阻斷 no-op 漏洞）
    - 有棄牌：懲罰 -0.03（加重成本，原 -0.01 太輕導致過度棄牌）

    設計原則：
    - 棄牌成本需高於「不確定性規避」的心理收益
    - Joker 連動效果會通過 score_delta/money_delta 自動體現
    - 配合 play_reward 的 +0.02 基礎獎勵，形成「出牌優於棄牌」的激勵
    """
    if cards_discarded == 0:
        return -0.05  # 懲罰空棄牌（no-op exploit 防護）

    # 加重棄牌成本，打破棄牌循環
    return -0.02


def blind_clear_reward(
    plays_left: int,
    blind_type: int,
    ante: int,
    boss_blind_id: Optional[int] = None
) -> float:
    """
    過關獎勵：強化 Boss 獎勵（v6.3）

    設計原則：
    - Boss 過關獎勵大幅提升（是 Small 的 4 倍）
    - 過關獎勵隨 Ante 顯著增加
    - Ante 3+ 的過關獎勵明顯高於早期

    v6.3 修改：Boss 基礎獎勵從 0.50 提升到 0.80
    """
    # 基礎獎勵（v6.3: Boss 大幅提升）
    base = {
        BLIND_SMALL: 0.20,   # 略降
        BLIND_BIG: 0.30,     # 略降
        BLIND_BOSS: 0.80,    # 大幅提升（原 0.50）
    }.get(blind_type, 0.20)

    # Boss 難度加成
    boss_bonus = 0.0
    if blind_type == BLIND_BOSS:
        boss_bonus = 0.10  # 提高（原 0.05）

    # 效率獎勵（剩餘出牌次數）
    efficiency = plays_left * 0.02  # 提高（原 0.01）

    # Ante 階段權重（v5.2: 更陡峭的增長）
    # Ante 1: 1.0, Ante 3: 1.3, Ante 5: 1.6, Ante 8: 2.05
    ante_mult = 1.0 + (ante - 1) * 0.15

    return clamp((base + boss_bonus + efficiency) * ante_mult, 0.20, 1.50)


def ante_progress_reward(old_ante: int, new_ante: int) -> float:
    """
    Ante 進度獎勵：強化中後期增長（v5.2）

    設計原則：
    - 使用更陡峭的曲線讓中後期獎勵更明顯
    - 公式：reward = 0.12 × a^2 + 0.08 × a
    - 累積獎勵：1→2: 0.56, 2→3: 0.80, 3→4: 1.04, 7→8: 1.76
    - 加入里程碑獎勵：Ante 3/5/7 額外 +0.3/+0.5/+0.8
    """
    def ante_value(a: int) -> float:
        # 更陡峭的曲線：0.12 × a^2 + 0.08 × a
        if a < 1:
            return 0.0
        return 0.12 * (a ** 2) + 0.08 * a

    base_reward = ante_value(new_ante) - ante_value(old_ante)

    # 里程碑獎勵：鼓勵突破關鍵 Ante
    milestone_bonus = 0.0
    if old_ante < 3 <= new_ante:
        milestone_bonus += 0.3  # 首次進入 Ante 3
    if old_ante < 5 <= new_ante:
        milestone_bonus += 0.5  # 首次進入 Ante 5
    if old_ante < 7 <= new_ante:
        milestone_bonus += 0.8  # 首次進入 Ante 7

    return base_reward + milestone_bonus


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
    購買 Joker 獎勵（-0.3~0.5）（v6.0 - 提高早期購買價值）

    考慮因素：
    - Joker 價值（基於成本估算稀有度）
    - 早期購買獎勵加倍（v6.0 核心修復）
    - 經濟懲罰（成本占比，使用平滑曲線）
    - 利息損失
    - 槽位壓力
    """
    # 是否成功購買
    if joker_count_after <= joker_count_before:
        return 0.0

    # Joker 價值估算（基於成本和可選的 joker_id）
    joker_value = estimate_joker_value(cost, joker_id)

    # v6.0: 早期購買價值大幅提升 — 這是核心修復
    # 前 2 個 Joker 是生存必需品
    if ante <= 2 and joker_count_after <= 2:
        joker_value *= 2.5  # 首批 Joker 價值提升 150%
    elif ante <= 3 and joker_count_after <= 3:
        joker_value *= 1.8  # 前 3 個 Joker 價值提升 80%
    elif ante <= 4 and joker_count_after <= 4:
        joker_value *= 1.3  # 額外 Joker 小幅提升

    # 平滑經濟懲罰（使用對數函數）— 但早期懲罰降低
    cost_ratio = min(cost / money_before, 1.0) if money_before > 0 else 1.0
    economic_penalty = smooth_economic_penalty(cost_ratio)
    if ante <= 2:
        economic_penalty *= 0.5  # 早期經濟懲罰減半

    # 利息損失（早期忽略）
    money_after = money_before - cost
    interest_before = min(money_before // 5, 5)
    interest_after = min(money_after // 5, 5)
    if ante <= 2:
        interest_loss = 0.0  # 早期不考慮利息
    else:
        interest_loss = 0.02 * (interest_before - interest_after) if interest_after < interest_before else 0.0

    # 階段權重（早期購買 Joker 更有價值）
    stage_mult = stage_weight_early(ante)

    # 槽位考量：接近滿槽時需要更好的 Joker 才值得
    slot_ratio = joker_count_after / joker_slot_limit
    slot_penalty = 0.03 * slot_ratio if slot_ratio > 0.8 else 0.0

    # 獎勵計算：Joker 價值 × 階段權重 - 各種懲罰
    reward = joker_value * stage_mult - economic_penalty - interest_loss - slot_penalty
    return clamp(reward, -0.3, 0.5)  # 上限提高到 0.5


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
    出售 Joker 獎勵（-0.5~0.1）（v6.0 - 嚴格控制賣出）

    核心原則（v6.0 重大修改）：
    - Joker 數量 <= 2 時賣出 = 重罰（這是致命錯誤）
    - 只有槽位真正滿且是弱 Joker 時才可能正向
    - 打破「賣 Joker 換錢」的套利迴路

    Args:
        money_gained: 賣出獲得的金幣
        ante: 當前 Ante
        joker_count_before: 賣出前的 Joker 數量
        joker_slot_limit: Joker 槽位上限
        joker_sold_id: 賣出的 Joker ID（用於精確評估損失）
    """
    # v6.0 核心修復：低 Joker 數量保護機制
    # 這是最關鍵的修復 — 嚴禁在 Joker 不足時賣出
    if joker_count_before <= 1:
        return -0.5  # 嚴重懲罰：只有 1 個還賣，幾乎是自殺行為
    if joker_count_before <= 2:
        return -0.3  # 重罰：只有 2 個還賣，高風險行為
    if joker_count_before <= 3 and ante <= 3:
        return -0.2  # 中等懲罰：早期 3 個也不該賣

    # 只有真正滿槽才有正向可能
    if joker_count_before >= joker_slot_limit:
        slot_bonus = 0.05  # 降低（原 0.08），滿槽時賣出勉強接受
    elif joker_count_before >= joker_slot_limit - 1:
        slot_bonus = 0.0   # 接近滿也不獎勵（原 0.03）
    else:
        slot_bonus = -0.08  # 未滿槽賣出 = 懲罰

    # 金幣收益價值（降低，打破套利動機）
    money_value = min(money_gained / 20.0, 0.05)  # 原 /12, max 0.1

    # 損失懲罰（提高）
    if joker_sold_id >= 0:
        estimated_cost = money_gained * 2
        rarity = estimate_joker_rarity_from_cost(estimated_cost)
        loss_penalty = JOKER_RARITY_VALUES.get(rarity, 0.08) * 1.5  # 提高 50%
    else:
        loss_penalty = 0.12  # 未知 Joker 懲罰更高（原 0.08）

    # 階段調整：早期賣出懲罰加重
    if ante <= 2:
        stage_mult = 1.5  # 早期懲罰加重 50%
    elif ante <= 4:
        stage_mult = 1.2  # 中期懲罰加重 20%
    else:
        stage_mult = 1.0  # 後期正常

    reward = (slot_bonus + money_value - loss_penalty) * stage_mult
    return clamp(reward, -0.5, 0.1)  # 範圍調整：下限 -0.5，上限 0.1


# 消耗品類型範圍（對應 Rust consumables.rs 的 to_global_index）
TAROT_COUNT = 22
PLANET_COUNT = 12
SPECTRAL_COUNT = 18


# ============================================================================
# v6.0 新增：Joker 持有與不足獎懲
# ============================================================================

def joker_holding_bonus(joker_count: int, ante: int) -> float:
    """
    持有 Joker 獎勵（v6.0 新增，在 CASH_OUT 時給予）

    核心原則：持有 Joker 是正確策略，需要直接獎勵
    - 0 個 Joker：懲罰
    - 每個 Joker：給予小獎勵
    - 早期持有更有價值

    Args:
        joker_count: 當前持有的 Joker 數量
        ante: 當前 Ante
    """
    if joker_count == 0:
        # 懲罰：0 個 Joker 進入商店是危險信號
        if ante <= 2:
            return -0.08  # 早期 0 Joker 更嚴重
        return -0.05

    # 每個 Joker 給予小獎勵
    base_bonus = joker_count * 0.015

    # 早期持有更有價值
    if ante <= 2:
        stage_mult = 1.5
    elif ante <= 4:
        stage_mult = 1.2
    else:
        stage_mult = 1.0

    return min(base_bonus * stage_mult, 0.08)


def joker_shortage_penalty(joker_count: int, ante: int) -> float:
    """
    Joker 不足懲罰（v6.0 新增，每步檢查）

    持續施加壓力：Joker 數量低於 Ante 要求時給予小懲罰
    這創造了持續的「購買 Joker」動機

    Args:
        joker_count: 當前持有的 Joker 數量
        ante: 當前 Ante
    """
    # 每個 Ante 的最低 Joker 要求
    min_jokers = {1: 1, 2: 2, 3: 2, 4: 3, 5: 3, 6: 3, 7: 3, 8: 3}
    required = min_jokers.get(ante, 2)

    shortage = max(0, required - joker_count)
    if shortage == 0:
        return 0.0

    # 持續小懲罰：每缺 1 個 = -0.01
    return -0.01 * shortage


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
        # v6.4: 棄牌改善獎勵狀態追蹤
        self._prev_hand_type: int = -1  # 上一次的最佳牌型
        self._consecutive_discards: int = 0  # 連續棄牌計數器（反循環保護）
        # v6.4: 手牌潛力追蹤
        self._prev_flush_potential: float = 0.0
        self._prev_straight_potential: float = 0.0
        self._prev_pairs_potential: float = 0.0

    def reset(self):
        """重置內部狀態"""
        self._prev_info = None
        self._build_tracker.reset()
        # v6.4: 重置棄牌追蹤狀態
        self._prev_hand_type = -1
        self._consecutive_discards = 0
        self._prev_flush_potential = 0.0
        self._prev_straight_potential = 0.0
        self._prev_pairs_potential = 0.0

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
            # v6.4: 過關後重置棄牌追蹤狀態
            self._consecutive_discards = 0
            self._prev_hand_type = -1
            self._prev_flush_potential = 0.0
            self._prev_straight_potential = 0.0
            self._prev_pairs_potential = 0.0

        # 根據動作類型計算獎勵
        elif action_type == ACTION_TYPE_PLAY:
            if info.score_delta > 0:
                # v6.4: 傳入 hand_type, ante, plays_left 以計算牌型+效率獎勵
                reward += play_reward(
                    info.score_delta,
                    info.blind_target,
                    hand_type=info.hand_type,
                    ante=info.ante,
                    plays_left=info.plays_left
                )
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

            # v6.4: 出牌後重置棄牌追蹤狀態
            self._consecutive_discards = 0
            self._prev_hand_type = -1
            self._prev_flush_potential = 0.0
            self._prev_straight_potential = 0.0
            self._prev_pairs_potential = 0.0

        elif action_type == ACTION_TYPE_DISCARD:
            # 使用實際棄牌數量（來自 Rust 端）
            base_discard_penalty = discard_reward(info.cards_discarded, info.discards_left)
            reward += base_discard_penalty

            # v6.4: 棄牌改善獎勵（帶反循環保護）
            self._consecutive_discards += 1

            # 初始化追蹤狀態（首次棄牌時從前一步獲取）
            if self._prev_hand_type < 0 and prev is not None:
                self._prev_hand_type = prev.hand_type
                self._prev_flush_potential = prev.flush_potential
                self._prev_straight_potential = prev.straight_potential
                self._prev_pairs_potential = prev.pairs_potential

            if self._consecutive_discards <= 2:
                # 只獎勵前 2 次連續棄牌
                # 1. 牌型改善/變差獎勵
                setup_bonus = hand_setup_reward(
                    self._prev_hand_type,
                    info.hand_type,
                    had_discard=(info.cards_discarded > 0)
                )
                reward += setup_bonus

                # 2. v6.4: 潛力變化獎勵（Potential-Based Shaping）
                potential_bonus = potential_change_reward(
                    self._prev_flush_potential,
                    self._prev_straight_potential,
                    self._prev_pairs_potential,
                    info.flush_potential,
                    info.straight_potential,
                    info.pairs_potential
                )
                reward += potential_bonus
            else:
                # 第 3 次及之後連續棄牌：額外懲罰（防止棄牌循環）
                reward -= 0.02 * (self._consecutive_discards - 2)

            # 更新手牌追蹤狀態
            self._prev_hand_type = info.hand_type
            self._prev_flush_potential = info.flush_potential
            self._prev_straight_potential = info.straight_potential
            self._prev_pairs_potential = info.pairs_potential

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
            # v6.0: Joker 持有獎勵 — 鼓勵保留 Joker
            reward += joker_holding_bonus(info.joker_count, info.ante)

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

        # v6.0: Joker 不足懲罰 — 持續施加壓力
        # 只在商店階段檢查（避免戰鬥中干擾）
        if info.stage == STAGE_SHOP:
            reward += joker_shortage_penalty(info.joker_count, info.ante)

        # v6.3: Boss 階段動作獎勵 — 鼓勵積極面對 Boss
        # 只有 PLAY 和 DISCARD 才給獎勵（SELECT 不算）
        if (info.stage == STAGE_BLIND
            and info.blind_type == BLIND_BOSS
            and action_type in (ACTION_TYPE_PLAY, ACTION_TYPE_DISCARD)):
            reward += 0.0001

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
