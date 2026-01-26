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
    SHOP_JOKER_COUNT,
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
            + embed_dim  # pooled joker embedding
            + JOKER_SLOTS  # joker_enabled flags
            + embed_dim  # pooled shop embedding
            + SHOP_JOKER_COUNT  # shop joker prices (normalized)
        )
        super().__init__(observation_space, features_dim=features_dim)

        self.joker_emb = torch.nn.Embedding(joker_vocab_size + 1, embed_dim, padding_idx=0)
        self.shop_emb = torch.nn.Embedding(joker_vocab_size + 1, embed_dim, padding_idx=0)

    def forward(self, observations: dict[str, torch.Tensor]) -> torch.Tensor:
        scalars = observations["scalars"]
        selection = observations["selection_mask"]
        hand = observations["hand"].flatten(start_dim=1)
        hand_type = observations["hand_type"]
        deck = observations["deck"]

        # 處理已擁有的 jokers
        jokers = observations["jokers"]
        joker_ids = jokers[..., 0].long().clamp_min(0)
        joker_enabled = jokers[..., 1]

        joker_emb = self.joker_emb(joker_ids)
        joker_mask = (joker_ids > 0).float().unsqueeze(-1)
        joker_pooled = (joker_emb * joker_mask).sum(dim=1) / joker_mask.sum(dim=1).clamp_min(1.0)

        # 處理商店中的 jokers
        shop = observations["shop"]
        shop_ids = shop[..., 0].long().clamp_min(0)
        shop_prices = shop[..., 1] / 10.0  # 正規化價格

        shop_emb = self.shop_emb(shop_ids)
        shop_mask = (shop_ids > 0).float().unsqueeze(-1)
        shop_pooled = (shop_emb * shop_mask).sum(dim=1) / shop_mask.sum(dim=1).clamp_min(1.0)

        return torch.cat(
            [scalars, selection, hand, hand_type, deck,
             joker_pooled, joker_enabled, shop_pooled, shop_prices], dim=1
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


def get_device(use_mps: bool = False) -> str:
    """選擇運算裝置"""
    if use_mps and torch.backends.mps.is_available():
        print("Using Apple MPS (Metal) acceleration")
        return "mps"
    elif torch.cuda.is_available():
        print("Using CUDA acceleration")
        return "cuda"
    else:
        print("Using CPU")
        return "cpu"


def train(
    total_timesteps: int = 50000,
    checkpoint: Path | None = None,
    save_interval: int = 25000,
    tensorboard_log: Path | None = None,
    use_mps: bool = False,
) -> None:
    env = JokerGymDictEnv()
    env = ActionMasker(env, lambda e: e.action_masks())

    device = get_device(use_mps)
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
        ent_coef=0.1,
        policy_kwargs=policy_kwargs,
        tensorboard_log=str(tensorboard_log) if tensorboard_log is not None else None,
        device=device,
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
    parser.add_argument("--mps", action="store_true", help="Use Apple MPS acceleration")
    args = parser.parse_args()

    train(
        total_timesteps=args.timesteps,
        checkpoint=args.checkpoint,
        save_interval=args.save_interval,
        tensorboard_log=args.tensorboard_log,
        use_mps=args.mps,
    )


if __name__ == "__main__":
    main()
