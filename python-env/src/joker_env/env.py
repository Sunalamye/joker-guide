from __future__ import annotations

from typing import Any, Dict, Tuple, Optional
from dataclasses import dataclass, field
from collections import defaultdict

import gymnasium as gym
import numpy as np
from gymnasium import spaces

from joker_env.client import JokerEnvClient
from joker_env.reward import RewardCalculator


# ============================================================================
# 常量定義 (需與 Rust 端保持一致)
# ============================================================================

HAND_SIZE = 8  # 手牌數量
MAX_SELECTED = 5  # 最多選擇 5 張打出
JOKER_SLOTS = 5
SHOP_JOKER_COUNT = 2
CONSUMABLE_SLOT_COUNT = 2
SHOP_VOUCHER_COUNT = 1
SHOP_PACK_COUNT = 2

# Observation 常量
SCALAR_COUNT = 32  # 標量特徵數（需與 Rust 端一致）
SELECTION_FEATURES = HAND_SIZE
CARD_BASE_FEATURES = 17  # 13 rank + 4 suit
CARD_ENHANCE_FEATURES = 4  # enhancement, seal, edition, face_down
CARD_FEATURES = CARD_BASE_FEATURES + CARD_ENHANCE_FEATURES  # 21
HAND_FEATURES = HAND_SIZE * CARD_FEATURES
HAND_TYPE_COUNT = 13  # 含進階牌型: FiveKind, FlushHouse, FlushFive
DECK_FEATURES = 52

# Joker 特徵: id (150 one-hot) + enabled (1) + eternal (1) + negative (1) = 153 per joker
JOKER_ID_SIZE = 150
JOKER_SINGLE_FEATURES = JOKER_ID_SIZE + 3  # 153
JOKER_FEATURES = JOKER_SLOTS * JOKER_SINGLE_FEATURES  # 765

# Shop: id (150 one-hot) + cost (1) = 151 per item
SHOP_SINGLE_FEATURES = JOKER_ID_SIZE + 1  # 151
SHOP_FEATURES = SHOP_JOKER_COUNT * SHOP_SINGLE_FEATURES  # 302

# 新增觀察空間
BOSS_BLIND_COUNT = 27
DECK_TYPE_FEATURES = 16
STAKE_FEATURES = 8
VOUCHER_FEATURES = 36
CONSUMABLE_FEATURES = 52  # Tarot(22) + Planet(12) + Spectral(18)
TAG_FEATURES = 25

# 計算總觀察空間大小
TOTAL_OBS_SIZE = (
    SCALAR_COUNT
    + SELECTION_FEATURES
    + HAND_FEATURES
    + HAND_TYPE_COUNT
    + DECK_FEATURES
    + JOKER_FEATURES
    + SHOP_FEATURES
    + BOSS_BLIND_COUNT
    + DECK_TYPE_FEATURES
    + STAKE_FEATURES
    + VOUCHER_FEATURES
    + CONSUMABLE_SLOT_COUNT * CONSUMABLE_FEATURES
    + TAG_FEATURES
)

# Action 類型 (13 種)
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

ACTION_TYPE_COUNT = 13

# Action mask layout (與 Rust 端一致)
# [0..13]: Action types (13)
# [13..29]: Card selection (16 = HAND_SIZE * 2)
# [29..32]: Blind selection (3)
# [32..34]: Shop joker purchase (2)
# [34..39]: Sell joker slots (5)
# [39]: Reroll (1)
# [40]: Skip Blind (1)
# [41..43]: Use consumable (2)
# [43]: Buy voucher (1)
# [44..46]: Buy pack (2)
ACTION_MASK_SIZE = (
    ACTION_TYPE_COUNT
    + HAND_SIZE * 2
    + 3
    + SHOP_JOKER_COUNT
    + JOKER_SLOTS
    + 1
    + 1
    + CONSUMABLE_SLOT_COUNT
    + SHOP_VOUCHER_COUNT
    + SHOP_PACK_COUNT
)

# Action space (MultiDiscrete) 對應 action mask layout
ACTION_PARAM_SIZES = [
    ACTION_TYPE_COUNT,                # action_type
    *([2] * HAND_SIZE),               # card selection
    3,                                # blind selection
    SHOP_JOKER_COUNT,                 # shop joker purchase
    JOKER_SLOTS,                      # sell joker slots
    1,                                # reroll (no index)
    1,                                # skip blind (no index)
    CONSUMABLE_SLOT_COUNT,            # use consumable
    SHOP_VOUCHER_COUNT,               # buy voucher (single)
    SHOP_PACK_COUNT,                  # buy pack
]

