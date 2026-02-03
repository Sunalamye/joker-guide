# Rust Trainer Implementation Plan

從 Python SB3/MaskablePPO 遷移到純 Rust PPO 訓練器的完整實作路線。

> **分析方法**: Super-Analyst-Pro 多輪專家模擬
> **專家小組**: RL 演算法專家、Rust 系統工程師、MLOps 架構師、QA 工程師

---

## Executive Summary

**結論**: Rust 生態沒有可直接替代 SB3 的成熟 PPO 訓練器，需要自研。

**建議路線**: 使用 `tch-rs` + 現有 gRPC 遊戲伺服器，實作 MVP PPO 訓練器，再逐步達成 parity。

**核心風險**: 演算法正確性 > 效能優化。Observation/Action mask 的微小差異會導致訓練靜默失敗。

---

## Framework Decisions

| 決策點 | 建議 | 理由 |
|--------|------|------|
| **訓練框架** | tch-rs | PyTorch parity，成熟 autograd，有 A2C/PG 範例 |
| **通訊層** | gRPC (MVP) | StepBatch 已實作，效能足夠（3.2k-3.7k FPS） |
| **獎勵計算** | 保留 Python | 15+ 組件 + 狀態追蹤（BuildTracker），遷移風險高 |
| **Checkpoint** | safetensors + JSON | 原生 tch-rs 支援，可讀 metadata |

---

## Implementation Phases

### Phase 0: 基準凍結 (1 週)

- [ ] 凍結 SB3 超參數和網路架構
- [ ] 建立 10 個 golden episode 測試
- [ ] 導出 VecNormalize 統計量為 JSON

### Phase 1: Rust Trainer 骨架 (2 週)

- [ ] 建立 `rust-trainer/` crate 結構
- [ ] 實作 tch-rs policy+value 網路
- [ ] 實作 gRPC StepBatch 客戶端
- [ ] CLI 配置（match SB3 超參數）

### Phase 2: PPO 核心 (3 週)

- [ ] GAE 計算（f64 中間值防止漂移）
- [ ] Clipped PPO loss
- [ ] Action masking（-1e8 logits，非 -inf）
- [ ] MultiDiscrete entropy（排除 masked actions）
- [ ] Target KL early stop（1.5x 閾值，per-epoch）
- [ ] VecNormalize 等效（Welford + 10k warmup）

### Phase 3: 驗證 (2 週)

- [ ] Observation parity（1000 states，exact match）
- [ ] Action mask coverage（13 types × 5 stages）
- [ ] Episode reward trajectory（tolerance <1e-5）
- [ ] A/B training（100k steps，win rate ±5%）
- [ ] Performance benchmark（目標 >5000 FPS）

### Phase 4: 效能優化 (1 週)

- [ ] ObservationBuffer 重用（O(1) one-hot）
- [ ] Batch construction（rayon parallel）
- [ ] Profiling + 熱點修復

### Phase 5: 生產就緒 (1 週)

- [ ] Checkpoint 格式定稿
- [ ] TensorBoard 整合（tensorboard-rs）
- [ ] 錯誤恢復（指數退避重試）
- [ ] 文件完成

---

## Repository Structure

```
rust-trainer/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs               # CLI + Tokio runtime
    ├── config.rs             # PPO hyperparams (match SB3)
    ├── grpc/
    │   ├── client.rs         # Tonic client wrapper
    │   └── types.rs          # Proto re-exports
    ├── env/
    │   ├── batch.rs          # StepBatch collector
    │   └── normalize.rs      # VecNormalize equivalent
    ├── model/
    │   ├── network.rs        # Policy + Value net (tch-rs)
    │   └── masking.rs        # Action mask to logits
    ├── ppo/
    │   ├── buffer.rs         # Rollout buffer
    │   ├── gae.rs            # GAE computation (f64)
    │   └── update.rs         # PPO update + KL early stop
    └── io/
        ├── checkpoint.rs     # safetensors + meta.json
        └── logger.rs         # TensorBoard + stdout
```

---

## Hyperparameter Parity

必須完全匹配 SB3 設定：

| Parameter | Value | Notes |
|-----------|-------|-------|
| `learning_rate` | 3e-4 | 標準 PPO |
| `gamma` | 0.99 | 長期信用分配 |
| `gae_lambda` | 0.95 | 減少 bias |
| `clip_range` | 0.2 | PPO clipping |
| `n_steps` | 256 | Rollout buffer |
| `batch_size` | 64 | Mini-batch |
| `n_epochs` | 4 | PPO epochs |
| `ent_coef` | 0.08 → 0.01 | 線性衰減 |
| `target_kl` | 0.015 | Early stop at 1.5x |
| `vf_coef` | 0.5 | Value loss weight |
| `max_grad_norm` | 0.5 | Gradient clipping |
| `clip_reward` | 10.0 | VecNormalize |

