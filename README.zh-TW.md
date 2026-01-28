# Joker Guide RL

[English Version](README.md)

Balatro 強化學習訓練專案，採用 **Rust + Python 分離架構**。

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

- **Rust Engine** (`rust-engine/`): 純遊戲環境，負責狀態管理、動作驗證、Action Mask 生成
- **Python Environment** (`python-env/`): 獎勵計算、Gymnasium 包裝、PPO/REINFORCE 訓練

## 核心概念

- **觀測向量 (長度 170)**: `[scalars(8), selection_mask(5), hand(5×17), hand_type(10), deck_counts(52), joker_slots(5×2)]`
- **動作元組**: MultiDiscrete `[action_type, card_0, …, card_4]`，支援 13 種動作類型
- **Joker 系統**: 164 個 Joker，採用聲明式效果定義系統
- **獎勵分離**: Rust 提供 delta 資訊，Python 計算獎勵函數

## Rust Engine 結構

```
rust-engine/src/
├── main.rs              # gRPC 服務主程式
├── lib.rs               # Proto 導入
├── game/                # 遊戲核心邏輯
│   ├── joker_def.rs     # 聲明式 Joker 效果系統 (164 Jokers)
│   ├── joker.rs         # Joker 實現與 Tiered Architecture
│   ├── scoring.rs       # 計分引擎
│   ├── consumables.rs   # 消耗品 (Tarot/Planet/Spectral)
│   ├── vouchers.rs      # Voucher 永久升級
│   ├── packs.rs         # 卡包系統
│   ├── blinds.rs        # Blind/Boss Blind/Ante
│   ├── stakes.rs        # Stake 難度系統
│   ├── tags.rs          # Tag 系統
│   ├── decks.rs         # 起始牌組
│   ├── cards.rs         # Card/Enhancement/Seal/Edition
│   ├── hand_types.rs    # 牌型定義
│   └── constants.rs     # 遊戲常量
└── service/             # gRPC 服務層
    ├── state.rs         # EnvState 狀態管理
    ├── observation.rs   # 觀測向量構建
    ├── action_mask.rs   # 合法動作遮罩
    └── scoring.rs       # 手牌計分分析
```

### 動作類型

| ID | 名稱 | ID | 名稱 |
|----|------|----|------|
| 0 | SELECT (選擇) | 7 | REROLL (重整商店) |
| 1 | PLAY (出牌) | 8 | SELL_JOKER (賣出 Joker) |
| 2 | DISCARD (棄牌) | 9 | SKIP_BLIND (跳過 Blind) |
| 3 | SELECT_BLIND (選擇 Blind) | 10 | USE_CONSUMABLE (使用消耗品) |
| 4 | CASH_OUT (結算) | 11 | BUY_VOUCHER (購買 Voucher) |
| 5 | BUY_JOKER (購買 Joker) | 12 | BUY_PACK (購買卡包) |
| 6 | NEXT_ROUND (下一回合) | | |

## 執行 Rust 伺服器

```bash
cd rust-engine
cargo run --release
```

gRPC 服務監聽 `127.0.0.1:50051`。修改 `proto/joker_guide.proto` 後需重新生成 Python stubs：

```bash
PYTHON_BIN=python3 ./scripts/gen_proto.sh
```

## Python 環境

在 `python-env` 目錄安裝 Python 套件：

```bash
cd python-env
python3 -m pip install .
python3 -m pip install grpcio-tools stable-baselines3 sb3-contrib
```

### 訓練腳本

#### `python -m joker_env.train`

- `--episodes`: REINFORCE 訓練回合數 (預設 50)
- `--checkpoint`: 策略參數儲存路徑 (例如 `python-env/models/simple.pt`)

範例：
```bash
PYTHONPATH=python-env/src python3 -m joker_env.train --episodes 20 --checkpoint python-env/models/simple.pt
```

#### `python -m joker_env.train_sb3`

