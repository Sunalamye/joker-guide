from __future__ import annotations

from typing import Any, Dict, Tuple

import gymnasium as gym
import numpy as np
from gymnasium import spaces

from joker_env.client import JokerEnvClient


HAND_SIZE = 5
SCALAR_COUNT = 8
SCALAR_IDX_HANDS_LEFT = 4
SCALAR_IDX_DISCARDS_LEFT = 5
SELECTION_FEATURES = HAND_SIZE
CARD_FEATURES = 17
HAND_FEATURES = HAND_SIZE * CARD_FEATURES
HAND_TYPE_COUNT = 10
DECK_FEATURES = 52
JOKER_SLOTS = 5
JOKER_FEATURES = JOKER_SLOTS * 2
TOTAL_OBS_SIZE = (
    SCALAR_COUNT
    + SELECTION_FEATURES
    + HAND_FEATURES
    + HAND_TYPE_COUNT
    + DECK_FEATURES
    + JOKER_FEATURES
)


class JokerGymEnv(gym.Env):
    metadata = {"render_modes": []}

    def __init__(self, address: str = "127.0.0.1:50051", hand_size: int = HAND_SIZE) -> None:
        self._client = JokerEnvClient(address)
        spec = self._client.get_spec()

        obs_shape = tuple(spec.observation.shape)
        if obs_shape and obs_shape[0] != TOTAL_OBS_SIZE:
            raise ValueError(f"Unexpected observation size: {obs_shape[0]}")
        if hand_size != HAND_SIZE:
            raise ValueError(f"hand_size must be {HAND_SIZE}")
        self._hand_size = hand_size
        self.observation_space = spaces.Box(
            low=-np.inf, high=np.inf, shape=obs_shape, dtype=np.float32
        )
        self.action_space = spaces.MultiDiscrete([3] + [2] * self._hand_size)
        self._last_done = False

    def reset(
        self, *, seed: int | None = None, options: Dict[str, Any] | None = None
    ) -> Tuple[np.ndarray, Dict[str, Any]]:
        response = self._client.reset(seed or 0)
        observation = _tensor_to_numpy(response.observation.features)
        info = _info_to_dict(response.info)
        self._last_done = False
        self._last_action_mask = _action_mask_from_flat(observation)
        return observation, info

    def step(self, action: int):
        action_type, discard_mask = _parse_action(action, self._hand_size)
        response = self._client.step(action_type=action_type, action_id=discard_mask)
        observation = _tensor_to_numpy(response.observation.features)
        info = _info_to_dict(response.info)
        terminated = response.done
        truncated = False
        self._last_done = terminated
        self._last_action_mask = _action_mask_from_flat(observation)
        return observation, response.reward, terminated, truncated, info

    def action_masks(self) -> np.ndarray:
        if self._last_done:
            return np.zeros(sum(self.action_space.nvec), dtype=bool)
        return self._last_action_mask


class JokerGymDictEnv(gym.Env):
    metadata = {"render_modes": []}

    def __init__(self, address: str = "127.0.0.1:50051", hand_size: int = HAND_SIZE) -> None:
        self._client = JokerEnvClient(address)
        spec = self._client.get_spec()

        obs_shape = tuple(spec.observation.shape)
        if obs_shape and obs_shape[0] != TOTAL_OBS_SIZE:
            raise ValueError(f"Unexpected observation size: {obs_shape[0]}")
        if hand_size != HAND_SIZE:
            raise ValueError(f"hand_size must be {HAND_SIZE}")

        self._hand_size = hand_size
        self.action_space = spaces.MultiDiscrete([3] + [2] * self._hand_size)
        self.observation_space = spaces.Dict(
            {
                "scalars": spaces.Box(
                    low=0.0, high=1.0, shape=(SCALAR_COUNT,), dtype=np.float32
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
                    low=0.0, high=52.0, shape=(DECK_FEATURES,), dtype=np.float32
                ),
                "jokers": spaces.Box(
                    low=0.0, high=np.inf, shape=(JOKER_SLOTS, 2), dtype=np.float32
                ),
            }
        )
        self._last_done = False

    def reset(
        self, *, seed: int | None = None, options: Dict[str, Any] | None = None
    ) -> Tuple[Dict[str, np.ndarray], Dict[str, Any]]:
        response = self._client.reset(seed or 0)
        flat = _tensor_to_numpy(response.observation.features)
        observation = _split_observation(flat)
        info = _info_to_dict(response.info)
        self._last_done = False
        self._last_action_mask = _action_mask_from_scalars(observation["scalars"])
        return observation, info

    def step(self, action: int):
        action_type, discard_mask = _parse_action(action, self._hand_size)
        response = self._client.step(action_type=action_type, action_id=discard_mask)
        flat = _tensor_to_numpy(response.observation.features)
        observation = _split_observation(flat)
        info = _info_to_dict(response.info)
        terminated = response.done
        truncated = False
        self._last_done = terminated
        self._last_action_mask = _action_mask_from_scalars(observation["scalars"])
        return observation, response.reward, terminated, truncated, info

    def action_masks(self) -> np.ndarray:
        if self._last_done:
            return np.zeros(sum(self.action_space.nvec), dtype=bool)
        return self._last_action_mask


