from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
import torch
from torch import nn, optim

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

EMBED_DIM = 32


def load_joker_vocab_size() -> int:
    for parent in Path(__file__).resolve().parents:
        candidate = parent / "data" / "joker-ids.json"
        if candidate.exists():
            ids = json.loads(candidate.read_text(encoding="ascii"))
            return max(1, len(ids))
    return 1


class PolicyNet(nn.Module):
    def __init__(self, joker_vocab_size: int, device: torch.device) -> None:
        super().__init__()
        self.device = device
        self.joker_emb = nn.Embedding(joker_vocab_size + 1, EMBED_DIM, padding_idx=0)
        input_dim = (
            SCALAR_COUNT
            + SELECTION_FEATURES
            + HAND_FEATURES
            + HAND_TYPE_COUNT
            + DECK_FEATURES
            + EMBED_DIM
            + JOKER_SLOTS
        )
        self.trunk = nn.Sequential(
            nn.Linear(input_dim, 128),
            nn.ReLU(),
            nn.Linear(128, 128),
            nn.ReLU(),
        )
        self.action_head = nn.Linear(128, 3 + HAND_SIZE)

    def forward(self, obs: dict[str, np.ndarray]) -> torch.Tensor:
        features = self._build_features(obs)
        return self.action_head(self.trunk(features))

    def sample_action(self, obs: dict[str, np.ndarray]) -> tuple[np.ndarray, torch.Tensor]:
        logits = self.forward(obs)
        type_logits = logits[:3]
        card_logits = logits[3:]

        type_dist = torch.distributions.Categorical(logits=type_logits)
        card_dist = torch.distributions.Bernoulli(logits=card_logits)

        action_type = type_dist.sample()
        action_cards = card_dist.sample()

        log_prob = type_dist.log_prob(action_type) + card_dist.log_prob(action_cards).sum()

        action_vector = torch.cat(
            [action_type.unsqueeze(0), action_cards], dim=0
        ).detach()
        return action_vector.cpu().numpy().astype(int), log_prob

    def _build_features(self, obs: dict[str, np.ndarray]) -> torch.Tensor:
        scalars = torch.tensor(obs["scalars"], dtype=torch.float32, device=self.device)
        selection_mask = torch.tensor(
            obs["selection_mask"], dtype=torch.float32, device=self.device
        )
        hand = torch.tensor(obs["hand"], dtype=torch.float32, device=self.device).flatten()
        hand_type = torch.tensor(obs["hand_type"], dtype=torch.float32, device=self.device)
        deck = torch.tensor(obs["deck"], dtype=torch.float32, device=self.device)

        jokers = np.asarray(obs["jokers"], dtype=np.float32)
        joker_ids = torch.tensor(jokers[:, 0], dtype=torch.long, device=self.device)
        joker_enabled = torch.tensor(jokers[:, 1], dtype=torch.float32, device=self.device)

        joker_emb = self.joker_emb(joker_ids)
        mask = (joker_ids > 0).float().unsqueeze(1)
        if mask.sum() > 0:
            joker_pooled = (joker_emb * mask).sum(dim=0) / mask.sum()
        else:
            joker_pooled = torch.zeros(EMBED_DIM, device=self.device)

        return torch.cat(
            [scalars, selection_mask, hand, hand_type, deck, joker_pooled, joker_enabled]
        )


def compute_returns(rewards: list[float], gamma: float, device: torch.device) -> torch.Tensor:
    returns = []
    total = 0.0
    for reward in reversed(rewards):
        total = reward + gamma * total
        returns.append(total)
    returns.reverse()
    tensor = torch.tensor(returns, dtype=torch.float32, device=device)
    if tensor.numel() > 1:
        tensor = (tensor - tensor.mean()) / (tensor.std() + 1e-6)
    return tensor


def train(
    episodes: int = 50,
    gamma: float = 0.99,
    lr: float = 3e-4,
    checkpoint: Path | None = None,
) -> None:
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    env = JokerGymDictEnv()
    joker_vocab_size = load_joker_vocab_size()
    policy = PolicyNet(joker_vocab_size, device).to(device)
    optimizer = optim.Adam(policy.parameters(), lr=lr)

    for episode in range(1, episodes + 1):
        obs, _ = env.reset()
        log_probs = []
        rewards = []
        done = False

        while not done:
            action, log_prob = policy.sample_action(obs)
            obs, reward, terminated, truncated, _ = env.step(action)
            done = terminated or truncated
            log_probs.append(log_prob)
            rewards.append(reward)

        returns = compute_returns(rewards, gamma, device)
        loss = -(torch.stack(log_probs) * returns).sum()

        optimizer.zero_grad()
        loss.backward()
        optimizer.step()

        episode_reward = float(np.sum(rewards))
        print(f"episode {episode} reward {episode_reward:.2f}")

    if checkpoint:
        checkpoint.parent.mkdir(parents=True, exist_ok=True)
        torch.save(policy.state_dict(), checkpoint)
        print(f"Saved policy state to {checkpoint}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Train the JokerGymDictEnv with a simple policy."
    )
    parser.add_argument("--episodes", type=int, default=50)
    parser.add_argument("--checkpoint", type=Path, default=None)
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--gamma", type=float, default=0.99)
    args = parser.parse_args()

    train(
        episodes=args.episodes,
        lr=args.lr,
        gamma=args.gamma,
        checkpoint=args.checkpoint,
    )


if __name__ == "__main__":
    main()