# Scalar 索引
SCALAR_IDX_SCORE_PROGRESS = 0
SCALAR_IDX_ANTE = 1
SCALAR_IDX_BLIND_TYPE = 2
SCALAR_IDX_STAGE = 3
SCALAR_IDX_PLAYS_LEFT = 4
SCALAR_IDX_DISCARDS_LEFT = 5
SCALAR_IDX_MONEY = 6
SCALAR_IDX_REWARD = 7
SCALAR_IDX_DECK_RATIO = 8
SCALAR_IDX_JOKER_USAGE = 9
SCALAR_IDX_ROUND = 10
SCALAR_IDX_STEP = 11
SCALAR_IDX_BOSS_BLIND = 12
SCALAR_IDX_CONSUMABLE_USAGE = 13
SCALAR_IDX_VOUCHER_PROGRESS = 14
SCALAR_IDX_HAND_LEVEL = 15  # 15..27 = 13 hand levels
SCALAR_IDX_TAG_COUNT = 28
SCALAR_IDX_ENDLESS_MODE = 29
SCALAR_IDX_ENDLESS_ANTE = 30
SCALAR_IDX_REROLL_COST = 31

# Stage 常量
STAGE_PRE_BLIND = 0
STAGE_BLIND = 1
STAGE_POST_BLIND = 2
STAGE_SHOP = 3
STAGE_END = 4

# Action type 名稱（用於日誌）
ACTION_TYPE_NAMES = [
    "SELECT", "PLAY", "DISCARD", "SELECT_BLIND", "CASH_OUT",
    "BUY_JOKER", "NEXT_ROUND", "REROLL", "SELL_JOKER", "SKIP_BLIND",
    "USE_CONSUMABLE", "BUY_VOUCHER", "BUY_PACK"
]


# ============================================================================
# Episode Metrics 追蹤系統
# ============================================================================

