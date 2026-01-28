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
- **Python Environment** (`python-env/`): Reward calculation, Gymnasium wrapper, PPO/REINFORCE training

## Key Concepts

- **Observation vector (length 170)**: `[scalars(8), selection_mask(5), hand(5×17), hand_type(10), deck_counts(52), joker_slots(5×2)]`
- **Action tuple**: MultiDiscrete `[action_type, card_0, …, card_4]`, supports 13 action types
- **Joker system**: 164 Jokers with declarative effect definition system
- **Reward separation**: Rust provides delta info, Python calculates reward functions

## Rust Engine Structure

```
rust-engine/src/
├── main.rs              # gRPC service entry point
├── lib.rs               # Proto imports
├── game/                # Core game logic
│   ├── joker_def.rs     # Declarative Joker effect system (164 Jokers)
│   ├── joker.rs         # Joker implementation with Tiered Architecture
│   ├── scoring.rs       # Scoring engine
│   ├── consumables.rs   # Consumables (Tarot/Planet/Spectral)
│   ├── vouchers.rs      # Voucher permanent upgrades
│   ├── packs.rs         # Card pack system
│   ├── blinds.rs        # Blind/Boss Blind/Ante
│   ├── stakes.rs        # Stake difficulty system
│   ├── tags.rs          # Tag system
│   ├── decks.rs         # Starting decks
│   ├── cards.rs         # Card/Enhancement/Seal/Edition
│   ├── hand_types.rs    # Hand type definitions
│   └── constants.rs     # Game constants
└── service/             # gRPC service layer
    ├── state.rs         # EnvState management
    ├── observation.rs   # Observation vector building
    ├── action_mask.rs   # Legal action mask generation
    └── scoring.rs       # Hand scoring analysis
```

### Action Types

| ID | Name | ID | Name |
|----|------|----|------|
| 0 | SELECT | 7 | REROLL |
| 1 | PLAY | 8 | SELL_JOKER |
| 2 | DISCARD | 9 | SKIP_BLIND |
| 3 | SELECT_BLIND | 10 | USE_CONSUMABLE |
| 4 | CASH_OUT | 11 | BUY_VOUCHER |
| 5 | BUY_JOKER | 12 | BUY_PACK |
| 6 | NEXT_ROUND | | |

## Running the Rust Server

```bash
cd rust-engine
cargo run --release
```

gRPC service listens on `127.0.0.1:50051`. Regenerate Python stubs after modifying `proto/joker_guide.proto`:

```bash
PYTHON_BIN=python3 ./scripts/gen_proto.sh
```

## Python Environment

Install the Python stack inside `python-env`:

```bash
cd python-env
python3 -m pip install .
python3 -m pip install grpcio-tools stable-baselines3 sb3-contrib
```

### Training Scripts

#### `python -m joker_env.train`

- `--episodes`: number of REINFORCE episodes to run (default 50)
- `--checkpoint`: optional path (e.g. `python-env/models/simple.pt`) to persist policy parameters

Example:
```bash
PYTHONPATH=python-env/src python3 -m joker_env.train --episodes 20 --checkpoint python-env/models/simple.pt
```

#### `python -m joker_env.train_sb3`

- `--timesteps`: total MaskablePPO timesteps (default 50000)
- `--checkpoint`: path to save the SB3 model (e.g. `python-env/models/ppo`)
- `--save-interval`: chunk size (default 25000); triggers intermediate snapshots like `python-env/models/ppo_25000`
- `--tensorboard-log`: optional path to write TensorBoard summaries

Example:
```bash
PYTHONPATH=python-env/src python3 -m joker_env.train_sb3 --timesteps 100000 --checkpoint python-env/models/ppo --save-interval 25000 --tensorboard-log python-env/logs/ppo
```

The SB3 script wraps the env with `ActionMasker`, uses a custom features extractor that concatenates selection, hand, deck, and Joker embeddings, chunks the learn loop so very long runs can resume cleanly, and drops per-chunk checkpoints next to `--checkpoint`.

## Experiment Tracking

- Each checkpoint chunk writes a record to `python-env/experiments/checkpoints.jsonl` (created automatically). Every line is JSON with `timestamp`, `checkpoint`, `steps`, `total_timesteps`, and `save_interval`.
- Use `tail -n 5 python-env/experiments/checkpoints.jsonl` to see the latest snapshots.
- Run `scripts/checkpoint_report.py` to print the latest log entries.
- Pipe `scripts/checkpoint_report.py --tail 10` into your dashboard or automation.

## Testing & Verification

```bash
cd rust-engine && cargo test
```

### Test Coverage

| Module | Test Content |
|--------|--------------|
| `game/cards.rs` | Card chips/mults calculation, suit rules, deck/index integrity |
| `game/hand_types.rs` | Hand type mapping, score sanity checks |
| `game/blinds.rs` | Blind/Boss Blind/Ante rule logic |
| `service/action_mask.rs` | State-gating, legal action generation |
| `service/scoring.rs` | Straight baseline, Flint halving, Plasma scoring, Observatory bonus, Selection fallback |

### Additional Tests

- **Scoring regression**: Validates against `references/RLatro/` for FullHouse scoring + Joker bonuses
- **Proptest suite**: Random hand/Joker combinations - chips/mults positivity, Joker multipliers ≥1
- **Edition coverage**: Steel-enhanced FullHouse, Holo+Poly edition, rare Joker combos (`Xm`, `++`, `+$`)

Re-run `cargo test` after modifying scoring, Joker, or proto files.

## Running End-to-End

1. Start the Rust server: `cargo run --release`
2. In another shell, run one of the training commands above
3. When finished, stop the Rust server (`Ctrl+C`)

## Next Steps

- **Integration tests**: Add full blinds/shops flow tests with Joker + Voucher combinations
- **Golden-score fixtures**: Establish fixed expected score test cases to prevent accidental scoring changes during refactoring
- **Reward function tuning**: Experiment with different reward designs in `python-env/src/joker_env/reward.py`
- **Long training runs**: Use `--tensorboard-log` to track training progress with `python-env/experiments/`
- **Reference implementation**: Validate rules against [references/RLatro](references/RLatro/)
