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

# Observation 大小
SCALAR_COUNT = 13  # 擴展的標量特徵 (包含 boss_blind_id)
SELECTION_FEATURES = HAND_SIZE
# Card features: 13 rank + 4 suit + 4 enhance info (enhancement, seal, edition, face_down)
CARD_FEATURES = 21
HAND_FEATURES = HAND_SIZE * CARD_FEATURES
HAND_TYPE_COUNT = 10
DECK_FEATURES = 52
JOKER_FEATURES = JOKER_SLOTS * 2
SHOP_FEATURES = SHOP_JOKER_COUNT * 2

TOTAL_OBS_SIZE = (
    SCALAR_COUNT
    + SELECTION_FEATURES
    + HAND_FEATURES
    + HAND_TYPE_COUNT
    + DECK_FEATURES
    + JOKER_FEATURES
    + SHOP_FEATURES
)

# Action 類型
ACTION_TYPE_SELECT = 0
ACTION_TYPE_PLAY = 1
ACTION_TYPE_DISCARD = 2
ACTION_TYPE_SELECT_BLIND = 3
ACTION_TYPE_CASH_OUT = 4
ACTION_TYPE_BUY_JOKER = 5
ACTION_TYPE_NEXT_ROUND = 6

# Scalar 索引
SCALAR_IDX_PLAYS_LEFT = 4
SCALAR_IDX_DISCARDS_LEFT = 5


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
        # action_type: 7 種
        # card_selection: 8 張卡片，每張 0 或 1
        self.action_space = spaces.MultiDiscrete([7] + [2] * HAND_SIZE)
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
            return np.zeros(7 + HAND_SIZE * 2, dtype=bool)
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

        self.action_space = spaces.MultiDiscrete([7] + [2] * HAND_SIZE)
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
                    low=0.0, high=4.0, shape=(DECK_FEATURES,), dtype=np.float32
                ),
                "jokers": spaces.Box(
                    low=0.0, high=np.inf, shape=(JOKER_SLOTS, 2), dtype=np.float32
                ),
                "shop": spaces.Box(
                    low=0.0, high=np.inf, shape=(SHOP_JOKER_COUNT, 2), dtype=np.float32
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
            return np.zeros(7 + HAND_SIZE * 2, dtype=bool)
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

    # Scalars (12)
    scalars = flat[offset : offset + SCALAR_COUNT]
    offset += SCALAR_COUNT

    # Selection mask (8)
    selection_mask = flat[offset : offset + SELECTION_FEATURES]
    offset += SELECTION_FEATURES

    # Hand features (8 * 21 = 168)
    hand_flat = flat[offset : offset + HAND_FEATURES]
    hand = hand_flat.reshape((HAND_SIZE, CARD_FEATURES))
    offset += HAND_FEATURES

    # Hand type (10)
    hand_type = flat[offset : offset + HAND_TYPE_COUNT]
    offset += HAND_TYPE_COUNT

    # Deck (52)
    deck = flat[offset : offset + DECK_FEATURES]
    offset += DECK_FEATURES

    # Jokers (5 * 2 = 10)
    jokers_flat = flat[offset : offset + JOKER_FEATURES]
    jokers = jokers_flat.reshape((JOKER_SLOTS, 2))
    offset += JOKER_FEATURES

    # Shop (2 * 2 = 4)
    shop_flat = flat[offset : offset + SHOP_FEATURES]
    shop = shop_flat.reshape((SHOP_JOKER_COUNT, 2))

    return {
        "scalars": scalars,
        "selection_mask": selection_mask,
        "hand": hand,
        "hand_type": hand_type,
        "deck": deck,
        "jokers": jokers,
        "shop": shop,
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
    if action_type < 0 or action_type > 6:
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
    stage = scalars[3] * 4.0  # 反正規化

    in_blind = abs(stage - 1.0) < 0.5
    in_pre_blind = stage < 0.5
    in_post_blind = abs(stage - 2.0) < 0.5
    in_shop = abs(stage - 3.0) < 0.5

    plays_left = scalars[SCALAR_IDX_PLAYS_LEFT] > 0.01
    discards_left = scalars[SCALAR_IDX_DISCARDS_LEFT] > 0.01

    return _build_action_mask(
        in_blind=in_blind,
        in_pre_blind=in_pre_blind,
        in_post_blind=in_post_blind,
        in_shop=in_shop,
        plays_left=plays_left,
        discards_left=discards_left,
    )


def _build_action_mask_from_obs(obs: np.ndarray) -> np.ndarray:
    """從扁平化 observation 構建 action mask"""
    if obs is None or len(obs) < SCALAR_COUNT:
        return np.ones(7 + HAND_SIZE * 2, dtype=bool)
    scalars = obs[:SCALAR_COUNT]
    return _action_mask_from_scalars(scalars)


def _build_action_mask(
    in_blind: bool,
    in_pre_blind: bool,
    in_post_blind: bool,
    in_shop: bool,
    plays_left: bool,
    discards_left: bool,
) -> np.ndarray:
    """
    構建 MaskablePPO 的 action mask。

    Action Space: MultiDiscrete([7, 2, 2, 2, 2, 2, 2, 2, 2])
    - 維度 0: action_type (7 種)
    - 維度 1-8: 每張卡片的選擇 (不選=0, 選=1)

    Mask 結構 (長度 7 + 8*2 = 23):
    - [0-6]: action_type mask
    - [7-8]: 卡片 0 的 [不選, 選]
    - [9-10]: 卡片 1 的 [不選, 選]
    - ...
    """
    mask = []

    # Action type mask (7)
    mask.append(bool(in_blind))  # SELECT
    mask.append(bool(in_blind and plays_left))  # PLAY
    mask.append(bool(in_blind and discards_left))  # DISCARD
    mask.append(bool(in_pre_blind))  # SELECT_BLIND
    mask.append(bool(in_post_blind))  # CASH_OUT
    mask.append(bool(in_shop))  # BUY_JOKER
    mask.append(bool(in_shop))  # NEXT_ROUND

    # 卡片選擇 mask - 只在 Blind 階段才能選擇
    can_select = in_blind
    for _ in range(HAND_SIZE):
        mask.append(can_select)  # 不選
        mask.append(can_select)  # 選

    return np.array(mask, dtype=bool)