@dataclass
class EpisodeMetrics:
    """追蹤單個 episode 的詳細統計"""

    # 基礎結果
    won: bool = False
    final_ante: int = 1
    final_round: int = 0
    total_steps: int = 0
    total_reward: float = 0.0

    # 經濟統計
    max_money: int = 0
    total_money_earned: int = 0
    total_money_spent: int = 0
    final_money: int = 0

    # Joker 統計
    max_jokers: int = 0
    jokers_bought: int = 0
    jokers_sold: int = 0

    # Blind 統計
    blinds_attempted: int = 0
    blinds_cleared: int = 0
    blinds_skipped: int = 0
    boss_blinds_cleared: int = 0

    # 動作統計
    action_counts: Dict[int, int] = field(default_factory=lambda: defaultdict(int))

    # 階段時間分布
    steps_in_blind: int = 0
    steps_in_shop: int = 0
    steps_in_pre_blind: int = 0

    # 商店統計
    shop_visits: int = 0
    rerolls_used: int = 0
    vouchers_bought: int = 0
    packs_bought: int = 0
    consumables_used: int = 0

    # 出牌統計
    plays_made: int = 0
    discards_made: int = 0
    avg_cards_per_play: float = 0.0
    total_cards_played: int = 0

    # 獎勵來源分解
    reward_from_play: float = 0.0
    reward_from_blind_clear: float = 0.0
    reward_from_shop: float = 0.0
    reward_from_skip: float = 0.0
    reward_from_game_end: float = 0.0

    # 分數統計
    max_score_in_blind: int = 0
    total_score: int = 0

    # 追蹤用的內部狀態
    _prev_stage: int = -1
    _prev_ante: int = 1
    _prev_money: int = 0
    _prev_score: int = 0
    _prev_joker_count: int = 0
    _in_blind_start_score: int = 0

    def update_from_step(
        self,
        info: Dict[str, Any],
        scalars: np.ndarray,
        action_type: int,
        reward: float,
        done: bool,
    ):
        """根據 gRPC info + observation 更新統計"""
        # 優先使用 gRPC info（真實值），不足時再從 scalars 估算
        ante = int(info.get("ante", 1)) or 1
        stage = int(info.get("stage", STAGE_PRE_BLIND))
        money = int(info.get("money", 0))
        score = int(info.get("chips", 0))
        joker_count = int(info.get("joker_count", 0))
        blind_type = int(info.get("blind_type", -1))
        game_end = int(info.get("game_end", 0))
        round_num = int(scalars[SCALAR_IDX_ROUND] * 24 + 0.5) if scalars.size > SCALAR_IDX_ROUND else 0

        self.total_steps += 1
        self.total_reward += reward

        # 更新動作計數
        if 0 <= action_type < ACTION_TYPE_COUNT:
            self.action_counts[action_type] += 1

        # 階段時間追蹤
        if stage == STAGE_BLIND:
            self.steps_in_blind += 1
        elif stage == STAGE_SHOP:
            self.steps_in_shop += 1
        elif stage == STAGE_PRE_BLIND:
            self.steps_in_pre_blind += 1

        # 經濟追蹤
        self.max_money = max(self.max_money, money)
        if money > self._prev_money:
            self.total_money_earned += money - self._prev_money
        elif money < self._prev_money:
            self.total_money_spent += self._prev_money - money

        # Joker 追蹤
        self.max_jokers = max(self.max_jokers, joker_count)

        # 動作特定統計
        if action_type == ACTION_TYPE_PLAY:
            self.plays_made += 1
            cards_played = int(info.get("cards_played", 0))
            if cards_played > 0:
                self.total_cards_played += cards_played
        elif action_type == ACTION_TYPE_DISCARD:
            self.discards_made += 1
        elif action_type == ACTION_TYPE_BUY_JOKER:
            if joker_count > self._prev_joker_count:
                self.jokers_bought += 1
        elif action_type == ACTION_TYPE_SELL_JOKER:
            if joker_count < self._prev_joker_count:
                self.jokers_sold += 1
        elif action_type == ACTION_TYPE_REROLL:
            self.rerolls_used += 1
        elif action_type == ACTION_TYPE_SKIP_BLIND:
            self.blinds_skipped += 1
            self.reward_from_skip += max(0, reward)
        elif action_type == ACTION_TYPE_USE_CONSUMABLE:
            self.consumables_used += 1
        elif action_type == ACTION_TYPE_BUY_VOUCHER:
            self.vouchers_bought += 1
        elif action_type == ACTION_TYPE_BUY_PACK:
            self.packs_bought += 1

        # 階段轉換檢測
        if self._prev_stage != stage:
            # 進入 Blind
            if stage == STAGE_BLIND and self._prev_stage != STAGE_BLIND:
                self.blinds_attempted += 1
                self._in_blind_start_score = score

            # 離開 Blind（進入 PostBlind = 過關）
            if self._prev_stage == STAGE_BLIND and stage == STAGE_POST_BLIND:
                self.blinds_cleared += 1
                blind_score = score - self._in_blind_start_score
                self.max_score_in_blind = max(self.max_score_in_blind, blind_score)
                self.total_score += blind_score
                # Boss Blind 檢測
                if blind_type == 2:  # Boss
                    self.boss_blinds_cleared += 1

            # 進入商店
            if stage == STAGE_SHOP:
                self.shop_visits += 1

            self._prev_stage = stage

        # Ante 進度追蹤
        if ante > self._prev_ante:
            self._prev_ante = ante

        # 獎勵來源分解（啟發式）
        if action_type == ACTION_TYPE_PLAY and reward > 0:
            self.reward_from_play += reward
        elif stage == STAGE_POST_BLIND and reward > 0.2:
            self.reward_from_blind_clear += reward
        elif stage == STAGE_SHOP and abs(reward) > 0.01:
            self.reward_from_shop += reward

        # 更新 prev 狀態
        self._prev_money = money
        self._prev_score = score
        self._prev_joker_count = joker_count

        # 最終狀態
        if done:
            self.final_ante = ante
            self.final_round = round_num
            self.final_money = money
            # 判斷勝負：以 game_end 為準
            self.won = game_end == 1
            self.reward_from_game_end = reward if abs(reward) > 0.3 else 0

            # 計算平均出牌數
            if self.plays_made > 0:
                self.avg_cards_per_play = self.total_cards_played / self.plays_made

    def to_dict(self) -> Dict[str, Any]:
        """轉換為字典格式，用於 info"""
        action_dist = {
            ACTION_TYPE_NAMES[k]: v
            for k, v in self.action_counts.items()
            if k < len(ACTION_TYPE_NAMES)
        }

        return {
            # 基礎結果
            "episode/won": int(self.won),
            "episode/final_ante": self.final_ante,
            "episode/final_round": self.final_round,
            "episode/total_steps": self.total_steps,
            "episode/total_reward": self.total_reward,

            # 進度指標
            "progress/blind_clear_rate": self.blinds_cleared / max(1, self.blinds_attempted),
            "progress/boss_clear_rate": self.boss_blinds_cleared / max(1, self.blinds_cleared),
            "progress/skip_rate": self.blinds_skipped / max(1, self.blinds_attempted + self.blinds_skipped),

            # 經濟指標
            "economy/max_money": self.max_money,
            "economy/total_earned": self.total_money_earned,
            "economy/total_spent": self.total_money_spent,
            "economy/final_money": self.final_money,
            "economy/efficiency": self.total_money_spent / max(1, self.total_money_earned),

            # Joker 指標
            "joker/max_count": self.max_jokers,
            "joker/bought": self.jokers_bought,
            "joker/sold": self.jokers_sold,
            "joker/net": self.jokers_bought - self.jokers_sold,

            # 商店行為
            "shop/visits": self.shop_visits,
            "shop/rerolls": self.rerolls_used,
            "shop/rerolls_per_visit": self.rerolls_used / max(1, self.shop_visits),
            "shop/vouchers_bought": self.vouchers_bought,
            "shop/packs_bought": self.packs_bought,
            "shop/consumables_used": self.consumables_used,

            # 出牌行為
            "play/total_plays": self.plays_made,
            "play/total_discards": self.discards_made,
            "play/discard_ratio": self.discards_made / max(1, self.plays_made),

            # 時間分布
            "time/pct_in_blind": self.steps_in_blind / max(1, self.total_steps),
            "time/pct_in_shop": self.steps_in_shop / max(1, self.total_steps),
            "time/pct_in_pre_blind": self.steps_in_pre_blind / max(1, self.total_steps),

            # 獎勵分解
            "reward/from_play": self.reward_from_play,
            "reward/from_blind_clear": self.reward_from_blind_clear,
            "reward/from_shop": self.reward_from_shop,
            "reward/from_skip": self.reward_from_skip,
            "reward/from_game_end": self.reward_from_game_end,

            # 分數
            "score/max_in_blind": self.max_score_in_blind,
            "score/total": self.total_score,

            # 動作分布
            **{f"action/{k}": v for k, v in action_dist.items()},
        }


