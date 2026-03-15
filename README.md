# Joker Guide RL

[繁體中文版本](README.zh-TW.md)

A Balatro reinforcement learning training project using **Rust + Python separated architecture**.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Training Loop                             │
│  ┌──────────────┐    gRPC     ┌──────────────────────────────┐  │
│  │  Rust Engine │◄───────────►│       Python Environment      │  │
│  │              │             │                                │  │
│  │ • Game State │  ────────►  │ • Reward Calculation          │  │
│  │ • Action Mask│  StepResp   │ • Policy Network              │  │
│  │ • Validation │             │ • Training Algorithm          │  │
│  └──────────────┘             └──────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

- **Rust Engine** (`rust-engine/`): Pure game environment - state management, action validation, action mask generation
- **Python Environment** (`python-env/`): Reward calculation, Gymnasium wrapper, MaskablePPO training

## Key Results

| Metric | Value |
|--------|-------|
| **Best avg_ante** | 8.42 (E4 experiment) |
| **Baseline → Best** | 3.57 → 8.42 (+136%) |
| **Training FPS** | 18,184 (27.3x from initial 665) |
| **Core Finding** | Reward simplification > reward engineering |

### Experiment History

| Experiment | Modification | avg_ante | Δ |
|------------|-------------|----------|---|
| E1 | Baseline (reward v5.0) | 3.57 | — |
| E2 | Freeze opinion rewards | 4.72 | +32% |
| E3 | Fix sell_joker asymmetry | 5.55 | +55% |
| E4 | Cap game_end_reward ±2.0 | 8.42 | +136% |

**Key insight**: VecNormalize scale dominance — a large terminal reward (±5.0) compressed all intermediate signals into noise. Capping it to ±2.0 restored learning across all reward channels.

## Quick Start

**One-click setup** (recommended):
```bash
chmod +x scripts/setup.sh
./scripts/setup.sh
```

Or manually:
```bash
# 1. Build Rust engine
cd rust-engine && cargo build --release && cd ..

# 2. Install Python dependencies
cd python-env && pip install . && cd ..

# 3. Start training
./train.sh 4 --timesteps 100000 --checkpoint python-env/models/my_model
```

See [INSTALL.md](INSTALL.md) for detailed installation instructions, GPU setup, and troubleshooting.

## Architecture Overview

### Observation Vector (1,556 dimensions)

| Component | Dims | Description |
|-----------|------|-------------|
| Scalars | 32 | Game state (score, ante, stage, money, etc.) |
| Selection | 8 | Hand card selection mask |
| Hand | 168 | 8 cards × 21 features (rank, suit, enhancement, seal, edition) |
| Hand Type | 13 | Poker hand type one-hot |
| Deck | 52 | Remaining card counts |
| Jokers | 765 | 5 slots × 153 features (150 ID one-hot + 3 flags) |
| Shop | 302 | 2 shop jokers × 151 features (150 ID one-hot + cost) |
| Boss Blind | 27 | Boss blind type one-hot |
| Deck Type | 16 | Starting deck type |
| Stake | 8 | Difficulty level |
| Vouchers | 36 | Owned voucher flags |
| Consumables | 104 | 2 slots × 52 types |
| Tags | 25 | Tag counts |

### Action Space

MultiDiscrete action space with **46-dimensional action mask**:

| ID | Action | ID | Action |
|----|--------|----|--------|
| 0 | SELECT | 7 | REROLL |
| 1 | PLAY | 8 | SELL_JOKER |
| 2 | DISCARD | 9 | SKIP_BLIND |
| 3 | SELECT_BLIND | 10 | USE_CONSUMABLE |
| 4 | CASH_OUT | 11 | BUY_VOUCHER |
| 5 | BUY_JOKER | 12 | BUY_PACK |
| 6 | NEXT_ROUND | | |

### Key Design Principles

- **164 Jokers** with declarative effect definition system (`joker_def.rs`)
- **Reward separation**: Rust provides delta info, Python calculates rewards (no recompilation needed)
- **Multi-session support**: Single Rust engine serves multiple Python environments via gRPC session IDs

## Concurrent Training (Recommended)

Use `train.sh` for automated concurrent training:

```bash
# 120 batch environments, 10M timesteps (high-performance mode)
./train.sh 120 --timesteps 10000000 --batch-env --batch-size 512 --n-steps 512

# 4 parallel environments, 1M timesteps (lightweight mode)
./train.sh 4 --timesteps 1000000 --checkpoint python-env/models/v1

# Disable TensorBoard
./train.sh 4 --timesteps 1000000 --no-tensorboard
```

The script automatically:
1. Starts the Rust engine
2. Waits for gRPC server ready
3. Creates timestamped log directory (`python-env/logs/run_YYYYMMDD_HHMMSS`)
4. Starts TensorBoard on http://localhost:6006
5. Launches parallel Python training
6. Handles graceful shutdown on Ctrl+C (stops all processes)

## Manual Training

### Start Rust Server

```bash
cd rust-engine && cargo run --release
```

gRPC service listens on `127.0.0.1:50051`.

### Training with MaskablePPO (Recommended)

```bash
PYTHONPATH=python-env/src python -m joker_env.train_sb3 \
  --timesteps 100000 \
  --checkpoint python-env/models/ppo \
  --tensorboard-log python-env/logs/ppo
```

Key parameters (v10.1):