---

## Critical Algorithm Details

### 1. Action Masking (MultiDiscrete)

**關鍵**: 使用 `-1e8` 而非 `-inf`，避免 softmax NaN。

```rust
fn sample_with_mask(logits: &[f32], mask: &[bool]) -> (usize, f32) {
    // Apply mask: set invalid logits to -1e8
    let masked_logits: Vec<f32> = logits.iter().zip(mask.iter())
        .map(|(&l, &m)| if m { l } else { -1e8 })
        .collect();

    // Softmax + sample
    let probs = softmax(&masked_logits);
    let action = categorical_sample(&probs);
    let log_prob = probs[action].ln();

    (action, log_prob)
}

// Entropy excludes masked actions
fn masked_entropy(logits: &[f32], mask: &[bool]) -> f32 {
    let probs = softmax_masked(logits, mask);
    probs.iter().zip(mask.iter())
        .map(|(&p, &m)| if m && p > 0.0 { -p * p.ln() } else { 0.0 })
        .sum()
}
```

### 2. GAE Computation (f64 Precision)

**關鍵**: 使用 f64 累積防止長 episode 漂移。

```rust
fn compute_gae(
    rewards: &[f32],
    values: &[f32],
    dones: &[bool],
    last_value: f32,
    gamma: f64,
    gae_lambda: f64,
) -> (Vec<f32>, Vec<f32>) {
    let n = rewards.len();
    let mut advantages = vec![0.0f64; n];
    let mut gae = 0.0f64;

    for t in (0..n).rev() {
        let next_value = if t == n - 1 { last_value as f64 } else { values[t + 1] as f64 };
        let mask = if dones[t] { 0.0 } else { 1.0 };

        let delta = rewards[t] as f64 + gamma * next_value * mask - values[t] as f64;
        gae = delta + gamma * gae_lambda * mask * gae;
        advantages[t] = gae;
    }

    let returns: Vec<f32> = advantages.iter().zip(values.iter())
        .map(|(&a, &v)| (a + v as f64) as f32)
        .collect();
    let advantages_f32: Vec<f32> = advantages.iter().map(|&a| a as f32).collect();

    (advantages_f32, returns)
}
```

### 3. Target KL Early Stopping

**關鍵**: Per-epoch 檢查，使用 1.5x 閾值。

```rust
fn should_stop_early(
    old_log_probs: &Tensor,
    new_log_probs: &Tensor,
    target_kl: f32,
) -> bool {
    let log_ratio = new_log_probs - old_log_probs;
    let ratio = log_ratio.exp();

    // Schulman's approximation: E[(exp(r) - 1) - r]
    let approx_kl = ((ratio - 1.0) - log_ratio).mean().double_value(&[]) as f32;

    approx_kl > 1.5 * target_kl  // 1.5x multiplier
}
```

### 4. VecNormalize (Reward Normalization)

**關鍵**: 使用 discounted return 累積，Welford 算法更新。

```rust
pub struct RewardNormalizer {
    pub ret_rms: RunningMeanStd,
    pub returns: Vec<f64>,      // Per-env discounted returns
    pub gamma: f64,
    pub clip_reward: f64,
    pub warmup_steps: usize,
}

impl RewardNormalizer {
    pub fn normalize_rewards(&mut self, rewards: &[f64], dones: &[bool]) -> Vec<f64> {
        // 1. Accumulate: returns = returns * gamma + reward
        for i in 0..rewards.len() {
            self.returns[i] = self.returns[i] * self.gamma + rewards[i];
        }

        // 2. Update running stats (Welford)
        self.ret_rms.update(&self.returns);

        // 3. Normalize: reward / sqrt(var + eps)
        let std = (self.ret_rms.var + 1e-8).sqrt();
        let normalized: Vec<f64> = rewards.iter()
            .map(|&r| (r / std).clamp(-self.clip_reward, self.clip_reward))
            .collect();

        // 4. Reset returns for done envs
        for (i, &done) in dones.iter().enumerate() {
            if done { self.returns[i] = 0.0; }
        }

        normalized
    }
}
```

**SB3 統計量導入**:

```python
# Python: 導出 VecNormalize 統計量
stats = {
    "ret_rms": {
        "mean": float(env.ret_rms.mean),
        "var": float(env.ret_rms.var),
        "count": float(env.ret_rms.count),
    },
    "gamma": 0.99,
    "clip_reward": 10.0,
}
json.dump(stats, open("vecnorm_stats.json", "w"))
```

---

## Observation Buffer Optimization

**問題**: 現有 observation 建構有 750+ one-hot 迴圈。

**解決**: O(1) single-index writes + buffer 重用。

