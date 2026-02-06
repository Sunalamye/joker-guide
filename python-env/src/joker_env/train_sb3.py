from __future__ import annotations

import argparse
import json
import inspect
import os
from datetime import datetime
from pathlib import Path

import torch
from gymnasium import spaces
from sb3_contrib import MaskablePPO
from sb3_contrib.common.wrappers import ActionMasker
from stable_baselines3.common.vec_env import SubprocVecEnv, VecNormalize
from joker_env.batch_vec_env import JokerBatchVecEnv
from stable_baselines3.common.torch_layers import BaseFeaturesExtractor

from joker_env import JokerGymDictEnv
from joker_env.callbacks import JokerMetricsCallback, EntropyScheduleCallback, FpsOnlyCallback
from joker_env.env import (
    BOSS_BLIND_COUNT,
    CARD_FEATURES,
    CONSUMABLE_FEATURES,
    CONSUMABLE_SLOT_COUNT,
    DECK_FEATURES,
    DECK_TYPE_FEATURES,
    HAND_FEATURES,
    HAND_SIZE,
    HAND_TYPE_COUNT,
    JOKER_SLOTS,
    SCALAR_COUNT,
    SELECTION_FEATURES,
    SHOP_JOKER_COUNT,
    STAKE_FEATURES,
    TAG_FEATURES,
    VOUCHER_FEATURES,
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
    """
    特徵提取器：使用嵌入層壓縮 Joker ID one-hot 編碼

    觀察空間處理：
    - Joker ID (150 維 one-hot) → 32 維嵌入 → 池化為單一向量
    - Shop Joker ID → 同上
    - 其他特徵直接傳遞
    """

    def __init__(self, observation_space: spaces.Dict, joker_vocab_size: int, embed_dim: int = 32):
        features_dim = (
            SCALAR_COUNT                    # 32: 遊戲狀態標量
            + SELECTION_FEATURES            # 8: 手牌選擇遮罩
            + HAND_FEATURES                 # 168: 8 * 21 手牌特徵
            + HAND_TYPE_COUNT               # 13: 牌型 one-hot
            + DECK_FEATURES                 # 52: 牌組計數
            + embed_dim                     # 32: 池化後的 Joker 嵌入
            + JOKER_SLOTS * 3               # 15: Joker 狀態標誌 (enabled, eternal, negative)
            + embed_dim                     # 32: 池化後的 Shop 嵌入
            + SHOP_JOKER_COUNT              # 2: Shop Joker 價格
            + BOSS_BLIND_COUNT              # 27: Boss Blind one-hot
            + DECK_TYPE_FEATURES            # 16: 牌組類型 one-hot
            + STAKE_FEATURES                # 8: 難度 one-hot
            + VOUCHER_FEATURES              # 36: Voucher 擁有標誌
            + CONSUMABLE_SLOT_COUNT * embed_dim  # 64: 消耗品嵌入
            + TAG_FEATURES                  # 25: Tag 計數
        )
        super().__init__(observation_space, features_dim=features_dim)

        # Joker 嵌入層：將 ID one-hot 壓縮為低維向量
        self.joker_emb = torch.nn.Embedding(joker_vocab_size + 1, embed_dim, padding_idx=0)
        self.shop_emb = torch.nn.Embedding(joker_vocab_size + 1, embed_dim, padding_idx=0)

        # 消耗品嵌入層
        consumable_vocab_size = CONSUMABLE_FEATURES  # 52
        self.consumable_emb = torch.nn.Embedding(consumable_vocab_size + 1, embed_dim, padding_idx=0)

    def _extract_joker_id_from_onehot(self, jokers: torch.Tensor) -> torch.Tensor:
        """從 one-hot 編碼提取 Joker ID（argmax）"""
        # jokers shape: (batch, JOKER_SLOTS, 153)
        # 前 150 維是 ID one-hot
        joker_onehot = jokers[..., :150]
        # argmax 獲取 ID，如果全為 0 則返回 0（空槽位）
        joker_ids = joker_onehot.argmax(dim=-1)
        # 檢查是否有有效的 one-hot（至少一個 1）
        has_joker = joker_onehot.sum(dim=-1) > 0.5
        joker_ids = joker_ids * has_joker.long()
        return joker_ids

    def _extract_consumable_id_from_onehot(self, consumables: torch.Tensor) -> torch.Tensor:
        """從 one-hot 編碼提取消耗品 ID"""
        # consumables shape: (batch, CONSUMABLE_SLOT_COUNT, 52)
        consumable_ids = consumables.argmax(dim=-1)
        has_consumable = consumables.sum(dim=-1) > 0.5
        consumable_ids = consumable_ids * has_consumable.long()
        return consumable_ids

    def forward(self, observations: dict[str, torch.Tensor]) -> torch.Tensor:
        # 基礎特徵（直接傳遞）
        scalars = observations["scalars"]
        selection = observations["selection_mask"]
        hand = observations["hand"].flatten(start_dim=1)
        hand_type = observations["hand_type"]
        deck = observations["deck"]

        # Boss Blind, Deck Type, Stake, Vouchers, Tags（直接傳遞）
        boss_blind = observations["boss_blind"]
        deck_type = observations["deck_type"]
        stake = observations["stake"]
        vouchers = observations["vouchers"]
        tags = observations["tags"]

        # 處理已擁有的 Jokers（使用嵌入層）
        jokers = observations["jokers"]  # (batch, 5, 153)
        joker_ids = self._extract_joker_id_from_onehot(jokers)
        joker_flags = jokers[..., 150:153]  # enabled, eternal, negative

        joker_emb = self.joker_emb(joker_ids)
        joker_mask = (joker_ids > 0).float().unsqueeze(-1)
        joker_pooled = (joker_emb * joker_mask).sum(dim=1) / joker_mask.sum(dim=1).clamp_min(1.0)

        # 處理商店中的 Jokers
        shop = observations["shop"]  # (batch, 2, 151)
        shop_onehot = shop[..., :150]
        shop_ids = shop_onehot.argmax(dim=-1) * (shop_onehot.sum(dim=-1) > 0.5).long()
        shop_prices = shop[..., 150] / 10.0  # 正規化價格

        shop_emb = self.shop_emb(shop_ids)
        shop_mask = (shop_ids > 0).float().unsqueeze(-1)
        shop_pooled = (shop_emb * shop_mask).sum(dim=1) / shop_mask.sum(dim=1).clamp_min(1.0)

        # 處理消耗品（使用嵌入層）
        consumables = observations["consumables"]  # (batch, 2, 52)
        consumable_ids = self._extract_consumable_id_from_onehot(consumables)
        consumable_emb = self.consumable_emb(consumable_ids)  # (batch, 2, embed_dim)
        consumable_flat = consumable_emb.flatten(start_dim=1)  # (batch, 2 * embed_dim)

        return torch.cat(
            [
                scalars,
                selection,
                hand,
                hand_type,
                deck,
                joker_pooled,
                joker_flags.flatten(start_dim=1),
                shop_pooled,
                shop_prices,
                boss_blind,
                deck_type,
                stake,
                vouchers,
                consumable_flat,
                tags,
            ],
            dim=1,
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


def make_env(seed: int | None = None, rank: int = 0, port: int = 50051):
    """創建單一環境的工廠函數"""
    def _init():
        address = f"127.0.0.1:{port}"
        env = JokerGymDictEnv(address=address)
        env = ActionMasker(env, lambda e: e.action_masks())
        if seed is not None:
            env.reset(seed=seed + rank)
        return env
    return _init


def train(
    total_timesteps: int = 50000,
    checkpoint: Path | None = None,
    save_interval: int = 25000,
    tensorboard_log: Path | None = None,
    use_mps: bool = False,
    log_freq: int = 10,
    tb_log_freq: int = 1,
    verbose: int = 1,
    n_steps: int = 256,
    batch_size: int = 64,
    ent_coef: float = 0.08,   # v6.4: 提高初始探索（原 0.05）
    learning_rate: float = 3e-4,
    gamma: float = 0.99,       # v6.8: 提高 gamma 改善長期信用分配（原 0.95）
    gae_lambda: float = 0.95,  # v6.8: 提高 gae_lambda 減少 bias（原 0.92）
    clip_range: float = 0.2,
    clip_range_vf: float | None = None,
    normalize_advantage: bool = True,
    n_epochs: int = 4,
    vf_coef: float = 0.5,
    max_grad_norm: float = 0.5,
    target_kl: float | None = 0.015,
    use_sde: bool = False,
    sde_sample_freq: int = -1,
    stats_window_size: int = 100,
    seed: int | None = None,
    net_arch: list[int] | None = None,
    n_envs: int = 1,
    resume: Path | None = None,
    reset_vec_normalize: bool = False,  # v6.8: 重置 VecNormalize 統計量
    batch_env: bool = False,
    fps_only: bool = True,
    fps_interval: float = 1.0,
) -> None:
    # 創建並行環境
    if n_envs > 1:
        base_port = int(os.environ.get("JOKER_BASE_PORT", "50051"))
        n_engines = int(os.environ.get("JOKER_N_ENGINES", str(n_envs)))

        if batch_env:
            if n_engines != 1:
                raise ValueError("Batch env currently supports only 1 Rust engine")
            print(f"Using {n_envs} parallel environments (Batch VecEnv)")
            print(f"Connecting to 1 Rust engine (port {base_port})")
            env = JokerBatchVecEnv(address=f"127.0.0.1:{base_port}", n_envs=n_envs)
        else:
            print(f"Using {n_envs} parallel environments (SubprocVecEnv)")
            print(f"Connecting to {n_engines} Rust engines (ports {base_port}-{base_port + n_engines - 1})")

            # Round-robin 分配環境到引擎
            env = SubprocVecEnv([
                make_env(seed, i, port=base_port + (i % n_engines))
                for i in range(n_envs)
            ])
        # v5.0: 添加 VecNormalize 進行獎勵正規化
        env = VecNormalize(
            env,
            norm_obs=False,      # Dict obs 不正規化
            norm_reward=True,    # 正規化獎勵到 ~N(0,1)
            clip_reward=10.0,    # 防止異常值
            gamma=gamma,         # 使用訓練的 gamma 值
            training=True,
        )
    else:
        env = JokerGymDictEnv()
        env = ActionMasker(env, lambda e: e.action_masks())

    device = get_device(use_mps)
    joker_vocab_size = load_joker_vocab_size()
    policy_kwargs = dict(
        features_extractor_class=JokerFeaturesExtractor,
        features_extractor_kwargs={"joker_vocab_size": joker_vocab_size},
        net_arch=net_arch or [128, 128],
    )

    effective_verbose = 0 if fps_only else verbose
    model_kwargs = {
        "verbose": effective_verbose,
        "n_steps": n_steps,
        "batch_size": batch_size,
        "ent_coef": ent_coef,
        "learning_rate": learning_rate,
        "gamma": gamma,
        "gae_lambda": gae_lambda,
        "clip_range": clip_range,
        "clip_range_vf": clip_range_vf,
        "normalize_advantage": normalize_advantage,
        "n_epochs": n_epochs,
        "vf_coef": vf_coef,
        "max_grad_norm": max_grad_norm,
        "target_kl": target_kl,
        "use_sde": use_sde,
        "sde_sample_freq": sde_sample_freq,
        "stats_window_size": stats_window_size,
        "seed": seed,
        "policy_kwargs": policy_kwargs,
        "tensorboard_log": str(tensorboard_log) if tensorboard_log is not None else None,
        "device": device,
    }
    accepted = set(inspect.signature(MaskablePPO.__init__).parameters)
    filtered_kwargs = {k: v for k, v in model_kwargs.items() if k in accepted}

    if resume is not None:
        print(f"Resuming training from {resume}")
        # 恢復時不傳遞 policy_kwargs，因為 class 對象在不同進程中會被視為不同
        # SB3 會使用 checkpoint 中保存的 policy_kwargs
        resume_kwargs = {k: v for k, v in filtered_kwargs.items() if k != "policy_kwargs"}
        model = MaskablePPO.load(resume, env=env, **resume_kwargs)

        # v6.8: 可選重置 VecNormalize 統計量
        # 當獎勵函數有重大變化時，舊統計量會污染新訓練
        if reset_vec_normalize and isinstance(env, VecNormalize):
            print("Resetting VecNormalize statistics (reward function changed)")
            env.ret_rms.mean = 0.0
            env.ret_rms.var = 1.0
            env.ret_rms.count = 1e-4
    else:
        model = MaskablePPO("MultiInputPolicy", env, **filtered_kwargs)

    remaining = total_timesteps
    chunk = save_interval if save_interval > 0 else total_timesteps
    # v6.4: 使用 callback 列表，包含 Entropy 衰減
    # final_ent 從 0.005 提升到 0.01，維持更多探索
    callbacks = [
        JokerMetricsCallback(verbose=0 if fps_only else verbose, log_freq=log_freq, tb_log_freq=tb_log_freq),
        EntropyScheduleCallback(
            initial_ent=ent_coef,
            final_ent=0.025,  # v10.1: 從 0.01 提升（防止 entropy collapse）
            total_steps=total_timesteps,
        ),
    ]
    if fps_only:
        callbacks.append(FpsOnlyCallback(interval_seconds=fps_interval))
    while remaining > 0:
        step = min(chunk, remaining)
        model.learn(total_timesteps=step, reset_num_timesteps=False, callback=callbacks)
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
    parser.add_argument("--resume", type=Path, default=None, help="Resume training from a saved model")
    parser.add_argument("--save-interval", type=int, default=25000)
    parser.add_argument("--tensorboard-log", type=Path, default=None)
    parser.add_argument("--mps", action="store_true", help="Use Apple MPS acceleration")
    parser.add_argument("--log-freq", type=int, default=10)
    parser.add_argument("--tb-log-freq", type=int, default=1)
    parser.add_argument("--verbose", type=int, default=1, help="SB3 verbosity level")
    parser.add_argument(
        "--fps-only",
        action="store_true",
        default=True,
        help="Only print FPS to stdout (default: on)",
    )
    parser.add_argument(
        "--no-fps-only",
        action="store_false",
        dest="fps_only",
        help="Disable FPS-only output",
    )
    parser.add_argument("--fps-interval", type=float, default=1.0, help="FPS print interval in seconds")
    parser.add_argument("--n-steps", type=int, default=256)
    parser.add_argument("--batch-size", type=int, default=64)
    parser.add_argument("--batch-env", action="store_true", help="Use batch Step RPC in a single process VecEnv")
    parser.add_argument("--ent-coef", type=float, default=0.05)
    parser.add_argument("--learning-rate", type=float, default=3e-4)
    parser.add_argument("--gamma", type=float, default=0.99, help="v6.8: 0.99 for better long-term credit assignment")
    parser.add_argument("--gae-lambda", type=float, default=0.95, help="v6.8: 0.95 for reduced bias")
    parser.add_argument("--clip-range", type=float, default=0.2)
    parser.add_argument("--clip-range-vf", type=float, default=None)
    parser.add_argument("--normalize-advantage", action="store_true", default=True)
    parser.add_argument("--no-normalize-advantage", action="store_false", dest="normalize_advantage")
    parser.add_argument("--n-epochs", type=int, default=4)
    parser.add_argument("--vf-coef", type=float, default=0.5)
    parser.add_argument("--max-grad-norm", type=float, default=0.5)
    parser.add_argument("--target-kl", type=float, default=0.015)
    parser.add_argument("--use-sde", action="store_true", default=False)
    parser.add_argument("--sde-sample-freq", type=int, default=-1)
    parser.add_argument("--stats-window-size", type=int, default=100)
    parser.add_argument("--seed", type=int, default=None)
    parser.add_argument(
        "--net-arch",
        type=int,
        nargs="+",
        default=[128, 128],
        help="MLP hidden sizes, e.g. --net-arch 128 128",
    )
    parser.add_argument(
        "--n-envs",
        type=int,
        default=1,
        help="Number of parallel environments (default: 1, recommended: 4-8)",
    )
    parser.add_argument(
        "--reset-vec-normalize",
        action="store_true",
        default=False,
        help="v6.8: Reset VecNormalize statistics when resuming (use when reward function changed)",
    )
    args = parser.parse_args()

    train(
        total_timesteps=args.timesteps,
        checkpoint=args.checkpoint,
        save_interval=args.save_interval,
        tensorboard_log=args.tensorboard_log,
        use_mps=args.mps,
        log_freq=args.log_freq,
        tb_log_freq=args.tb_log_freq,
        verbose=args.verbose,
        n_steps=args.n_steps,
        batch_size=args.batch_size,
        ent_coef=args.ent_coef,
        learning_rate=args.learning_rate,
        gamma=args.gamma,
        gae_lambda=args.gae_lambda,
        clip_range=args.clip_range,
        clip_range_vf=args.clip_range_vf,
        normalize_advantage=args.normalize_advantage,
        n_epochs=args.n_epochs,
        vf_coef=args.vf_coef,
        max_grad_norm=args.max_grad_norm,
        target_kl=args.target_kl,
        use_sde=args.use_sde,
        sde_sample_freq=args.sde_sample_freq,
        stats_window_size=args.stats_window_size,
        seed=args.seed,
        net_arch=args.net_arch,
        n_envs=args.n_envs,
        resume=args.resume,
        reset_vec_normalize=args.reset_vec_normalize,
        batch_env=args.batch_env,
        fps_only=args.fps_only,
        fps_interval=args.fps_interval,
    )


if __name__ == "__main__":
    main()
