# Notes: Balatro RL reference points

## Sources
### balatro-rl
- Path: `references/balatro-rl/ai/environment/balatro_env.py`
- Key points:
  - Uses Gymnasium wrapper with action masks.
  - Action model: select/play/discard with card indices.
  - Observation size is fixed-length vector derived from game state.
- Path: `references/balatro-rl/ai/utils/mappers.py`
- Key points:
  - Hand and action mapping logic.
  - One-hot encoding for cards and hand types.
  - Action space sliced into action selection + card bits.
- Path: `references/balatro-rl/ai/environment/reward.py`
- Key points:
  - Reward shaping around blind requirement thresholds.

## Synthesized Findings
- Keep a stable, fixed-length vector in the engine; split into Dict observation in Python for embedding and clarity.
- Use Joker slots as categorical IDs, embed in Python, and optionally add enabled/disabled flags.
- Reward shaping tied to blind threshold helps early training stability.
