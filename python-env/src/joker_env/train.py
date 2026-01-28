from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import torch
from torch import nn, optim

from joker_env import JokerGymDictEnv
from joker_env.env import (
    ACTION_TYPE_COUNT,
    BOSS_BLIND_COUNT,
    CARD_FEATURES,
    CONSUMABLE_FEATURES,
    CONSUMABLE_SLOT_COUNT,
    DECK_FEATURES,
    DECK_TYPE_FEATURES,
    HAND_FEATURES,
    HAND_SIZE,
    HAND_TYPE_COUNT,
    JOKER_FEATURES,
    JOKER_SLOTS,
    SCALAR_COUNT,
    SELECTION_FEATURES,
    SHOP_FEATURES,
    SHOP_JOKER_COUNT,
    SHOP_PACK_COUNT,
    STAKE_FEATURES,
    TAG_FEATURES,
    VOUCHER_FEATURES,
)

BLIND_SELECTION_COUNT = 3


class PolicyNet(nn.Module):
    def __init__(self, device: torch.device) -> None:
        super().__init__()
        self.device = device
        input_dim = (
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
        self.trunk = nn.Sequential(
            nn.Linear(input_dim, 128),
            nn.ReLU(),
            nn.Linear(128, 128),
            nn.ReLU(),
        )
        action_logit_size = (
            ACTION_TYPE_COUNT
            + HAND_SIZE
            + BLIND_SELECTION_COUNT
            + SHOP_JOKER_COUNT
            + JOKER_SLOTS
            + CONSUMABLE_SLOT_COUNT
            + SHOP_PACK_COUNT
        )
        self.action_head = nn.Linear(128, action_logit_size)

    def forward(self, obs: dict[str, np.ndarray]) -> torch.Tensor:
        features = self._build_features(obs)
        return self.action_head(self.trunk(features))

    def sample_action(self, obs: dict[str, np.ndarray]) -> tuple[np.ndarray, torch.Tensor]:
        logits = self.forward(obs)
        offset = 0
        type_logits = logits[offset : offset + ACTION_TYPE_COUNT]
        offset += ACTION_TYPE_COUNT
        card_logits = logits[offset : offset + HAND_SIZE]
        offset += HAND_SIZE
        blind_logits = logits[offset : offset + BLIND_SELECTION_COUNT]
        offset += BLIND_SELECTION_COUNT
        shop_logits = logits[offset : offset + SHOP_JOKER_COUNT]
        offset += SHOP_JOKER_COUNT
        sell_logits = logits[offset : offset + JOKER_SLOTS]
        offset += JOKER_SLOTS
        consumable_logits = logits[offset : offset + CONSUMABLE_SLOT_COUNT]
        offset += CONSUMABLE_SLOT_COUNT
        pack_logits = logits[offset : offset + SHOP_PACK_COUNT]

        type_dist = torch.distributions.Categorical(logits=type_logits)
        card_dist = torch.distributions.Bernoulli(logits=card_logits)
        blind_dist = torch.distributions.Categorical(logits=blind_logits)
        shop_dist = torch.distributions.Categorical(logits=shop_logits)
        sell_dist = torch.distributions.Categorical(logits=sell_logits)
        consumable_dist = torch.distributions.Categorical(logits=consumable_logits)
        pack_dist = torch.distributions.Categorical(logits=pack_logits)

        action_type = type_dist.sample()
        action_cards = card_dist.sample()
        blind_choice = blind_dist.sample()
        shop_choice = shop_dist.sample()
        sell_choice = sell_dist.sample()
        consumable_choice = consumable_dist.sample()
        pack_choice = pack_dist.sample()

        log_prob = (
            type_dist.log_prob(action_type)
            + card_dist.log_prob(action_cards).sum()
            + blind_dist.log_prob(blind_choice)
            + shop_dist.log_prob(shop_choice)
            + sell_dist.log_prob(sell_choice)
            + consumable_dist.log_prob(consumable_choice)
            + pack_dist.log_prob(pack_choice)
        )

        action_vector = torch.cat(
            [
                action_type.unsqueeze(0),
                action_cards,
                blind_choice.unsqueeze(0),
                shop_choice.unsqueeze(0),
                sell_choice.unsqueeze(0),
                torch.zeros(1, device=self.device),  # reroll (no index)
                torch.zeros(1, device=self.device),  # skip blind (no index)
                consumable_choice.unsqueeze(0),
                torch.zeros(1, device=self.device),  # buy voucher (single)
                pack_choice.unsqueeze(0),
            ],
            dim=0,
        ).detach()
        return action_vector.cpu().numpy().astype(int), log_prob

    def _build_features(self, obs: dict[str, np.ndarray]) -> torch.Tensor:
        scalars = torch.tensor(obs["scalars"], dtype=torch.float32, device=self.device)
        selection_mask = torch.tensor(obs["selection_mask"], dtype=torch.float32, device=self.device)
        hand = torch.tensor(obs["hand"], dtype=torch.float32, device=self.device).flatten()
        hand_type = torch.tensor(obs["hand_type"], dtype=torch.float32, device=self.device)
        deck = torch.tensor(obs["deck"], dtype=torch.float32, device=self.device)
        jokers = torch.tensor(obs["jokers"], dtype=torch.float32, device=self.device).flatten()
        shop = torch.tensor(obs["shop"], dtype=torch.float32, device=self.device).flatten()
        boss_blind = torch.tensor(obs["boss_blind"], dtype=torch.float32, device=self.device)
        deck_type = torch.tensor(obs["deck_type"], dtype=torch.float32, device=self.device)
        stake = torch.tensor(obs["stake"], dtype=torch.float32, device=self.device)
        vouchers = torch.tensor(obs["vouchers"], dtype=torch.float32, device=self.device)
        consumables = torch.tensor(obs["consumables"], dtype=torch.float32, device=self.device).flatten()
        tags = torch.tensor(obs["tags"], dtype=torch.float32, device=self.device)

        return torch.cat(
            [
                scalars,
                selection_mask,
                hand,
                hand_type,
                deck,
                jokers,
                shop,
                boss_blind,
                deck_type,
                stake,
                vouchers,
                consumables,
                tags,
            ]
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
    policy = PolicyNet(device).to(device)
    optimizer = optim.Adam(policy.parameters(), lr=lr)

    for episode in range(1, episodes + 1):
        obs, _ = env.reset()
        log_probs = []
        rewards = []
        done = False

        info = {}
        while not done:
            action, log_prob = policy.sample_action(obs)
            obs, reward, terminated, truncated, info = env.step(action)
            done = terminated or truncated
            log_probs.append(log_prob)
            rewards.append(reward)

        returns = compute_returns(rewards, gamma, device)
        loss = -(torch.stack(log_probs) * returns).sum()

        optimizer.zero_grad()
        loss.backward()
        optimizer.step()

        episode_reward = float(np.sum(rewards))

        # 基本指標
        won = int(info.get("episode/won", 0))
        final_ante = info.get("episode/final_ante", 0)
        final_round = info.get("episode/final_round", 0)
        final_money = info.get("economy/final_money", info.get("episode/final_money", 0))
        blind_clear_rate = info.get("progress/blind_clear_rate", 0.0)

        # 進階指標
        boss_clear_rate = info.get("progress/boss_clear_rate", 0.0)
        skip_rate = info.get("progress/skip_rate", 0.0)
        jokers_bought = info.get("joker/bought", 0)
        jokers_sold = info.get("joker/sold", 0)
        rerolls = info.get("shop/rerolls", 0)
        econ_efficiency = info.get("economy/efficiency", 0.0)

        # 移動平均（來自 AggregatedMetrics）
        recent_win_rate = info.get("recent/win_rate", 0.0)
        recent_avg_ante = info.get("recent/avg_ante", 0.0)
        recent_avg_reward = info.get("recent/avg_reward", 0.0)

        # Action 分佈
        plays = info.get("action/PLAY", 0)
        discards = info.get("action/DISCARD", 0)
        buys = info.get("action/BUY_JOKER", 0)

        # 主要輸出行
        print(
            f"ep {episode:4d} | R {episode_reward:+7.2f} | win {won} | ante {int(final_ante):2d} | "
            f"clr {blind_clear_rate:.0%} | boss {boss_clear_rate:.0%} | skip {skip_rate:.0%}"
        )

        # 每 10 局輸出詳細統計
        if episode % 10 == 0:
            print(
                f"         | joker +{jokers_bought}/-{jokers_sold} | reroll {rerolls} | "
                f"play/disc {plays}/{discards} | econ {econ_efficiency:.2f}"
            )
            print(
                f"  recent | win {recent_win_rate:.1%} | ante {recent_avg_ante:.1f} | "
                f"reward {recent_avg_reward:+.2f}"
            )
            print("-" * 80)

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