def _tensor_to_numpy(tensor) -> np.ndarray:
    data = np.asarray(tensor.data, dtype=np.float32)
    if tensor.shape:
        return data.reshape(tuple(tensor.shape))
    return data


def _split_observation(flat: np.ndarray) -> Dict[str, np.ndarray]:
    flat = np.asarray(flat, dtype=np.float32).flatten()
    if flat.size != TOTAL_OBS_SIZE:
        raise ValueError(f"Unexpected observation size: {flat.size}")

    offset = 0
    scalars = flat[offset : offset + SCALAR_COUNT]
    offset += SCALAR_COUNT

    selection_flat = flat[offset : offset + SELECTION_FEATURES]
    selection_mask = selection_flat.copy()
    offset += SELECTION_FEATURES

    hand_flat = flat[offset : offset + HAND_FEATURES]
    hand = hand_flat.reshape((HAND_SIZE, CARD_FEATURES))
    offset += HAND_FEATURES

    hand_type = flat[offset : offset + HAND_TYPE_COUNT]
    offset += HAND_TYPE_COUNT

    deck = flat[offset : offset + DECK_FEATURES]
    offset += DECK_FEATURES

    jokers_flat = flat[offset : offset + JOKER_FEATURES]
    jokers = jokers_flat.reshape((JOKER_SLOTS, 2))

    return {
        "scalars": scalars,
        "selection_mask": selection_mask,
        "hand": hand,
        "hand_type": hand_type,
        "deck": deck,
        "jokers": jokers,
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


def _parse_action(action, hand_size: int) -> tuple[int, int]:
    if isinstance(action, (int, np.integer)):
        return 0, 0

    action = np.asarray(action, dtype=int).flatten()
    if action.size < 1:
        return 0, 0

    action_type = int(action[0])
    if action_type not in (0, 1, 2):
        action_type = 0

    discard_mask = 0
    for idx, flag in enumerate(action[1 : 1 + hand_size]):
        if flag:
            discard_mask |= 1 << idx

    return action_type, discard_mask


def _action_mask_from_flat(flat: np.ndarray) -> np.ndarray:
    scalars = np.asarray(flat, dtype=np.float32).flatten()[:SCALAR_COUNT]
    return _action_mask_from_scalars(scalars)


def _action_mask_from_scalars(scalars: np.ndarray) -> np.ndarray:
    hands_left = scalars[SCALAR_IDX_HANDS_LEFT] > 0.0
    discards_left = scalars[SCALAR_IDX_DISCARDS_LEFT] > 0.0
    return _build_action_mask(
        play_allowed=hands_left,
        discard_allowed=discards_left,
        select_allowed=hands_left,
    )


def _build_action_mask(
    play_allowed: bool, discard_allowed: bool, select_allowed: bool = True
) -> np.ndarray:
    mask = []
    mask.append(bool(play_allowed))
    mask.append(bool(discard_allowed))
    mask.append(bool(select_allowed))
    for _ in range(HAND_SIZE):
        mask.append(bool(discard_allowed))
        mask.append(True)
    return np.array(mask, dtype=bool)