| Parameter | Default | Description |
|-----------|---------|-------------|
| `--timesteps` | 50000 | Total training steps |
| `--checkpoint` | - | Model save path |
| `--save-interval` | 25000 | Checkpoint interval |
| `--n-envs` | 1 | Parallel environments |
| `--n-steps` | 512 | Rollout length per env |
| `--batch-size` | 256 | Minibatch size |
| `--learning-rate` | 3e-4 | Learning rate |
| `--ent-coef` | 0.06 | Entropy coefficient |
| `--gamma` | 0.95 | Discount factor |
| `--gae-lambda` | 0.92 | GAE lambda |
| `--n-epochs` | 3 | PPO epochs per rollout |
| `--clip-range-vf` | 0.2 | Value function clip range |
| `--target-kl` | 0.02 | Early stopping KL threshold |
| `--net-arch` | 128 128 | MLP hidden layers |
| `--batch-env` | off | Use JokerBatchVecEnv for high FPS |

Full parameter list: `python -m joker_env.train_sb3 --help`

## Reward System (v10.0)

Rewards are calculated in Python (`python-env/src/joker_env/reward.py`):

| Event | Range | Description |
|-------|-------|-------------|
| Game Win | +5.0 | Terminal goal, highest reward |
| Game Lose | -2.0 ~ -0.5 | Penalty scaled by progress |
| Ante Progress | +0.48 ~ +2.27 | Progressive scaling (0.15×a^1.5) |
| Blind Clear | +0.25 ~ +0.75 | Ante-adjusted bonus |
| Boss Clear | +0.0 ~ +0.15 | Difficulty-based bonus (v7.0) |
| Play Hand | +0.02 ~ +0.17 | Base bonus + normalized score reward |
| Discard | -0.05 ~ -0.02 | Increased cost to prevent discard loops |
| Buy Joker | -0.3 ~ +0.3 | Phase-weighted with economy penalty |
| Skip Blind | -0.2 ~ +0.25 | State-aware with dynamic opportunity cost (v10.0) |
| Reroll | variable | Shop quality-aware reroll budget (v10.0) |
| Joker Synergy | +0.0 ~ +0.12 | Synergy group and build alignment (v7.0) |
| Score Efficiency | +0.0 ~ +0.06 | Bonus for exceeding expected score (v6.9) |

Key features:
- **Terminal reward dominance**: Win reward (5.0) outweighs all intermediate rewards
- **Reward hacking protection**: Empty discard (-0.05), failed purchase (-0.05), no-op (-0.03)
- **Shop quality scoring**: Rarity (40%), synergy (30%), cost-efficiency (20%), specials (10%)
- **Reroll budget tracking**: First 2 rerolls normal, diminishing returns after 3rd
- **Joker contribution tracking**: xMult weighted highest (0.5), chips lowest (0.2)

## FPS Optimization History

| Phase | FPS | Speedup | Technique |
|-------|-----|---------|-----------|
| Initial | 665 | 1.0x | Basic gRPC |
| Proto zero-copy | — | — | raw_data zero-copy deserialization |
| Vectorized obs | — | — | Batch observation splitting |
| torch.compile | — | — | JIT-compiled policy network |
| f64→f32 fix | — | — | Action mask dtype correction |
| **Final** | **18,184** | **27.3x** | All optimizations combined |

## Project Structure

```
joker-guide/
├── rust-engine/src/
│   ├── main.rs              # gRPC service entry
│   ├── game/                # Core game logic
│   │   ├── joker_def.rs     # Declarative Joker effects (164 Jokers)
│   │   ├── joker.rs         # Joker implementation
│   │   ├── scoring.rs       # Scoring engine
│   │   ├── blinds.rs        # Blind/Boss/Ante
│   │   ├── cards.rs         # Card/Enhancement/Seal/Edition
│   │   └── ...              # Other game systems
│   └── service/             # gRPC service layer
│       ├── state.rs         # Game state management
│       ├── observation.rs   # Observation vector building
│       └── action_mask.rs   # Legal action generation
├── python-env/src/joker_env/
│   ├── env.py               # Gymnasium environment wrapper
│   ├── reward.py            # Reward calculation (v10.0)
│   ├── callbacks.py         # Training callbacks
│   ├── train_sb3.py         # MaskablePPO training
│   ├── batch_vec_env.py     # High-performance vectorized environment
│   └── train.py             # Basic REINFORCE training
├── proto/
│   └── joker_guide.proto    # gRPC protocol definition
├── data/                    # Game data (JSON reference files)
├── experiments/             # Experiment logs and research notes
└── train.sh                 # Concurrent training script
```

## Testing

```bash
# Rust tests (195 tests)
cd rust-engine && cargo test

# Python reward tests
cd python-env && pytest tests/
```

### Test Coverage

| Module | Content |
|--------|---------|
| `game/joker.rs` | Joker effect calculations, state accumulation |
| `game/scoring.rs` | Scoring engine, hand type recognition |
| `service/action_mask.rs` | State-gating, legal action generation |
| `reward.py` | 70 reward function unit tests |

## Experiment Tracking

- **TensorBoard**: Auto-starts with `train.sh` at http://localhost:6006
- **Log directory**: `python-env/logs/run_YYYYMMDD_HHMMSS/` (timestamped)
- **Checkpoints**: Saved to `python-env/experiments/checkpoints.jsonl`
- **Report script**: `python scripts/checkpoint_report.py --tail 10`
- **Experiment logs**: `experiments/autoresearch_log.md`, `experiments/daily_log_*.md`

## Proto Regeneration

After modifying `proto/joker_guide.proto`:

```bash
./scripts/gen_proto.sh
```

## Requirements

- Rust 1.70+
- Python 3.10+
- Dependencies: `gymnasium`, `torch`, `stable-baselines3`, `sb3-contrib`, `grpcio`
