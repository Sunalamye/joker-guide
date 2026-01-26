from __future__ import annotations

from typing import Any, Dict, Tuple

import gymnasium as gym
import numpy as np
from gymnasium import spaces

from joker_env.client import JokerEnvClient


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
SCALAR_COUNT = 20  # 擴展的標量特徵
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
DECK_TYPE_FEATURES = 12
STAKE_FEATURES = 8
VOUCHER_FEATURES = 32
CONSUMABLE_FEATURES = 52  # Tarot(22) + Planet(12) + Spectral(18)
TAG_FEATURES = 22

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
SCALAR_IDX_HAND_LEVEL = 15
SCALAR_IDX_TAG_COUNT = 16
SCALAR_IDX_ENDLESS_MODE = 17
SCALAR_IDX_ENDLESS_ANTE = 18
SCALAR_IDX_REROLL_COST = 19


class JokerGymEnv(gym.Env):
    """簡化的 Gym 環境，使用扁平化 observation"""

    metadata = {"render_modes": []}

    def __init__(self, address: str = "127.0.0.1:50051") -> None:
        self._client = JokerEnvClient(address)
        spec = self._client.get_spec()

        obs_shape = tuple(spec.observation.shape)
        self.observation_space = spaces.Box(
            low=-np.inf, high=np.inf, shape=obs_shape, dtype=np.float32
        )

        # Action space: [action_type, card_selection]
        # action_type: 13 種
        # card_selection: 8 張卡片，每張 0 或 1
        self.action_space = spaces.MultiDiscrete([ACTION_TYPE_COUNT] + [2] * HAND_SIZE)
        self._last_done = False
        self._last_obs = None

    def reset(
        self, *, seed: int | None = None, options: Dict[str, Any] | None = None
    ) -> Tuple[np.ndarray, Dict[str, Any]]:
        response = self._client.reset(seed or 0)
        observation = _tensor_to_numpy(response.observation.features)
        info = _info_to_dict(response.info)
        self._last_done = False
        self._last_obs = observation
        return observation, info

    def step(self, action):
        action_type, card_mask = _parse_action(action)
        response = self._client.step(action_type=action_type, action_id=card_mask)
        observation = _tensor_to_numpy(response.observation.features)
        info = _info_to_dict(response.info)
        terminated = response.done
        truncated = False
        self._last_done = terminated
        self._last_obs = observation
        return observation, response.reward, terminated, truncated, info

    def action_masks(self) -> np.ndarray:
        if self._last_done:
            return np.zeros(ACTION_TYPE_COUNT + HAND_SIZE * 2, dtype=bool)
        return _build_action_mask_from_obs(self._last_obs)


class JokerGymDictEnv(gym.Env):
    """使用 Dict observation 的 Gym 環境，適合訓練"""

    metadata = {"render_modes": []}

    def __init__(self, address: str = "127.0.0.1:50051") -> None:
        self._client = JokerEnvClient(address)
        spec = self._client.get_spec()

        obs_shape = tuple(spec.observation.shape)
        if obs_shape and obs_shape[0] != TOTAL_OBS_SIZE:
            print(f"Warning: Observation size mismatch. Expected {TOTAL_OBS_SIZE}, got {obs_shape[0]}")

        self.action_space = spaces.MultiDiscrete([ACTION_TYPE_COUNT] + [2] * HAND_SIZE)
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

    def reset(
        self, *, seed: int | None = None, options: Dict[str, Any] | None = None
    ) -> Tuple[Dict[str, np.ndarray], Dict[str, Any]]:
        response = self._client.reset(seed or 0)
        flat = _tensor_to_numpy(response.observation.features)
        observation = _split_observation(flat)
        info = _info_to_dict(response.info)
        self._last_done = False
        self._last_obs = flat
        self._last_action_mask = _action_mask_from_scalars(observation["scalars"])
        return observation, info

    def step(self, action):
        action_type, card_mask = _parse_action(action)
        response = self._client.step(action_type=action_type, action_id=card_mask)
        flat = _tensor_to_numpy(response.observation.features)
        observation = _split_observation(flat)
        info = _info_to_dict(response.info)
        terminated = response.done
        truncated = False
        self._last_done = terminated
        self._last_obs = flat
        self._last_action_mask = _action_mask_from_scalars(observation["scalars"])
        return observation, response.reward, terminated, truncated, info

    def action_masks(self) -> np.ndarray:
        if self._last_done:
            return np.zeros(ACTION_TYPE_COUNT + HAND_SIZE * 2, dtype=bool)
        return self._last_action_mask


# ============================================================================
# 輔助函數
# ============================================================================

def _tensor_to_numpy(tensor) -> np.ndarray:
    data = np.asarray(tensor.data, dtype=np.float32)
    if tensor.shape:
        return data.reshape(tuple(tensor.shape))
    return data


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

    # Deck type one-hot (12)
    deck_type = flat[offset : offset + DECK_TYPE_FEATURES]
    offset += DECK_TYPE_FEATURES

    # Stake one-hot (8)
    stake = flat[offset : offset + STAKE_FEATURES]
    offset += STAKE_FEATURES

    # Voucher ownership (32)
    vouchers = flat[offset : offset + VOUCHER_FEATURES]
    offset += VOUCHER_FEATURES

    # Consumables (2 * 52 = 104)
    consumables_flat = flat[offset : offset + CONSUMABLE_SLOT_COUNT * CONSUMABLE_FEATURES]
    consumables = consumables_flat.reshape((CONSUMABLE_SLOT_COUNT, CONSUMABLE_FEATURES))
    offset += CONSUMABLE_SLOT_COUNT * CONSUMABLE_FEATURES

    # Tags (22)
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
    if info is None:
        return {}
    return {
        "episode_step": info.episode_step,
        "chips": info.chips,
        "mult": info.mult,
        "blind_target": info.blind_target,
    }


def _parse_action(action) -> tuple[int, int]:
    """解析 action 為 (action_type, card_mask)"""
    if isinstance(action, (int, np.integer)):
        return int(action), 0

    action = np.asarray(action, dtype=int).flatten()
    if action.size < 1:
        return 0, 0

    action_type = int(action[0])
    if action_type < 0 or action_type >= ACTION_TYPE_COUNT:
        action_type = 0

    # 構建卡片選擇 mask
    card_mask = 0
    for idx, flag in enumerate(action[1 : 1 + HAND_SIZE]):
        if flag:
            card_mask |= 1 << idx

    return action_type, card_mask


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
    構建 MaskablePPO 的 action mask。

    Action Space: MultiDiscrete([13, 2, 2, 2, 2, 2, 2, 2, 2])
    - 維度 0: action_type (13 種)
    - 維度 1-8: 每張卡片的選擇 (不選=0, 選=1)

    Mask 結構 (長度 13 + 8*2 = 29):
    - [0-12]: action_type mask
    - [13-14]: 卡片 0 的 [不選, 選]
    - [15-16]: 卡片 1 的 [不選, 選]
    - ...
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

    return np.array(mask, dtype=bool)
