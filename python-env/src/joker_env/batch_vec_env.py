import os
import time
from typing import Any, Dict, List, Optional, Tuple

import numpy as np
from gymnasium import spaces
from stable_baselines3.common.vec_env import VecEnv

from joker_env.client import JokerEnvClient
from joker_env.proto import joker_guide_pb2
from joker_env.env import (
    ACTION_MASK_SIZE,
    ACTION_PARAM_SIZES,
    BOSS_BLIND_COUNT,
    CARD_FEATURES,
    CONSUMABLE_FEATURES,
    CONSUMABLE_SLOT_COUNT,
    DECK_FEATURES,
    DECK_TYPE_FEATURES,
    HAND_SIZE,
    HAND_TYPE_COUNT,
    JOKER_SINGLE_FEATURES,
    JOKER_SLOTS,
    RewardCalculator,
    SCALAR_COUNT,
    SELECTION_FEATURES,
    SHOP_JOKER_COUNT,
    SHOP_SINGLE_FEATURES,
    STAKE_FEATURES,
    TAG_FEATURES,
    TOTAL_OBS_SIZE,
    VOUCHER_FEATURES,
    _info_to_dict,
    _normalize_action_mask,
    _parse_action,
    _split_observation,
    _tensor_to_numpy,
    EpisodeMetrics,
    AggregatedMetrics,
)


def _stack_obs(obs_list: List[Dict[str, np.ndarray]]) -> Dict[str, np.ndarray]:
    keys = obs_list[0].keys()
    return {k: np.stack([o[k] for o in obs_list], axis=0) for k in keys}


