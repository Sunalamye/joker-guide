# Rust Trainer Implementation Plan

從 Python SB3/MaskablePPO **完全遷移**到純 Rust PPO 訓練器的完整實作路線。

> **分析方法**: Super-Analyst-Pro 多輪專家模擬（2 輪迭代）
> **專家小組**: RL 演算法專家、Rust 系統工程師、MLOps 架構師、遊戲 AI 開發者
> **更新日期**: 2026-02-03

---

## Executive Summary

**結論**: Rust 生態沒有可直接替代 SB3 的成熟 PPO 訓練器，需要自研。**包含獎勵計算的完全遷移**。

**建議路線**: 使用 `tch-rs` + Metal/Accelerate 加速，實作完整 PPO 訓練器（含獎勵系統）。

**核心風險**: 演算法正確性 > 效能優化。獎勵函數的 BuildTracker 狀態同步是最高風險點。

**時程估計**: 13 週（原 10 週 + 獎勵遷移 3 週）

---

## Framework Decisions

| 決策點 | 建議 | 理由 |
|--------|------|------|
| **訓練框架** | tch-rs | PyTorch parity，成熟 autograd，MPS 支援 |
| **通訊層** | 直接呼叫（無 gRPC） | 完全 Rust 化後無需跨進程通訊 |
| **獎勵計算** | **遷移到 Rust** | 完全 Rust 化，消除 Python 依賴 |
| **GPU 加速** | Metal (MPS) + Accelerate | Mac 原生加速，vDSP 向量運算 |
| **Checkpoint** | safetensors + JSON | 原生 tch-rs 支援，可讀 metadata |

---

## Metal / Accelerate 加速策略

### 加速點分析

| 組件 | 瓶頸類型 | Metal (MPS) | Accelerate (vDSP) | 建議 |
|------|----------|-------------|-------------------|------|
| **Policy Forward** | GPU-bound | **高收益** | - | MPS |
| **Policy Backward** | GPU-bound | **高收益** | - | MPS |
| **GAE 計算** | CPU-bound | 低 | **高收益** | vDSP 向量化 |
| **Softmax + Mask** | CPU-bound | 中 | **中收益** | vDSP |
| **Observation 建構** | CPU-bound | 無 | 無 | SIMD buffer |
| **VecNormalize** | CPU-bound | 低 | **中收益** | vDSP |

### tch-rs MPS 配置

```bash
# 需要 Python PyTorch 安裝（無獨立 LibTorch MPS）
export LIBTORCH_USE_PYTORCH=1
export PYTORCH_ENABLE_MPS_FALLBACK=1  # 處理不支援的操作
```

```rust
// src/model/device.rs
use tch::Device;

pub fn get_device() -> Device {
    if tch::utils::has_mps() {
        Device::Mps
    } else {
        Device::Cpu
    }
}
```

### Accelerate 整合

```toml
# Cargo.toml (macOS specific)
[target.'cfg(target_os = "macos")'.dependencies]
accelerate-src = { version = "0.3", features = ["accelerate"] }
```

```rust
// src/ppo/gae_accelerate.rs
#[cfg(target_os = "macos")]
use accelerate::vdsp::*;

/// vDSP 加速的 GAE 計算
pub fn compute_gae_accelerated(
    rewards: &[f64],
    values: &[f64],
    dones: &[bool],
    last_value: f64,
    gamma: f64,
    gae_lambda: f64,
) -> (Vec<f64>, Vec<f64>) {
    let n = rewards.len();
    let mut advantages = vec![0.0f64; n];
    let mut gae = 0.0f64;

    // 反向累積（vDSP 可加速中間向量運算）
    for t in (0..n).rev() {
        let next_value = if t == n - 1 { last_value } else { values[t + 1] };
        let mask = if dones[t] { 0.0 } else { 1.0 };

        let delta = rewards[t] + gamma * next_value * mask - values[t];
        gae = delta + gamma * gae_lambda * mask * gae;
        advantages[t] = gae;
    }

    // vDSP 向量加法計算 returns
    let mut returns = vec![0.0f64; n];
    unsafe {
        vDSP_vaddD(
            advantages.as_ptr(), 1,
            values.as_ptr(), 1,
            returns.as_mut_ptr(), 1,
            n as u64,
        );
    }

    (advantages, returns)
}
```

### 效能預期