@dataclass
class AggregatedMetrics:
    """聚合多個 episode 的統計"""

    episodes: int = 0
    wins: int = 0
    total_reward: float = 0.0
    total_ante: int = 0
    total_rounds: int = 0
    total_blinds_cleared: int = 0
    total_blinds_attempted: int = 0
    total_boss_cleared: int = 0
    total_money_earned: int = 0
    total_jokers_bought: int = 0
    total_plays: int = 0
    total_discards: int = 0
    total_skips: int = 0
    total_rerolls: int = 0

    # 用於計算移動平均
    recent_wins: list = field(default_factory=list)
    recent_antes: list = field(default_factory=list)
    recent_rewards: list = field(default_factory=list)
    window_size: int = 100

    def update(self, metrics: EpisodeMetrics):
        """用新 episode 的統計更新聚合"""
        self.episodes += 1
        self.wins += int(metrics.won)
        self.total_reward += metrics.total_reward
        self.total_ante += metrics.final_ante
        self.total_rounds += metrics.final_round
        self.total_blinds_cleared += metrics.blinds_cleared
        self.total_blinds_attempted += metrics.blinds_attempted
        self.total_boss_cleared += metrics.boss_blinds_cleared
        self.total_money_earned += metrics.total_money_earned
        self.total_jokers_bought += metrics.jokers_bought
        self.total_plays += metrics.plays_made
        self.total_discards += metrics.discards_made
        self.total_skips += metrics.blinds_skipped
        self.total_rerolls += metrics.rerolls_used

        # 更新移動窗口
        self.recent_wins.append(int(metrics.won))
        self.recent_antes.append(metrics.final_ante)
        self.recent_rewards.append(metrics.total_reward)

        if len(self.recent_wins) > self.window_size:
            self.recent_wins.pop(0)
            self.recent_antes.pop(0)
            self.recent_rewards.pop(0)

    def to_dict(self) -> Dict[str, Any]:
        """轉換為字典格式"""
        n = max(1, self.episodes)
        recent_n = max(1, len(self.recent_wins))

        return {
            # 總體統計
            "agg/episodes": self.episodes,
            "agg/win_rate": self.wins / n,
            "agg/avg_reward": self.total_reward / n,
            "agg/avg_ante": self.total_ante / n,
            "agg/avg_rounds": self.total_rounds / n,

            # 移動平均（最近 N 局）
            "recent/win_rate": sum(self.recent_wins) / recent_n,
            "recent/avg_ante": sum(self.recent_antes) / recent_n,
            "recent/avg_reward": sum(self.recent_rewards) / recent_n,

            # 行為統計
            "agg/blind_clear_rate": self.total_blinds_cleared / max(1, self.total_blinds_attempted),
            "agg/boss_clear_rate": self.total_boss_cleared / max(1, self.total_blinds_cleared),
            "agg/skip_rate": self.total_skips / max(1, self.total_blinds_attempted + self.total_skips),
            "agg/avg_jokers_bought": self.total_jokers_bought / n,
            "agg/avg_rerolls": self.total_rerolls / n,
            "agg/discard_ratio": self.total_discards / max(1, self.total_plays),
        }