class JokerBatchVecEnv(VecEnv):
    """單一進程的 batch VecEnv，使用 StepBatch 降低 gRPC round-trip."""

    def __init__(self, address: str, n_envs: int, track_metrics: bool = True):
        self.n_envs = n_envs
        self._client = JokerEnvClient(address)

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

        self._track_metrics = track_metrics
        self._reward_calculators = [RewardCalculator() for _ in range(n_envs)]
        self._episode_metrics: List[Optional[EpisodeMetrics]] = [None] * n_envs
        self._aggregated_metrics = [AggregatedMetrics() for _ in range(n_envs)]
        self._last_action_mask = [np.ones(ACTION_MASK_SIZE, dtype=bool) for _ in range(n_envs)]

        self._session_ids = [0] * n_envs
        self._actions = None

        if os.environ.get("JOKER_PROFILE_DISABLE") == "1":
            self._py_profile_every = 0
        else:
            self._py_profile_every = int(os.environ.get("JOKER_PY_PROFILE_EVERY", "0") or 0)
        self._py_profile_counter = 0

        super().__init__(n_envs, self.observation_space, self.action_space)

    def reset(self) -> Dict[str, np.ndarray]:
        obs_list = []
        for i in range(self.n_envs):
            resp = self._client.reset_with_session(self._session_ids[i], seed=0)
            self._session_ids[i] = resp.session_id
            flat = _tensor_to_numpy(resp.observation.features)
            action_mask = _tensor_to_numpy(resp.observation.action_mask) if resp.observation else None
            obs = _split_observation(flat)
            obs_list.append(obs)

            self._last_action_mask[i] = _normalize_action_mask(action_mask)
            self._reward_calculators[i].reset()
            if self._track_metrics:
                self._episode_metrics[i] = EpisodeMetrics()
        return _stack_obs(obs_list)

    def step_async(self, actions: np.ndarray) -> None:
        self._actions = actions

    def step_wait(self) -> Tuple[Dict[str, np.ndarray], np.ndarray, np.ndarray, List[Dict[str, Any]]]:
        assert self._actions is not None
        start_ns = None
        if self._py_profile_every > 0:
            start_ns = time.perf_counter_ns()
            self._py_profile_counter += 1
        requests = []
        for i in range(self.n_envs):
            action_type, card_mask = _parse_action(self._actions[i])
            action = joker_guide_pb2.Action(action_id=card_mask, params=[], action_type=action_type)
            req = joker_guide_pb2.StepRequest(action=action, session_id=self._session_ids[i])
            requests.append(req)

        batch_resp = self._client.step_batch(requests)

        obs_list: List[Dict[str, np.ndarray]] = []
        rewards = np.zeros((self.n_envs,), dtype=np.float32)
        dones = np.zeros((self.n_envs,), dtype=bool)
        infos: List[Dict[str, Any]] = [{} for _ in range(self.n_envs)]

        for i, resp in enumerate(batch_resp.responses):
            flat = _tensor_to_numpy(resp.observation.features)
            action_mask = _tensor_to_numpy(resp.observation.action_mask) if resp.observation else None
            obs = _split_observation(flat)
            info = _info_to_dict(resp.info)
            terminated = resp.done

            reward = self._reward_calculators[i].calculate(info)
            rewards[i] = reward
            dones[i] = terminated

            self._last_action_mask[i] = _normalize_action_mask(action_mask)

            if self._track_metrics and self._episode_metrics[i] is not None:
                scalars = obs["scalars"]
                self._episode_metrics[i].update_from_step(
                    info, scalars, info.get("last_action_type", -1), reward, terminated
                )
                if terminated:
                    episode_info = self._episode_metrics[i].to_dict()
                    info.update(episode_info)
                    agg_info = self._aggregated_metrics[i].to_dict()
                    info.update(agg_info)

            if terminated:
                info["terminal_observation"] = obs
                # Auto-reset this env
                resp_reset = self._client.reset_with_session(self._session_ids[i], seed=0)
                self._session_ids[i] = resp_reset.session_id
                flat_reset = _tensor_to_numpy(resp_reset.observation.features)
                mask_reset = (
                    _tensor_to_numpy(resp_reset.observation.action_mask)
                    if resp_reset.observation
                    else None
                )
                obs = _split_observation(flat_reset)
                self._last_action_mask[i] = _normalize_action_mask(mask_reset)
                self._reward_calculators[i].reset()
                if self._track_metrics:
                    self._episode_metrics[i] = EpisodeMetrics()

            obs_list.append(obs)
            infos[i] = info

        obs_batch = _stack_obs(obs_list)

        if self._py_profile_every > 0 and start_ns is not None:
            if self._py_profile_counter % self._py_profile_every == 0:
                elapsed_ms = (time.perf_counter_ns() - start_ns) / 1_000_000.0
                print(f"PY_PROFILE_BATCH ms={elapsed_ms:.3f}", flush=True)

        return obs_batch, rewards, dones, infos

    def close(self) -> None:
        return None

    def get_attr(self, attr_name: str, indices: Optional[List[int]] = None) -> List[Any]:
        if indices is None:
            indices = list(range(self.n_envs))
        return [getattr(self, attr_name) for _ in indices]

    def set_attr(self, attr_name: str, value: Any, indices: Optional[List[int]] = None) -> None:
        if indices is None:
            indices = list(range(self.n_envs))
        for _ in indices:
            setattr(self, attr_name, value)

    def env_method(self, method_name: str, *method_args, **method_kwargs) -> List[Any]:
        if method_name == "action_masks":
            return [self._last_action_mask[i] for i in range(self.n_envs)]
        raise AttributeError(f"Method {method_name} not supported in JokerBatchVecEnv")

    def action_masks(self) -> np.ndarray:
        return np.stack(self._last_action_mask, axis=0)

    def env_is_wrapped(self, wrapper_class, indices: Optional[List[int]] = None) -> List[bool]:
        if indices is None:
            indices = list(range(self.n_envs))
        return [False for _ in indices]

    def seed(self, seed: Optional[int] = None) -> List[Optional[int]]:
        return [seed for _ in range(self.n_envs)]