| 配置 | 預期 FPS | 說明 |
|------|----------|------|
| Python SB3 + gRPC | ~1,200 | 基準 |
| Rust + gRPC | 3,200-3,700 | 當前 StepBatch |
| Rust + MPS (forward only) | 5,000-7,000 | GPU 加速推論 |
| Rust + MPS + Accelerate | **8,000-12,000** | 完全優化 |

---

## Implementation Phases

### Phase 0: 基準凍結 (1 週)

- [ ] 凍結 SB3 超參數和網路架構
- [ ] 建立 **100 個** golden episode 測試（原 10 個不足）
- [ ] 導出 VecNormalize 統計量為 JSON
- [ ] 導出 reward.py 每步獎勵作為驗證基準

### Phase 1: Rust Trainer 骨架 (2 週)

- [ ] 建立 `rust-trainer/` crate 結構
- [ ] 實作 tch-rs policy+value 網路（MPS 支援）
- [ ] 移除 gRPC，直接呼叫 `rust-engine`
- [ ] CLI 配置（match SB3 超參數）

### Phase 2: PPO 核心 (4 週) ⚠️ 原 3 週，增加緩衝

- [ ] GAE 計算（f64 + Accelerate vDSP）
- [ ] Clipped PPO loss
- [ ] Action masking（-1e8 logits，非 -inf）
- [ ] **MaskedMultiCategorical** 分佈（見下方實作）
- [ ] MultiDiscrete entropy（排除 masked actions）
- [ ] Target KL early stop（1.5x 閾值，per-epoch）
- [ ] VecNormalize 等效（Welford + 10k warmup）

### Phase 3: 獎勵系統遷移 (3 週) 🆕

分三波次遷移 `reward.py`（800+ 行）：

| 波次 | 組件 | 工作量 | 風險 |
|------|------|--------|------|
| **Wave 1** | 常量 + 無狀態函數 | 2 天 | 低 |
| **Wave 2** | 單步狀態函數 | 1.5 天 | 中 |
| **Wave 3** | 跨步狀態（BuildTracker） | 2 天 | **高** |

#### Wave 1: 常量 + 無狀態函數 (2 天)

- [ ] ACTION_TYPE_*, STAGE_*, BOSS_*, TAG_VALUES 常量
- [ ] `hand_type_bonus()`, `game_end_reward()`, `clamp()`
- [ ] `stage_weight_early()`, `stage_weight_late()`
- [ ] `get_tag_value()`, BOSS_DIFFICULTY 映射

#### Wave 2: 單步狀態函數 (1.5 天)

- [ ] `play_reward()`, `discard_reward()`
- [ ] `blind_clear_reward()`, `ante_progress_reward()`
- [ ] `money_reward()`, `joker_buy_reward()`, `sell_joker_reward()`
- [ ] `consumable_use_reward()`, `voucher_buy_reward()`

#### Wave 3: 跨步狀態 (2 天) ⚠️ 最高風險

- [ ] `BuildTracker` 結構體遷移
- [ ] `consecutive_discard` 計數器
- [ ] `skip_blind_reward_v2()` + context multiplier
- [ ] `reroll_reward_v2()` + shop_quality 整合
- [ ] `joker_synergy_reward()` + JOKER_SYNERGY_GROUPS

### Phase 4: 驗證 (3 週) ⚠️ 原 2 週，增加獎勵驗證

- [ ] Observation parity（1000 states，exact match）
- [ ] Action mask coverage（13 types × 5 stages）
- [ ] **Reward parity（100 golden episodes，tolerance <1e-6）** 🆕
- [ ] **BuildTracker state parity** 🆕
- [ ] Episode reward trajectory（tolerance <1e-5）
- [ ] A/B training（100k steps，win rate ±5%）
- [ ] Performance benchmark（目標 >8000 FPS）

### Phase 5: 效能優化 (1 週)

- [ ] ObservationBuffer 重用（O(1) one-hot）
- [ ] Batch construction（rayon parallel）
- [ ] Metal MPS 啟用驗證
- [ ] Accelerate vDSP 整合
- [ ] Profiling + 熱點修復

