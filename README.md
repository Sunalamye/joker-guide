# Joker Guide RL

This repository ships a Rust-based Balatro-like engine + Python training surface.  
The Rust server exposes a gRPC `JokerEnv` and emits fixed-length vector observations. The Python side wraps that vector into Gym-style observations, tracks action masks, and offers two training scripts (simple REINFORCE and MaskablePPO with Joker embeddings).

## Key Concepts
- **Observation vector (length 170)**: `[scalars(8), selection_mask(5), hand(5×17), hand_type(10), deck_counts(52), joker_slots(5×2)]`.  
  Scalars include chips/mult/blind/episode/hand/discard counters plus deck/joker heap sizes.
- **Selection mask**: the next 5 values correspond to the currently highlighted cards before playing. This allows multi-step actions (select → discard/play).
- **Action tuple**: MultiDiscrete `[action_type (play=0, discard=1, select=2), card_0, …, card_4]`. `JokerGymDictEnv` builds action masks per step, enabling only valid paths.  
- **Joker effects**: On reset the engine seeds five Joker slots with random type IDs (`+c`, `+m`, `Xm`, etc.). Each enabled Joker adjusts scoring — chips, additive mult, or multiplicative mult — so the RL agent learns which Jokers to hoard or drop.

## Running the Rust server

```bash
cd rust-engine
cargo run
```

It will compile the `joker_env` binary and open a gRPC server on `127.0.0.1:50051`. Make sure you regenerate the Python stubs whenever you touch `proto/joker_guide.proto`:

```bash
PYTHON_BIN=python3 ./scripts/gen_proto.sh
```

## Python environment

Install the Python stack inside `python-env`:

```bash
cd python-env
python3 -m pip install .
python3 -m pip install grpcio-tools stable-baselines3 sb3-contrib
```

### Training scripts

#### `python -m joker_env.train`

- `--episodes`: number of REINFORCE episodes to run (default 50).  
- `--checkpoint`: optional path (e.g. `python-env/models/simple.pt`) to persist the policy parameters.  

Example:
```bash
PYTHONPATH=python-env/src python3 -m joker_env.train --episodes 20 --checkpoint python-env/models/simple.pt
```

#### `python -m joker_env.train_sb3`

- `--timesteps`: total MaskablePPO timesteps (default 50000).  
- `--checkpoint`: path to save the SB3 model (e.g. `python-env/models/ppo`).  
- `--save-interval`: chunk size (default 25000); triggers intermediate snapshots like `python-env/models/ppo_25000`.  
- `--tensorboard-log`: optional path to write TensorBoard summaries.

Example:
```bash
PYTHONPATH=python-env/src python3 -m joker_env.train_sb3 --timesteps 100000 --checkpoint python-env/models/ppo --save-interval 25000 --tensorboard-log python-env/logs/ppo
```

The SB3 script wraps the env with `ActionMasker`, uses a custom features extractor that concatenates selection, hand, deck, and Joker embeddings, chunks the learn loop so very long runs can resume cleanly, and drops per-chunk checkpoints next to `--checkpoint`.

## Experiment tracking

- Each checkpoint chunk also writes a record to `python-env/experiments/checkpoints.jsonl` (created automatically). Every line is JSON with `timestamp`, `checkpoint`, `steps`, `total_timesteps`, and `save_interval`, so you can correlate runs with `MaskablePPO` statistics for later analysis.
- Use `tail -n 5 python-env/experiments/checkpoints.jsonl` (or load it in a notebook) to see the latest snapshots; pass the most recent `python-env/models/ppo_<steps>` path back to `joker_env.train_sb3 --checkpoint` to resume or compare results.
- Run `scripts/checkpoint_report.py` to print the latest log entries, which feeds directly into your experiment tracker alongside the SB3 checkpoints.
- Pipe `scripts/checkpoint_report.py --tail 10` into your dashboard or automation (for example `scripts/checkpoint_report.py --tail 10 > python-env/experiments/latest_checkpoints.txt`), so the JSONL rows can be imported by whatever tool tracks your MaskablePPO metrics/checkpoints.

## Testing & verification

- `cargo test` now runs the scoring regression that mirrors the known cases in `references/RLatro/RLatro.Test/CoreRules/HandEvaluationTest.cs`, verifying the FullHouse scoring + Joker bonuses before the Python wrapper sees the obs/actions.  
- The same command also executes the `proptest` suite so random hands and Joker configurations keep chips/mults positive, Joker multipliers ≥1, and `build_selected_hand` never drops cards.  
- New Rust coverage now includes the Steel-enhanced FullHouse, the rare edition FullHouse (Holo+Poly), and the rare Joker combo (`Xm`, `++`, `+$`), so the scoring logic remains synced with the RLatro reference as more Joker behaviors arrive.
- New Rust coverage now includes the Steel-enhanced FullHouse and the rare Joker combo (`Xm`, `++`, `+$`), keeping the scoring logic aligned with the RLatro reference as additional Joker behaviors arrive.
- Re-run `cargo test` after touching scoring, Joker metadata, or the RPC proto to keep the verified workflow in sync with the `RLatro` reference.

## Running end-to-end

1. Start the Rust server: `cargo run`  
2. In another shell, run one of the training commands above.  
3. When finished, stop the Rust server (`Ctrl+C`).  

## Next steps

- Complete logic / scoring details by cross-checking [references/RLatro](references/RLatro/), a deterministic Balatro re-implementation that documents deck-builder rules, Joker handling, and reward signals — the new edition/bonus system is already inspired by that codebase. Use it as a reference when enriching the Rust engine or verifying Python obs/actions.
- Hook `joker_env.train_sb3` checkpoints + metrics into your experimentation directory (`python-env/models/`) for longer runs.
- Hook the Joker metadata to real game modifiers (the Rust engine already seeds types).  
- Extend the action set with shop/select phases (the engine exposes `select` as action_type 2).  
- Tune the training scripts: higher timesteps, more complex policies, or TensorBoard logging.
