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

### Observation Vector (1,613 dimensions)

| Component | Dims | Description |
|-----------|------|-------------|
| Scalars | 32 | Game state (score, ante, stage, money, etc.) |
| Selection | 8 | Hand card selection mask |
| Hand | 168 | 8 cards × 21 features (rank, suit, enhancement, seal, edition) |
| Hand Type | 13 | Poker hand type one-hot |
| Deck | 52 | Remaining card counts |
| Jokers | 765 | 5 slots × 153 features (150 ID one-hot + 3 flags) |
| Shop | 302 | 2 shop jokers × 151 features |
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
# 4 parallel environments, 1M timesteps
./train.sh 4 --timesteps 1000000 --checkpoint python-env/models/v1

# 8 parallel environments with TensorBoard logging
./train.sh 8 --timesteps 1000000 --checkpoint python-env/models/v1 \
  --tensorboard-log python-env/logs/v1

# Resume interrupted training
./train.sh 4 --resume python-env/models/v1_500000 --timesteps 1000000
```

The script automatically:
1. Builds and starts the Rust engine
2. Waits for gRPC server ready
3. Launches parallel Python training
4. Handles graceful shutdown on Ctrl+C

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

Key parameters:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `--timesteps` | 50000 | Total training steps |
| `--checkpoint` | - | Model save path |
| `--save-interval` | 25000 | Checkpoint interval |
| `--n-envs` | 1 | Parallel environments |
| `--learning-rate` | 3e-4 | Learning rate |
| `--ent-coef` | 0.05 | Entropy coefficient |
| `--gamma` | 0.95 | Discount factor |
| `--batch-size` | 64 | Minibatch size |
| `--net-arch` | 128 128 | MLP hidden layers |

Full parameter list: `python -m joker_env.train_sb3 --help`

## Reward System (v5.0)

Rewards are calculated in Python (`python-env/src/joker_env/reward.py`):

| Event | Range | Description |
|-------|-------|-------------|
| Game Win | +5.0 | Terminal goal, highest reward |
| Game Lose | -2.0 ~ -0.5 | Penalty scaled by progress |
| Ante Progress | +0.48 ~ +2.27 | Progressive scaling (0.15×a^1.5) |
| Blind Clear | +0.25 ~ +0.75 | Ante-adjusted bonus |
| Play Hand | 0 ~ +0.15 | Normalized score reward |
| Discard | -0.05 ~ +0.05 | Empty discard penalty |
| Buy Joker | -0.3 ~ +0.3 | Phase-weighted with economy penalty |
| Skip Blind | -0.2 ~ +0.25 | Tag value assessment |

Key features:
- **Terminal reward dominance**: Win reward (5.0) outweighs all intermediate rewards
- **Reward hacking protection**: Empty discard (-0.05), failed purchase (-0.05), no-op (-0.03)
- **Interest threshold bonuses**: $5/$10/$15/$20/$25 milestone rewards
- **Tag value mapping**: 25 tag types with individual value assessment

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
│   ├── reward.py            # Reward calculation (v5.0)
│   ├── callbacks.py         # Training callbacks
│   ├── train_sb3.py         # MaskablePPO training
│   └── train.py             # Basic REINFORCE training
├── proto/
│   └── joker_guide.proto    # gRPC protocol definition
├── data/                    # Game data (JSON reference files)
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

- Checkpoints saved to `python-env/experiments/checkpoints.jsonl`
- View latest: `tail -n 5 python-env/experiments/checkpoints.jsonl`
- Report script: `python scripts/checkpoint_report.py --tail 10`
- TensorBoard: `tensorboard --logdir python-env/logs/`

## Proto Regeneration

After modifying `proto/joker_guide.proto`:

```bash
./scripts/gen_proto.sh
```

## Requirements

- Rust 1.70+
- Python 3.10+
- Dependencies: `gymnasium`, `torch`, `stable-baselines3`, `sb3-contrib`, `grpcio`