class JokerGymEnv(gym.Env):
    """簡化的 Gym 環境，使用扁平化 observation，支援詳細 metrics 追蹤"""

    metadata = {"render_modes": []}

    def __init__(self, address: str = "127.0.0.1:50051", track_metrics: bool = True) -> None:
        self._client = JokerEnvClient(address)
        spec = self._client.get_spec()

        obs_shape = tuple(spec.observation.shape)
        self.observation_space = spaces.Box(
            low=-np.inf, high=np.inf, shape=obs_shape, dtype=np.float32
        )

        # Action space: MultiDiscrete 對應 action mask layout
        self.action_space = spaces.MultiDiscrete(ACTION_PARAM_SIZES)
        self._last_done = False
        self._last_obs = None

        # 獎勵計算器（Python 端計算獎勵）
        self._reward_calculator = RewardCalculator()

        # Metrics 追蹤
        self._track_metrics = track_metrics
        self._episode_metrics: Optional[EpisodeMetrics] = None
        self._aggregated_metrics = AggregatedMetrics()
        self._last_action_type = -1

    def reset(
        self, *, seed: int | None = None, options: Dict[str, Any] | None = None
    ) -> Tuple[np.ndarray, Dict[str, Any]]:
        # 如果有上一局的 metrics，聚合它
        if self._episode_metrics is not None and self._track_metrics:
            self._aggregated_metrics.update(self._episode_metrics)

        response = self._client.reset(seed or 0)
        observation = _tensor_to_numpy(response.observation.features)
        action_mask = _tensor_to_numpy(response.observation.action_mask) if response.observation else None
        info = _info_to_dict(response.info)

        self._last_done = False
        self._last_obs = observation
        self._last_action_mask = _normalize_action_mask(action_mask)
        self._last_action_type = -1

        # 重置獎勵計算器
        self._reward_calculator.reset()

        # 開始新的 episode metrics
        if self._track_metrics:
            self._episode_metrics = EpisodeMetrics()

        return observation, info

    def step(self, action):
        action_type, card_mask = _parse_action(action)
        response = self._client.step(action_type=action_type, action_id=card_mask)
        observation = _tensor_to_numpy(response.observation.features)
        action_mask = _tensor_to_numpy(response.observation.action_mask) if response.observation else None
        info = _info_to_dict(response.info)
        terminated = response.done
        truncated = False

        # Python 端計算獎勵
        reward = self._reward_calculator.calculate(info)

        self._last_done = terminated
        self._last_obs = observation
        self._last_action_mask = _normalize_action_mask(action_mask)
        self._last_action_type = action_type

        # 更新 metrics
        if self._track_metrics and self._episode_metrics is not None:
            scalars = observation[:SCALAR_COUNT]
            self._episode_metrics.update_from_step(
                info, scalars, action_type, reward, terminated
            )

            # 如果 episode 結束，將詳細 metrics 加入 info
            if terminated:
                episode_info = self._episode_metrics.to_dict()
                info.update(episode_info)
                # 也加入聚合統計
                agg_info = self._aggregated_metrics.to_dict()
                info.update(agg_info)

        return observation, reward, terminated, truncated, info

    def action_masks(self) -> np.ndarray:
        if self._last_done:
            return np.zeros(ACTION_MASK_SIZE, dtype=bool)
        return self._last_action_mask

    def get_aggregated_metrics(self) -> Dict[str, Any]:
        """取得聚合的 metrics（用於外部監控）"""
        return self._aggregated_metrics.to_dict()

    def get_episode_metrics(self) -> Optional[Dict[str, Any]]:
        """取得當前 episode 的 metrics"""
        if self._episode_metrics is not None:
            return self._episode_metrics.to_dict()
        return None


