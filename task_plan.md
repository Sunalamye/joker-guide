# Task Plan: Joker integration + Dict observation + Python training

## Goal
Add Joker slots/state to the Rust engine, expose a Dict-style observation in Python, and provide a training script with Joker embeddings.

## Phases
- [x] Phase 1: Define observation layout and Joker slot contract
- [x] Phase 2: Implement Joker loading + state in Rust
- [x] Phase 3: Provide Dict observation wrapper in Python
- [x] Phase 4: Add Python training script with embeddings
- [x] Phase 5: Verify build/run steps and document usage
- [x] Phase 6: Add select action type + selection mask features
- [x] Phase 7: Document controls / next enhancements
- [x] Phase 8: Integrate RLatro scoring + Joker edition logic
- [x] Phase 9: Update docs, training scripts, checkpoints

## Key Questions
1. Joker slots fixed at 5, or should we allow fewer/more?
2. Should Joker IDs be raw numeric IDs or mapped to contiguous embedding indices?
3. Do we keep the proto tensor flat and split in Python, or add structured fields?

## Decisions Made
- Use discard-mask (0..31) as the MVP action space for a 5-card hand.
- Reward = base_chips × base_mult from standard poker hand scoring.
- Add blind-target reward shaping aligned with balatro-rl's ante-1 focus.
- Python wrapper exposes MultiDiscrete actions (play/discard + card bits).
- Observation layout expanded to include scalars, hand one-hot, hand type, deck counts, and joker slots.
- Joker slots will be represented as (id, enabled) pairs in the observation vector.
- Joker IDs map to contiguous embedding indices via `data/joker-ids.json`.
- Added an `action_type` flag (play/discard/select) so the RL agent can stage multi-step moves.

## Errors Encountered
- cargo build failed: `protoc` not found (install protobuf or set `PROTOC`).
- `scripts/gen_proto.sh` failed: Python has no `grpc_tools` module.
- First server launch failed: address already in use (resolved by stopping old process).
- `sb3_contrib` ActionMasker import path mismatch (fixed by using `sb3_contrib.common.wrappers`).

## Status
**Status** - all phases complete; RLatro-inspired scoring, documentation, and training checkpoints are in place.