```rust
#[repr(align(64))]
pub struct ObservationBuffer {
    data: [f32; 1543],  // OBS_SIZE
}

impl ObservationBuffer {
    pub fn build_from_state(&mut self, state: &EnvState) -> &[f32] {
        self.data.fill(0.0);  // SIMD-friendly clear

        // Single write for Joker ID one-hot (instead of 150 iterations!)
        for (slot, joker) in state.jokers.iter().enumerate() {
            let offset = JOKER_OFFSET + slot * 153;
            self.data[offset + joker.id.to_index()] = 1.0;  // O(1)
        }

        &self.data
    }
}
```

**預期效果**: 70x fewer branches，消除 heap allocation。

---

## Verification Strategy

### Parity Tests

| Test | Method | Tolerance |
|------|--------|-----------|
| Observation shape | Assert `obs.shape == (1543,)` | Exact |
| Feature ordering | Byte-level comparison | Exact |
| Action mask | 1M random steps, 0 invalid actions | 100% |
| Per-step reward | Replay golden episodes | 1e-6 |
| Episode total | Compare cumulative | 1e-4 |
| Training win rate | A/B vs SB3 @ 100k steps | ±5% |

### Golden Episode Test Cases

| ID | Scenario | State Transitions |
|----|----------|-------------------|
| E001 | BuildTracker threshold | 5 FLUSH → dominant_build activates |
| E002 | Consecutive discard | 4+ DISCARDs → progressive penalty |
| E003 | State reset on PLAY | DISCARD×3 → PLAY → counter=0 |
| E004 | Boss difficulty bonus | Beat BOSS_WALL with 3 plays_left |
| E005 | Joker buy failure | BUY_JOKER, joker_count unchanged |
| E006 | v10.0 reroll budget | 5 REROLLs → progressive penalty |
| E007 | Skip with context | Low joker + high shop_quality |
| E008 | Ante milestone | Ante 2→3 → +0.3 bonus |
| E009 | Game end scaling | LOSE at Ante 1 vs Ante 7 |
| E010 | Joker synergy | 2+ pair_power Jokers → CASH_OUT bonus |

---

## Risk Matrix

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Observation 順序不匹配 | Medium | High (silent) | 1000 states parity test |
| VecNormalize 不一致 | High | Medium | Import SB3 stats, 10k warmup |
| Action mask 邊界錯誤 | Medium | High | 1M random policy validation |
| libtorch 依賴複雜 | Low | Low | Docker build environment |
| Reward 遷移錯誤 | High | High | 保留 Python，不遷移 |

---

## Success Criteria

| Metric | Target |
|--------|--------|
| Observation parity | 100% exact match (1543 dims) |
| Action mask validity | 0 invalid actions / 1M steps |
| Reward trajectory | <1e-5 per-step tolerance |
| Training win rate | ±5% vs SB3 @ 100k steps |
| FPS | >5000 (vs Python ~1200) |

---

## Dependencies

### Required (MVP)

```toml
[dependencies]
tch = "0.14"           # PyTorch bindings
tonic = "0.10"         # gRPC client
prost = "0.12"         # Protobuf
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rand = "0.8"
rand_distr = "0.4"
```

### Optional (Phase 2+)

```toml
[dependencies]
tensorboard-rs = "0.2"  # TensorBoard logging
rayon = "1.8"           # Parallel observation construction
safetensors = "0.4"     # Checkpoint format
```

---

## Checkpoint Format

```
checkpoints/
├── model_step_100000.safetensors   # tch-rs VarStore weights
└── model_step_100000.meta.json     # Metadata
```

**meta.json schema**:

```json
{
  "step": 100000,
  "hyperparams": {
    "learning_rate": 3e-4,
    "gamma": 0.99,
    "gae_lambda": 0.95,
    "clip_range": 0.2,
    "ent_coef": 0.05,
    "target_kl": 0.015
  },
  "reward_normalization": {
    "mean": 0.0,
    "var": 1.234,
    "count": 50000.0
  },
  "metrics": {
    "win_rate": 0.15,
    "avg_ante": 3.2,
    "fps": 5500
  }
}
```

---

## References

- [PPO Implementation Details (37 details)](https://iclr-blog-track.github.io/2022/03/25/ppo-implementation-details/)
- [MaskablePPO Source](https://github.com/Stable-Baselines-Team/stable-baselines3-contrib/blob/master/sb3_contrib/ppo_mask/ppo_mask.py)
- [VecNormalize Source](https://github.com/DLR-RM/stable-baselines3/blob/master/stable_baselines3/common/vec_env/vec_normalize.py)
- [tch-rs Documentation](https://docs.rs/tch/latest/tch/)
- [Candle (Alternative)](https://huggingface.github.io/candle/)

---

*Generated by Super-Analyst-Pro multi-expert analysis*