class JokerGymDictEnv(gym.Env):
    """使用 Dict observation 的 Gym 環境，適合訓練，支援詳細 metrics 追蹤"""

    metadata = {"render_modes": []}

    def __init__(self, address: str = "127.0.0.1:50051", track_metrics: bool = True) -> None:
        self._client = JokerEnvClient(address)
        spec = self._client.get_spec()

        obs_shape = tuple(spec.observation.shape)
        if obs_shape and obs_shape[0] != TOTAL_OBS_SIZE:
            print(f"Warning: Observation size mismatch. Expected {TOTAL_OBS_SIZE}, got {obs_shape[0]}")

        self.action_space = spaces.MultiDiscrete(ACTION_PARAM_SIZES)
        self.observation_space = spaces.Dict(
            {
                "scalars": spaces.Box(
                    low=-np.inf, high=np.inf, shape=(SCALAR_COUNT,), dtype=np.float32
                ),
                "selection_mask": spaces.Box(
                    low=0.0, high=1.0, shape=(SELECTION_FEATURES,), dtype=np.float32
                ),
                "hand": spaces.Box(
                    low=0.0,
                    high=1.0,
                    shape=(HAND_SIZE, CARD_FEATURES),
                    dtype=np.float32,
                ),
                "hand_type": spaces.Box(
                    low=0.0, high=1.0, shape=(HAND_TYPE_COUNT,), dtype=np.float32
                ),
                "deck": spaces.Box(
                    low=0.0, high=1.0, shape=(DECK_FEATURES,), dtype=np.float32
                ),
                "jokers": spaces.Box(
                    low=0.0, high=1.0, shape=(JOKER_SLOTS, JOKER_SINGLE_FEATURES), dtype=np.float32
                ),
                "shop": spaces.Box(
                    low=0.0, high=np.inf, shape=(SHOP_JOKER_COUNT, SHOP_SINGLE_FEATURES), dtype=np.float32
                ),
                "boss_blind": spaces.Box(
                    low=0.0, high=1.0, shape=(BOSS_BLIND_COUNT,), dtype=np.float32
                ),
                "deck_type": spaces.Box(
                    low=0.0, high=1.0, shape=(DECK_TYPE_FEATURES,), dtype=np.float32
                ),
                "stake": spaces.Box(
                    low=0.0, high=1.0, shape=(STAKE_FEATURES,), dtype=np.float32
                ),
                "vouchers": spaces.Box(
                    low=0.0, high=1.0, shape=(VOUCHER_FEATURES,), dtype=np.float32
                ),
                "consumables": spaces.Box(
                    low=0.0, high=1.0, shape=(CONSUMABLE_SLOT_COUNT, CONSUMABLE_FEATURES), dtype=np.float32
                ),
                "tags": spaces.Box(
                    low=0.0, high=1.0, shape=(TAG_FEATURES,), dtype=np.float32
                ),
            }
        )
        self._last_done = False
        self._last_obs = None

        # 獎勵計算器（Python 端計算獎勵）
        self._reward_calculator = RewardCalculator()

        # Metrics 追蹤
        self._track_metrics = track_metrics
        self._episode_metrics: Optional[EpisodeMetrics] = None
        self._aggregated_metrics = AggregatedMetrics()
        self._last_action_type = -1

    def reset(
        self, *, seed: int | None = None, options: Dict[str, Any] | None = None
    ) -> Tuple[Dict[str, np.ndarray], Dict[str, Any]]:
        # 如果有上一局的 metrics，聚合它
        if self._episode_metrics is not None and self._track_metrics:
            self._aggregated_metrics.update(self._episode_metrics)

        response = self._client.reset(seed or 0)
        flat = _tensor_to_numpy(response.observation.features)
        action_mask = _tensor_to_numpy(response.observation.action_mask) if response.observation else None
        observation = _split_observation(flat)
        info = _info_to_dict(response.info)

        self._last_done = False
        self._last_obs = flat
        self._last_action_mask = _normalize_action_mask(action_mask)
        self._last_action_type = -1

        # 重置獎勵計算器
        self._reward_calculator.reset()

        # 開始新的 episode metrics
        if self._track_metrics:
            self._episode_metrics = EpisodeMetrics()

        return observation, info

    def step(self, action):
        action_type, card_mask = _parse_action(action)
        response = self._client.step(action_type=action_type, action_id=card_mask)
        flat = _tensor_to_numpy(response.observation.features)
        action_mask = _tensor_to_numpy(response.observation.action_mask) if response.observation else None
        observation = _split_observation(flat)
        info = _info_to_dict(response.info)
        terminated = response.done
        truncated = False

        # Python 端計算獎勵
        reward = self._reward_calculator.calculate(info)

        self._last_done = terminated
        self._last_obs = flat
        self._last_action_mask = _normalize_action_mask(action_mask)
        self._last_action_type = action_type

        # 更新 metrics
        if self._track_metrics and self._episode_metrics is not None:
            scalars = observation["scalars"]
            self._episode_metrics.update_from_step(
                info, scalars, action_type, reward, terminated
            )

            # 如果 episode 結束，將詳細 metrics 加入 info
            if terminated:
                episode_info = self._episode_metrics.to_dict()
                info.update(episode_info)
                # 也加入聚合統計
                agg_info = self._aggregated_metrics.to_dict()
                info.update(agg_info)

        return observation, reward, terminated, truncated, info

    def action_masks(self) -> np.ndarray:
        if self._last_done:
            return np.zeros(ACTION_MASK_SIZE, dtype=bool)
        return self._last_action_mask

    def get_aggregated_metrics(self) -> Dict[str, Any]:
        """取得聚合的 metrics（用於外部監控）"""
        return self._aggregated_metrics.to_dict()

    def get_episode_metrics(self) -> Optional[Dict[str, Any]]:
        """取得當前 episode 的 metrics"""
        if self._episode_metrics is not None:
            return self._episode_metrics.to_dict()
        return None


# ============================================================================
# 輔助函數
# ============================================================================

def _tensor_to_numpy(tensor) -> np.ndarray:
    data = np.asarray(tensor.data, dtype=np.float32)
    if tensor.shape:
        return data.reshape(tuple(tensor.shape))
    return data


def _normalize_action_mask(mask: Optional[np.ndarray]) -> np.ndarray:
    """確保 action mask 長度一致，並轉為 bool"""
    if mask is None:
        return np.ones(ACTION_MASK_SIZE, dtype=bool)
    flat = np.asarray(mask, dtype=np.float32).flatten()
    if flat.size < ACTION_MASK_SIZE:
        flat = np.pad(flat, (0, ACTION_MASK_SIZE - flat.size))
    elif flat.size > ACTION_MASK_SIZE:
        flat = flat[:ACTION_MASK_SIZE]
    return flat > 0.5