### Phase 6: 生產就緒 (1 週)

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
    ├── device.rs             # MPS/CPU device selection
    ├── env/
    │   ├── batch.rs          # Direct batch collector (no gRPC)
    │   └── normalize.rs      # VecNormalize equivalent
    ├── model/
    │   ├── network.rs        # Policy + Value net (tch-rs + MPS)
    │   └── masking.rs        # MaskedMultiCategorical
    ├── ppo/
    │   ├── buffer.rs         # Rollout buffer
    │   ├── gae.rs            # GAE computation (f64 + Accelerate)
    │   └── update.rs         # PPO update + KL early stop
    ├── reward/               # 🆕 獎勵系統
    │   ├── mod.rs            # RewardCalculator
    │   ├── constants.rs      # ACTION_TYPE_*, BOSS_*, TAG_VALUES
    │   ├── build_tracker.rs  # BuildTracker 狀態追蹤
    │   ├── components.rs     # 15+ 獎勵組件
    │   └── synergy.rs        # Joker 協同群組
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

### 1. MaskedMultiCategorical Distribution 🆕

**關鍵**: 完整實作 MultiDiscrete action masking，支援 46 維 mask。

```rust
// src/model/masking.rs
use tch::{Tensor, Kind, Device};

pub const ACTION_PARAM_SIZES: &[i64] = &[
    13,           // action_type
    2, 2, 2, 2, 2, 2, 2, 2,  // card selection (8 binary)
    3,            // blind selection
    2,            // shop joker purchase slot
    5,            // sell joker slot
    1,            // reroll
    1,            // skip blind
    2,            // use consumable slot
    1,            // buy voucher
    2,            // buy pack slot
];

pub struct MaskedMultiCategorical {
    logits: Vec<Tensor>,
    masks: Vec<Tensor>,
}

impl MaskedMultiCategorical {
    pub fn new(logits: &Tensor, mask: &Tensor) -> Self {
        let mut split_logits = Vec::new();
        let mut split_masks = Vec::new();
        let mut offset = 0i64;

        for &n in ACTION_PARAM_SIZES {
            let l = logits.narrow(1, offset, n);
            let m = mask.narrow(1, offset, n);

            // Apply mask: invalid actions get -1e8 (not -inf to avoid NaN)
            let masked_l = &l + m.logical_not().to_kind(Kind::Float) * (-1e8);

            split_logits.push(masked_l);
            split_masks.push(m);
            offset += n;
        }

        Self { logits: split_logits, masks: split_masks }
    }

    pub fn sample(&self) -> (Tensor, Tensor) {
        let mut actions = Vec::new();
        let mut log_probs = Vec::new();

        for logits in &self.logits {
            let probs = logits.softmax(-1, Kind::Float);
            let action = probs.multinomial(1, true);
            let log_prob = probs.gather(-1, &action, false).log();

            actions.push(action);
            log_probs.push(log_prob);
        }

        let actions = Tensor::cat(&actions, 1);
        let log_probs = Tensor::cat(&log_probs, 1)
            .sum_dim_intlist(&[-1i64][..], false, Kind::Float);

        (actions, log_probs)
    }

    /// Entropy 必須排除 masked actions（per-dimension 計算）
    pub fn entropy(&self) -> Tensor {
        let mut entropies = Vec::new();

        for (logits, mask) in self.logits.iter().zip(&self.masks) {
            let probs = logits.softmax(-1, Kind::Float);
            let log_probs = (probs.clamp_min(1e-10)).log();

            // Only compute entropy for valid actions
            let masked_entropy = (&probs * &log_probs * mask.to_kind(Kind::Float))
                .sum_dim_intlist(&[-1i64][..], false, Kind::Float)
                .neg();

            entropies.push(masked_entropy);
        }

        Tensor::stack(&entropies, 1)
            .sum_dim_intlist(&[-1i64][..], false, Kind::Float)
    }

    pub fn log_prob(&self, actions: &Tensor) -> Tensor {
        let mut log_probs = Vec::new();

        for (i, logits) in self.logits.iter().enumerate() {
            let action = actions.narrow(1, i as i64, 1);
            let probs = logits.softmax(-1, Kind::Float);
            let log_prob = probs.gather(-1, &action, false).log();
            log_probs.push(log_prob);
        }

        Tensor::cat(&log_probs, 1)
            .sum_dim_intlist(&[-1i64][..], false, Kind::Float)
    }
}
```

### 2. BuildTracker Rust 實作 🆕

**關鍵**: 與 Python 完全一致的狀態追蹤。