- `--timesteps`: MaskablePPO 總時間步數 (預設 50000)
- `--checkpoint`: SB3 模型儲存路徑 (例如 `python-env/models/ppo`)
- `--save-interval`: 分段儲存間隔 (預設 25000)，觸發中間快照如 `python-env/models/ppo_25000`
- `--tensorboard-log`: TensorBoard 摘要寫入路徑
- `--log-freq`: 自訂 callback 的 console summary 頻率 (預設 10)
- `--tb-log-freq`: 自訂 metrics 寫入 TensorBoard 的頻率 (預設 1)
- `--verbose`: SB3 輸出詳細度 (預設 1)
- `--mps`: 可用時啟用 Apple MPS 加速
- `--n-steps`: 每次更新的 rollout 步數 (預設 256)
- `--batch-size`: minibatch 大小 (預設 64)
- `--ent-coef`: entropy 係數 (預設 0.1)
- `--learning-rate`: 學習率 (預設 0.0003)
- `--gamma`: 折扣因子 (預設 0.99)
- `--gae-lambda`: GAE lambda (預設 0.95)
- `--clip-range`: PPO clip 範圍 (預設 0.2)
- `--clip-range-vf`: value function clip 範圍 (預設 None)
- `--normalize-advantage` / `--no-normalize-advantage`: 是否正規化 advantage (預設開啟)
- `--n-epochs`: 每次更新的訓練 epoch 數 (預設 10)
- `--vf-coef`: value function 係數 (預設 0.5)
- `--max-grad-norm`: 梯度裁切 (預設 0.5)
- `--target-kl`: 目標 KL，超過可提前停止 (預設 None)
- `--use-sde`: 啟用 generalized state-dependent exploration (預設關閉)
- `--sde-sample-freq`: SDE 取樣頻率 (預設 -1)
- `--stats-window-size`: log 的 rolling 統計視窗 (預設 100)
- `--seed`: 隨機種子 (預設 None)
- `--net-arch`: policy/value MLP 隱藏層大小 (預設 `128 128`)

範例：
```bash
PYTHONPATH=python-env/src python3 -m joker_env.train_sb3 --timesteps 100000 --checkpoint python-env/models/ppo --save-interval 25000 --tensorboard-log python-env/logs/ppo
```

SB3 腳本使用 `ActionMasker` 包裝環境，採用自訂特徵提取器串接 selection、hand、deck 和 Joker embeddings，將學習迴圈分段以便長時間訓練可順利恢復，並在 `--checkpoint` 路徑旁存放分段檢查點。

## 實驗追蹤

- 每個檢查點分段會寫入記錄到 `python-env/experiments/checkpoints.jsonl`（自動建立）。每行是 JSON 格式，包含 `timestamp`、`checkpoint`、`steps`、`total_timesteps` 和 `save_interval`。
- 使用 `tail -n 5 python-env/experiments/checkpoints.jsonl` 查看最新快照。
- 執行 `scripts/checkpoint_report.py` 列印最新日誌條目。
- 將 `scripts/checkpoint_report.py --tail 10` 導入你的儀表板或自動化流程。

## 測試與驗證

```bash
cd rust-engine && cargo test
```

### 測試覆蓋範圍

| 模組 | 測試內容 |
|------|----------|
| `game/cards.rs` | 卡牌 chips/mults 計算、花色規則、deck/index 完整性 |
| `game/hand_types.rs` | 牌型映射、分數合理性檢查 |
| `game/blinds.rs` | Blind/Boss Blind/Ante 規則邏輯 |
| `service/action_mask.rs` | 狀態門控 (state-gating)、合法動作生成 |
| `service/scoring.rs` | Straight baseline、Flint halving、Plasma scoring、Observatory bonus、Selection fallback |

### 其他測試

- **Scoring regression**: 對照 `references/RLatro/` 的 FullHouse scoring + Joker bonuses
- **Proptest suite**: 隨機手牌/Joker 組合的 chips/mults 正值檢驗、Joker multipliers ≥1
- **Edition coverage**: Steel-enhanced FullHouse、Holo+Poly edition、rare Joker combos (`Xm`, `++`, `+$`)

修改 scoring、Joker、或 proto 後請重新執行 `cargo test`。

## 端對端執行

1. 啟動 Rust 伺服器：`cargo run --release`
2. 在另一個終端執行上述訓練指令
3. 完成後停止 Rust 伺服器 (`Ctrl+C`)

## 後續步驟

- **整合測試**: 添加完整 blinds/shops 流程測試，包含 Joker + Voucher 組合效果
- **Golden-score fixtures**: 建立固定預期分數的測試案例，防止重構時意外改變計分行為
- **獎勵函數調優**: 在 `python-env/src/joker_env/reward.py` 中實驗不同獎勵設計
- **長時間訓練**: 使用 `--tensorboard-log` 追蹤訓練進度，配合 `python-env/experiments/` 做實驗管理
- **參考實現**: 對照 [references/RLatro](references/RLatro/) 驗證規則正確性