def _split_observation(flat: np.ndarray) -> Dict[str, np.ndarray]:
    """將扁平化的 observation 分割成 Dict 格式"""
    flat = np.asarray(flat, dtype=np.float32).flatten()

    offset = 0

    # Scalars (20)
    scalars = flat[offset : offset + SCALAR_COUNT]
    offset += SCALAR_COUNT

    # Selection mask (8)
    selection_mask = flat[offset : offset + SELECTION_FEATURES]
    offset += SELECTION_FEATURES

    # Hand features (8 * 21 = 168)
    hand_flat = flat[offset : offset + HAND_FEATURES]
    hand = hand_flat.reshape((HAND_SIZE, CARD_FEATURES))
    offset += HAND_FEATURES

    # Hand type (13)
    hand_type = flat[offset : offset + HAND_TYPE_COUNT]
    offset += HAND_TYPE_COUNT

    # Deck (52)
    deck = flat[offset : offset + DECK_FEATURES]
    offset += DECK_FEATURES

    # Jokers (5 * 153 = 765)
    jokers_flat = flat[offset : offset + JOKER_FEATURES]
    jokers = jokers_flat.reshape((JOKER_SLOTS, JOKER_SINGLE_FEATURES))
    offset += JOKER_FEATURES

    # Shop (2 * 151 = 302)
    shop_flat = flat[offset : offset + SHOP_FEATURES]
    shop = shop_flat.reshape((SHOP_JOKER_COUNT, SHOP_SINGLE_FEATURES))
    offset += SHOP_FEATURES

    # Boss Blind one-hot (27)
    boss_blind = flat[offset : offset + BOSS_BLIND_COUNT]
    offset += BOSS_BLIND_COUNT

    # Deck type one-hot (16)
    deck_type = flat[offset : offset + DECK_TYPE_FEATURES]
    offset += DECK_TYPE_FEATURES

    # Stake one-hot (8)
    stake = flat[offset : offset + STAKE_FEATURES]
    offset += STAKE_FEATURES

    # Voucher ownership (36)
    vouchers = flat[offset : offset + VOUCHER_FEATURES]
    offset += VOUCHER_FEATURES

    # Consumables (2 * 52 = 104)
    consumables_flat = flat[offset : offset + CONSUMABLE_SLOT_COUNT * CONSUMABLE_FEATURES]
    consumables = consumables_flat.reshape((CONSUMABLE_SLOT_COUNT, CONSUMABLE_FEATURES))
    offset += CONSUMABLE_SLOT_COUNT * CONSUMABLE_FEATURES

    # Tags (25)
    tags = flat[offset : offset + TAG_FEATURES]

    return {
        "scalars": scalars,
        "selection_mask": selection_mask,
        "hand": hand,
        "hand_type": hand_type,
        "deck": deck,
        "jokers": jokers,
        "shop": shop,
        "boss_blind": boss_blind,
        "deck_type": deck_type,
        "stake": stake,
        "vouchers": vouchers,
        "consumables": consumables,
        "tags": tags,
    }


def _info_to_dict(info) -> Dict[str, Any]:
    """從 gRPC EnvInfo 提取所有字段"""
    if info is None:
        return {}
    return {
        # 基本狀態
        "episode_step": info.episode_step,
        "chips": info.chips,
        "mult": info.mult,
        "blind_target": info.blind_target,

        # 擴展狀態 - 用於獎勵計算
        "ante": info.ante,
        "stage": info.stage,
        "blind_type": info.blind_type,
        "plays_left": info.plays_left,
        "discards_left": info.discards_left,
        "money": info.money,

        # 事件追蹤
        "score_delta": info.score_delta,
        "money_delta": info.money_delta,
        "last_action_type": info.last_action_type,
        "last_action_cost": info.last_action_cost,

        # Joker 狀態
        "joker_count": info.joker_count,
        "joker_slot_limit": info.joker_slot_limit,

        # 遊戲結束狀態
        "game_end": info.game_end,
        "blind_cleared": info.blind_cleared,

        # 動作細節
        "cards_played": info.cards_played,
        "cards_discarded": info.cards_discarded,
        "hand_type": info.hand_type,

        # Skip Blind 相關
        "tag_id": info.tag_id,

        # 消耗品相關
        "consumable_id": info.consumable_id,
    }


def _parse_action(action) -> tuple[int, int]:
    """解析 action 為 (action_type, action_id)"""
    if isinstance(action, (int, np.integer)):
        return int(action), 0

    action = np.asarray(action, dtype=int).flatten()
    if action.size < 1:
        return 0, 0

    action_type = int(action[0])
    if action_type < 0 or action_type >= ACTION_TYPE_COUNT:
        action_type = 0

    # 解析 card selection
    card_mask = 0
    card_flags = action[1 : 1 + HAND_SIZE]
    for idx, flag in enumerate(card_flags):
        if flag:
            card_mask |= 1 << idx

    # 其他 action 參數（依照 ACTION_PARAM_SIZES）
    offset = 1 + HAND_SIZE
    blind_choice = int(action[offset]) if action.size > offset else 0
    offset += 1
    shop_choice = int(action[offset]) if action.size > offset else 0
    offset += 1
    sell_choice = int(action[offset]) if action.size > offset else 0
    offset += 1
    offset += 1  # reroll (no index)
    offset += 1  # skip blind (no index)
    consumable_choice = int(action[offset]) if action.size > offset else 0
    offset += 1
    offset += 1  # voucher (single)
    pack_choice = int(action[offset]) if action.size > offset else 0

    # 根據 action_type 決定 action_id
    if action_type == ACTION_TYPE_SELECT:
        action_id = card_mask
    elif action_type == ACTION_TYPE_SELECT_BLIND:
        action_id = blind_choice
    elif action_type == ACTION_TYPE_BUY_JOKER:
        action_id = shop_choice
    elif action_type == ACTION_TYPE_SELL_JOKER:
        action_id = sell_choice
    elif action_type == ACTION_TYPE_USE_CONSUMABLE:
        action_id = consumable_choice
    elif action_type == ACTION_TYPE_BUY_PACK:
        action_id = pack_choice
    else:
        action_id = 0

    return action_type, action_id