```rust
// src/reward/build_tracker.rs
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Build {
    Pairs,
    Straight,
    Flush,
}

// Hand type to build mapping (from Python BUILD_HANDS)
const PAIR_HANDS: &[i32] = &[1, 2, 3, 6, 7, 10, 11];      // Pair, TwoPair, ThreeKind, FullHouse, FourKind, FiveKind, FlushHouse
const STRAIGHT_HANDS: &[i32] = &[4, 8, 9];                 // Straight, StraightFlush, RoyalFlush
const FLUSH_HANDS: &[i32] = &[5, 8, 9, 11, 12];           // Flush, StraightFlush, RoyalFlush, FlushHouse, FlushFive

#[derive(Default, Clone)]
pub struct BuildTracker {
    pairs: u32,
    straight: u32,
    flush: u32,
    total_hands: u32,
}

impl BuildTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.pairs = 0;
        self.straight = 0;
        self.flush = 0;
        self.total_hands = 0;
    }

    pub fn record_hand(&mut self, hand_type: i32) {
        if hand_type < 0 {
            return;
        }
        self.total_hands += 1;

        if PAIR_HANDS.contains(&hand_type) {
            self.pairs += 1;
        }
        if STRAIGHT_HANDS.contains(&hand_type) {
            self.straight += 1;
        }
        if FLUSH_HANDS.contains(&hand_type) {
            self.flush += 1;
        }
    }

    /// 60% 閾值 + 最少 5 個樣本
    pub fn get_dominant_build(&self) -> Option<Build> {
        if self.total_hands < 5 {
            return None;
        }

        let threshold = (self.total_hands as f32 * 0.6) as u32;

        if self.pairs >= threshold {
            Some(Build::Pairs)
        } else if self.straight >= threshold {
            Some(Build::Straight)
        } else if self.flush >= threshold {
            Some(Build::Flush)
        } else {
            None
        }
    }

    pub fn get_build_weights(&self) -> (f32, f32, f32) {
        if self.total_hands == 0 {
            return (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0);
        }
        let total = self.total_hands as f32;
        (
            self.pairs as f32 / total,
            self.straight as f32 / total,
            self.flush as f32 / total,
        )
    }
}
```

### 3. GAE Computation (f64 Precision)

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

### 4. Target KL Early Stopping

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

### 5. VecNormalize (Reward Normalization)

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
| **Reward parity** | 100 golden episodes | **<1e-6** |
| **BuildTracker state** | Per-step state comparison | Exact |
| Episode total | Compare cumulative | 1e-4 |
| Training win rate | A/B vs SB3 @ 100k steps | ±5% |

### Golden Episode Test Cases (100 個)

| Category | Count | Focus |
|----------|-------|-------|
| BuildTracker 轉換 | 20 | 60% 閾值 + 5 樣本啟動 |
| Consecutive discard | 15 | PLAY 重置計數器 |
| Boss 難度 | 15 | 27 種 Boss，特別是 Wall/Needle/Violet |
| Joker 交易 | 20 | 購買成功/失敗，賣出保護，協同群組 |
| v10.0 Reroll/Skip | 15 | shop_quality 整合，context multiplier |
| 邊界案例 | 15 | Ante 轉換，遊戲結束縮放 |

### Regression Test Suite

```rust
#[cfg(test)]
mod regression_tests {
    use super::*;

    // R001: BuildTracker 60% 閾值
    #[test]
    fn test_build_tracker_threshold() {
        let mut tracker = BuildTracker::new();
        // 4 FLUSH out of 7 = 57% - should NOT activate
        for _ in 0..4 { tracker.record_hand(HAND_FLUSH); }
        for _ in 0..3 { tracker.record_hand(HAND_PAIR); }
        assert_eq!(tracker.get_dominant_build(), None);

        // Add 1 more FLUSH = 5/8 = 62.5% - should activate
        tracker.record_hand(HAND_FLUSH);
        assert_eq!(tracker.get_dominant_build(), Some(Build::Flush));
    }

    // R002: Consecutive discard reset on PLAY
    #[test]
    fn test_consecutive_discard_reset() {
        let mut calc = RewardCalculator::new();
        // DISCARD 3 times
        for _ in 0..3 {
            calc.calculate(&mock_step_info(ACTION_TYPE_DISCARD));
        }
        assert_eq!(calc.consecutive_discards, 3);

        // PLAY resets counter
        calc.calculate(&mock_step_info(ACTION_TYPE_PLAY));
        assert_eq!(calc.consecutive_discards, 0);
    }

    // R003: Boss difficulty bonus threshold
    #[test]
    fn test_boss_difficulty_threshold() {
        // BOSS_HOOK (difficulty 0.8) - no bonus
        assert_eq!(boss_clear_difficulty_bonus(BOSS_HOOK, 3), 0.0);

        // BOSS_WALL (difficulty 1.4) - bonus
        let bonus = boss_clear_difficulty_bonus(BOSS_WALL, 3);
        assert!(bonus > 0.0 && bonus <= 0.15);
    }

    // R004: Joker sell protection
    #[test]
    fn test_joker_sell_protection() {
        // 1 Joker - maximum penalty
        assert!((sell_joker_reward(5, 1, 1, 5, -1) - (-0.5)).abs() < 1e-6);

        // 2 Jokers - heavy penalty
        assert!((sell_joker_reward(5, 1, 2, 5, -1) - (-0.3)).abs() < 1e-6);
    }
}
```

