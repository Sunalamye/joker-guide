# Joker Guide - Balatro RL Training Project

## Architecture Overview

本專案採用 **Rust + Python 分離架構**，用於訓練 Balatro 遊戲的強化學習 AI。

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

---

## Rust Engine (`rust-engine/`)

**職責：純遊戲環境，不計算獎勵**

### 核心功能
- 遊戲狀態管理（牌組、手牌、Joker、商店）
- 動作驗證與執行
- Action Mask 生成（合法動作遮罩）
- gRPC 服務端

### 返回數據（EnvInfo）
```protobuf
message EnvInfo {
  // 基本狀態
  int32 episode_step = 1;
  int64 chips = 2;           // 當前分數
  int64 blind_target = 4;    // 目標分數

  // 遊戲階段
  int32 ante = 5;            // 1-8
  int32 stage = 6;           // 0=PreBlind, 1=Blind, 2=PostBlind, 3=Shop, 4=End
  int32 blind_type = 7;      // 0=Small, 1=Big, 2=Boss, -1=None
  int32 plays_left = 8;
  int32 discards_left = 9;
  int32 money = 10;

  // Delta 追蹤（供 Python 計算獎勵）
  int64 score_delta = 11;    // 這次動作獲得的分數
  int32 money_delta = 12;    // 這次動作的金幣變化
  int32 last_action_type = 13;
  int32 last_action_cost = 14;

  // Joker 狀態
  int32 joker_count = 15;
  int32 joker_slot_limit = 16;

  // 遊戲結束
  int32 game_end = 17;       // 0=None, 1=Win, 2=Lose
  bool blind_cleared = 18;
}
```

### 設計原則
- **reward 永遠返回 0.0** — 獎勵由 Python 端計算
- 專注於遊戲規則正確性
- 提供足夠的狀態信息供 Python 計算獎勵

---

## Python Environment (`python-env/`)

**職責：獎勵計算、訓練、策略優化**

### 核心模組
- `joker_env/reward.py` — 獎勵計算模組
- `joker_env/env.py` — Gymnasium 環境包裝器
- `training/` — PPO/DQN 訓練腳本

### 獎勵函數 (`reward.py`)

| 函數 | 用途 | 觸發條件 |
|------|------|----------|
| `play_reward()` | 出牌獎勵 | action_type == PLAY |
| `discard_reward()` | 棄牌懲罰 | action_type == DISCARD |
| `blind_clear_reward()` | 過關獎勵 | blind_cleared == true |
| `ante_progress_reward()` | Ante 進度獎勵 | Ante 提升時 |
| `game_end_reward()` | 遊戲結束獎勵 | game_end != 0 |
| `money_reward()` | 金幣管理獎勵 | 利息閾值達成 |
| `joker_buy_reward()` | 購買 Joker 獎勵 | action_type == BUY_JOKER |
| `skip_blind_reward()` | 跳過 Blind 獎勵 | action_type == SKIP_BLIND |
| `reroll_reward()` | Reroll 懲罰 | action_type == REROLL |
| `sell_joker_reward()` | 賣出 Joker 懲罰 | action_type == SELL_JOKER |

### 獎勵設計原則
1. **Ante-aware scaling** — 獎勵隨 Ante 縮放
2. **非線性經濟懲罰** — 過度消費指數懲罰
3. **利息閾值獎勵** — $5/$10/$15/$20/$25 階梯獎勵
4. **階段感知** — 早期/晚期不同策略

### RewardCalculator 使用
```python
from joker_env.reward import RewardCalculator

calculator = RewardCalculator()
calculator.reset()  # 每個 episode 開始時

# 每個 step
reward = calculator.calculate(info_dict)
```

---

## 通訊協議 (`proto/`)

### gRPC 服務
```protobuf
service JokerEnv {
  rpc Reset(ResetRequest) returns (ResetResponse);
  rpc Step(StepRequest) returns (StepResponse);
  rpc GetSpec(GetSpecRequest) returns (GetSpecResponse);
}
```

### Action Types
| ID | 名稱 | 說明 |
|----|------|------|
| 0 | SELECT | 選擇手牌 |
| 1 | PLAY | 出牌 |
| 2 | DISCARD | 棄牌 |
| 3 | SELECT_BLIND | 選擇進入 Blind |
| 4 | CASH_OUT | 結算獎勵 |
| 5 | BUY_JOKER | 購買 Joker |
| 6 | NEXT_ROUND | 下一回合 |
| 7 | REROLL | 重新整理商店 |
| 8 | SELL_JOKER | 賣出 Joker |
| 9 | SKIP_BLIND | 跳過 Blind |
| 10 | USE_CONSUMABLE | 使用消耗品 |
| 11 | BUY_VOUCHER | 購買 Voucher |
| 12 | BUY_PACK | 購買卡包 |

---

## 開發指南

### 修改獎勵函數
1. 編輯 `python-env/src/joker_env/reward.py`
2. 無需重新編譯 Rust
3. 立即生效

### 添加新遊戲狀態
1. 修改 `proto/joker_guide.proto` 的 EnvInfo
2. 更新 `rust-engine/src/main.rs` 填充新字段
3. 重新編譯 Rust 並重新生成 Python proto
4. 在 `reward.py` 中使用新狀態

### 測試
```bash
# Rust 測試
cd rust-engine && cargo test

# Python 測試
cd python-env && pytest
```

---

## 為什麼分離 Reward 計算？

1. **迭代速度** — 調整獎勵無需重新編譯 Rust
2. **實驗靈活性** — 可以快速測試不同獎勵設計
3. **關注點分離** — Rust 專注遊戲規則，Python 專注訓練
4. **可測試性** — 獎勵函數可以獨立單元測試
