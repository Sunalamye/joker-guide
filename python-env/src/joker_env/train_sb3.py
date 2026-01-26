from __future__ import annotations

import argparse
import json
from datetime import datetime
from pathlib import Path

import torch
from gymnasium import spaces
from sb3_contrib import MaskablePPO
from sb3_contrib.common.wrappers import ActionMasker
from stable_baselines3.common.torch_layers import BaseFeaturesExtractor

from joker_env import JokerGymDictEnv
from joker_env.env import (
    CARD_FEATURES,
    DECK_FEATURES,
    HAND_FEATURES,
    HAND_SIZE,
    HAND_TYPE_COUNT,
    JOKER_SLOTS,
    SCALAR_COUNT,
    SELECTION_FEATURES,
)


def load_joker_vocab_size() -> int:
    for parent in Path(__file__).resolve().parents:
        candidate = parent / "data" / "joker-ids.json"
        if candidate.exists():
            ids = json.loads(candidate.read_text(encoding="ascii"))
            return max(1, len(ids))
    return 1


EXPERIMENTS_DIR = Path(__file__).resolve().parents[2] / "experiments"
EXPERIMENTS_DIR.mkdir(parents=True, exist_ok=True)
CHECKPOINT_LOG = EXPERIMENTS_DIR / "checkpoints.jsonl"


class JokerFeaturesExtractor(BaseFeaturesExtractor):
    def __init__(self, observation_space: spaces.Dict, joker_vocab_size: int, embed_dim: int = 32):
        features_dim = (
            SCALAR_COUNT
            + SELECTION_FEATURES
            + HAND_FEATURES
            + HAND_TYPE_COUNT
            + DECK_FEATURES
            + embed_dim
            + JOKER_SLOTS
        )
        super().__init__(observation_space, features_dim=features_dim)

        self.joker_emb = torch.nn.Embedding(joker_vocab_size + 1, embed_dim, padding_idx=0)

    def forward(self, observations: dict[str, torch.Tensor]) -> torch.Tensor:
        scalars = observations["scalars"]
        selection = observations["selection_mask"]
        hand = observations["hand"].flatten(start_dim=1)
        hand_type = observations["hand_type"]
        deck = observations["deck"]

        jokers = observations["jokers"]
        joker_ids = jokers[..., 0].long().clamp_min(0)
        joker_enabled = jokers[..., 1]

        joker_emb = self.joker_emb(joker_ids)
        mask = (joker_ids > 0).float().unsqueeze(-1)
        pooled = (joker_emb * mask).sum(dim=1) / mask.sum(dim=1).clamp_min(1.0)

        return torch.cat(
            [scalars, selection, hand, hand_type, deck, pooled, joker_enabled], dim=1
        )


def _save_checkpoint(
    model: MaskablePPO,
    checkpoint: Path,
    steps: int,
    total_timesteps: int,
    save_interval: int,
) -> None:
    checkpoint.parent.mkdir(parents=True, exist_ok=True)
    target = checkpoint.with_name(f"{checkpoint.name}_{steps}")
    model.save(target)
    _record_checkpoint(target, steps, total_timesteps, save_interval)
    print(f"Saved SB3 checkpoint to {target}")


def _record_checkpoint(
    path: Path,
    steps: int,
    total_timesteps: int,
    save_interval: int,
) -> None:
    entry = {
        "timestamp": datetime.utcnow().isoformat() + "Z",
        "checkpoint": str(path),
        "steps": steps,
        "total_timesteps": total_timesteps,
        "save_interval": save_interval,
    }
    with CHECKPOINT_LOG.open("a", encoding="utf-8") as log:
        log.write(json.dumps(entry) + "\n")


def train(
    total_timesteps: int = 50000,
    checkpoint: Path | None = None,
    save_interval: int = 25000,
    tensorboard_log: Path | None = None,
) -> None:
    env = JokerGymDictEnv()
    env = ActionMasker(env, lambda e: e.action_masks())

    joker_vocab_size = load_joker_vocab_size()
    policy_kwargs = dict(
        features_extractor_class=JokerFeaturesExtractor,
        features_extractor_kwargs={"joker_vocab_size": joker_vocab_size},
        net_arch=[128, 128],
    )

    model = MaskablePPO(
        "MultiInputPolicy",
        env,
        verbose=1,
        n_steps=256,
        batch_size=64,
        policy_kwargs=policy_kwargs,
        tensorboard_log=str(tensorboard_log) if tensorboard_log is not None else None,
    )

    remaining = total_timesteps
    chunk = save_interval if save_interval > 0 else total_timesteps
    while remaining > 0:
        step = min(chunk, remaining)
        model.learn(total_timesteps=step, reset_num_timesteps=False)
        remaining -= step
        if checkpoint:
            _save_checkpoint(
                model,
                checkpoint,
                model.num_timesteps,
                total_timesteps,
                save_interval,
            )


def main() -> None:
    parser = argparse.ArgumentParser(description="Train MaskablePPO on JokerGymDictEnv")
    parser.add_argument("--timesteps", type=int, default=50000)
    parser.add_argument("--checkpoint", type=Path, default=None)
    parser.add_argument("--save-interval", type=int, default=25000)
    parser.add_argument("--tensorboard-log", type=Path, default=None)
    args = parser.parse_args()

    train(
        total_timesteps=args.timesteps,
        checkpoint=args.checkpoint,
        save_interval=args.save_interval,
        tensorboard_log=args.tensorboard_log,
    )


if __name__ == "__main__":
    main()