---

## Risk Matrix

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Observation 順序不匹配 | Medium | High (silent) | 1000 states parity test |
| VecNormalize 不一致 | High | Medium | Import SB3 stats, 10k warmup |
| Action mask 邊界錯誤 | Medium | High | 1M random policy validation |
| **BuildTracker 狀態同步** | **High** | **High** | Per-step state comparison |
| **獎勵組件遺漏** | **Medium** | **High** | 100 golden episodes |
| libtorch 依賴複雜 | Low | Low | Docker build environment |
| MPS 操作不支援 | Medium | Medium | FALLBACK=1 + 測試 |

---

## Success Criteria

| Metric | Target |
|--------|--------|
| Observation parity | 100% exact match (1543 dims) |
| Action mask validity | 0 invalid actions / 1M steps |
| **Reward parity** | **<1e-6 per-step (100 golden episodes)** |
| **BuildTracker parity** | **100% state match** |
| Training win rate | ±5% vs SB3 @ 100k steps |
| FPS | **>8000** (vs Python ~1200) |

---

## Dependencies

### Required (MVP)

```toml
[dependencies]
tch = "0.14"           # PyTorch bindings (MPS support)
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rand = "0.8"
rand_distr = "0.4"

# Direct integration with rust-engine
rust-engine = { path = "../rust-engine" }
```

### macOS Acceleration

```toml
[target.'cfg(target_os = "macos")'.dependencies]
accelerate-src = { version = "0.3", features = ["accelerate"] }
```

### Optional (Phase 5+)

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
  "build_tracker": {
    "pairs": 45,
    "straight": 12,
    "flush": 23,
    "total_hands": 80
  },
  "metrics": {
    "win_rate": 0.15,
    "avg_ante": 3.2,
    "fps": 8500
  }
}
```

---

## Timeline Summary

| Phase | Duration | Cumulative |
|-------|----------|------------|
| Phase 0: 基準凍結 | 1 週 | 1 週 |
| Phase 1: Trainer 骨架 | 2 週 | 3 週 |
| Phase 2: PPO 核心 | 4 週 | 7 週 |
| **Phase 3: 獎勵遷移** | **3 週** | **10 週** |
| Phase 4: 驗證 | 3 週 | 13 週 |
| Phase 5: 效能優化 | - | (含在驗證中) |
| Phase 6: 生產就緒 | - | (含在驗證中) |
| **Total** | **13 週** | |

---

## References

- [PPO Implementation Details (37 details)](https://iclr-blog-track.github.io/2022/03/25/ppo-implementation-details/)
- [MaskablePPO Source](https://github.com/Stable-Baselines-Team/stable-baselines3-contrib/blob/master/sb3_contrib/ppo_mask/ppo_mask.py)
- [VecNormalize Source](https://github.com/DLR-RM/stable-baselines3/blob/master/stable_baselines3/common/vec_env/vec_normalize.py)
- [tch-rs Documentation](https://docs.rs/tch/latest/tch/)
- [PyTorch MPS Backend](https://docs.pytorch.org/docs/stable/notes/mps.html)
- [Apple Accelerate Framework](https://developer.apple.com/documentation/accelerate)
- [accelerate-src Crate](https://github.com/blas-lapack-rs/accelerate-src)
- [A Closer Look at Invalid Action Masking](https://costa.sh/blog-a-closer-look-at-invalid-action-masking-in-policy-gradient-algorithms.html)

---

*Generated by Super-Analyst-Pro multi-expert analysis (2 rounds, 4 experts)*