def _action_mask_from_scalars(scalars: np.ndarray) -> np.ndarray:
    """根據 scalars 構建 action mask"""
    # scalars[3] = stage (0=PreBlind, 1=Blind, 2=PostBlind, 3=Shop, 4=End)
    stage = scalars[SCALAR_IDX_STAGE] * 4.0  # 反正規化

    in_blind = abs(stage - 1.0) < 0.5
    in_pre_blind = stage < 0.5
    in_post_blind = abs(stage - 2.0) < 0.5
    in_shop = abs(stage - 3.0) < 0.5

    plays_left = scalars[SCALAR_IDX_PLAYS_LEFT] > 0.01
    discards_left = scalars[SCALAR_IDX_DISCARDS_LEFT] > 0.01
    has_money = scalars[SCALAR_IDX_MONEY] > 0.01

    return _build_action_mask(
        in_blind=in_blind,
        in_pre_blind=in_pre_blind,
        in_post_blind=in_post_blind,
        in_shop=in_shop,
        plays_left=plays_left,
        discards_left=discards_left,
        has_money=has_money,
    )


def _build_action_mask_from_obs(obs: np.ndarray) -> np.ndarray:
    """從扁平化 observation 構建 action mask"""
    if obs is None or len(obs) < SCALAR_COUNT:
        return np.ones(ACTION_TYPE_COUNT + HAND_SIZE * 2, dtype=bool)
    scalars = obs[:SCALAR_COUNT]
    return _action_mask_from_scalars(scalars)


def _build_action_mask(
    in_blind: bool,
    in_pre_blind: bool,
    in_post_blind: bool,
    in_shop: bool,
    plays_left: bool,
    discards_left: bool,
    has_money: bool = True,
) -> np.ndarray:
    """
    構建 action mask（與 ACTION_MASK_SIZE 對齊）。

    Mask 結構:
    - [0-12]: action_type
    - [13..29]: card selection (8 * 2)
    - [29..32]: blind selection (3)
    - [32..34]: shop joker (2)
    - [34..39]: sell joker slots (5)
    - [39]: reroll
    - [40]: skip blind
    - [41..43]: use consumable (2)
    - [43]: buy voucher
    - [44..46]: buy pack (2)
    """
    mask = []

    # Action type mask (13)
    mask.append(bool(in_blind))  # SELECT
    mask.append(bool(in_blind and plays_left))  # PLAY
    mask.append(bool(in_blind and discards_left))  # DISCARD
    mask.append(bool(in_pre_blind))  # SELECT_BLIND
    mask.append(bool(in_post_blind))  # CASH_OUT
    mask.append(bool(in_shop and has_money))  # BUY_JOKER
    mask.append(bool(in_shop))  # NEXT_ROUND
    mask.append(bool(in_shop and has_money))  # REROLL
    mask.append(bool(in_shop))  # SELL_JOKER
    mask.append(bool(in_pre_blind))  # SKIP_BLIND
    mask.append(bool(in_blind))  # USE_CONSUMABLE
    mask.append(bool(in_shop and has_money))  # BUY_VOUCHER
    mask.append(bool(in_shop and has_money))  # BUY_PACK

    # 卡片選擇 mask - 只在 Blind 階段才能選擇
    can_select = in_blind
    for _ in range(HAND_SIZE):
        mask.append(can_select)  # 不選
        mask.append(can_select)  # 選

    # Blind selection (3)
    mask.extend([in_pre_blind] * 3)

    # Shop joker purchase (2)
    mask.extend([in_shop and has_money] * SHOP_JOKER_COUNT)

    # Sell joker slots (5)
    mask.extend([in_shop] * JOKER_SLOTS)

    # Reroll (1)
    mask.append(in_shop and has_money)

    # Skip blind (1)
    mask.append(in_pre_blind)

    # Use consumable (2)
    mask.extend([in_blind] * CONSUMABLE_SLOT_COUNT)

    # Buy voucher (1)
    mask.append(in_shop and has_money)

    # Buy pack (2)
    mask.extend([in_shop and has_money] * SHOP_PACK_COUNT)

    return np.array(mask, dtype=bool)
