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
- **Python Environment** (`python-env/`): 獎勵計算、Gymnasium 包裝、MaskablePPO 訓練

## 核心成果

| 指標 | 數值 |
|------|------|
| **最佳 avg_ante** | 8.42（E4 實驗） |
| **基線 → 最佳** | 3.57 → 8.42（+136%） |
| **訓練 FPS** | 18,184（初始 665 的 27.3 倍） |
| **核心發現** | 獎勵簡化 > 獎勵工程 |

### 實驗歷程

| 實驗 | 修改內容 | avg_ante | 變化 |
|------|---------|----------|------|
| E1 | 基線（reward v5.0） | 3.57 | — |
| E2 | 凍結主觀獎勵 | 4.72 | +32% |
| E3 | 修復 sell_joker 不對稱性 | 5.55 | +55% |
| E4 | 限制 game_end_reward ±2.0 | 8.42 | +136% |

**關鍵發現**：VecNormalize scale dominance — 過大的終端獎勵（±5.0）將所有中間信號壓縮為雜訊。將其限制在 ±2.0 後，所有獎勵通道恢復正常學習。

## 快速開始

**一鍵安裝**（推薦）：
```bash
chmod +x scripts/setup.sh
./scripts/setup.sh
```

或手動安裝：
```bash
# 1. 編譯 Rust 引擎
cd rust-engine && cargo build --release && cd ..

# 2. 安裝 Python 依賴
cd python-env && pip install . && cd ..

# 3. 開始訓練
./train.sh 4 --timesteps 100000 --checkpoint python-env/models/my_model
```

詳細安裝說明、GPU 設置和疑難排解請參閱 [INSTALL.md](INSTALL.md)。

## 架構概覽

### 觀測向量（1,556 維）

| 區塊 | 維度 | 說明 |
|------|------|------|
| Scalars | 32 | 遊戲狀態（分數、Ante、階段、金幣等） |
| Selection | 8 | 手牌選擇遮罩 |
| Hand | 168 | 8 張牌 × 21 特徵（點數、花色、增強、封印、版本） |
| Hand Type | 13 | 牌型 one-hot 編碼 |
| Deck | 52 | 剩餘牌組計數 |
| Jokers | 765 | 5 槽位 × 153 特徵（150 ID one-hot + 3 旗標） |
| Shop | 302 | 2 商店 Joker × 151 特徵（150 ID one-hot + 成本） |
| Boss Blind | 27 | Boss Blind 類型 one-hot |
| Deck Type | 16 | 起始牌組類型 |
| Stake | 8 | 難度等級 |
| Vouchers | 36 | 已擁有 Voucher 旗標 |
| Consumables | 104 | 2 槽位 × 52 類型 |
| Tags | 25 | Tag 計數 |

### 動作空間

MultiDiscrete 動作空間，**46 維動作遮罩**：

| ID | 動作 | ID | 動作 |
|----|------|----|------|
| 0 | SELECT（選擇） | 7 | REROLL（重整商店） |
| 1 | PLAY（出牌） | 8 | SELL_JOKER（賣出 Joker） |
| 2 | DISCARD（棄牌） | 9 | SKIP_BLIND（跳過 Blind） |
| 3 | SELECT_BLIND（選擇 Blind） | 10 | USE_CONSUMABLE（使用消耗品） |
| 4 | CASH_OUT（結算） | 11 | BUY_VOUCHER（購買 Voucher） |
| 5 | BUY_JOKER（購買 Joker） | 12 | BUY_PACK（購買卡包） |
| 6 | NEXT_ROUND（下一回合） | | |

### 核心設計原則

- **164 個 Joker**，採用聲明式效果定義系統（`joker_def.rs`）
- **獎勵分離**：Rust 提供 delta 資訊，Python 計算獎勵（無需重新編譯）
- **多 Session 支援**：單一 Rust 引擎透過 gRPC session ID 服務多個 Python 環境

## 並發訓練（推薦）

使用 `train.sh` 進行自動化並發訓練：

```bash
# 120 個批次環境，1000 萬步（高效能模式）
./train.sh 120 --timesteps 10000000 --batch-env --batch-size 512 --n-steps 512

# 4 個並行環境，100 萬步（輕量模式）
./train.sh 4 --timesteps 1000000 --checkpoint python-env/models/v1

# 關閉 TensorBoard
./train.sh 4 --timesteps 1000000 --no-tensorboard
```

腳本自動處理：
1. 啟動 Rust 引擎
2. 等待 gRPC 服務就緒
3. 建立時間戳 log 目錄（`python-env/logs/run_YYYYMMDD_HHMMSS`）
4. 啟動 TensorBoard（http://localhost:6006）
5. 啟動並行 Python 訓練
6. Ctrl+C 時優雅關閉所有進程

## 手動訓練

### 啟動 Rust 伺服器

```bash
cd rust-engine && cargo run --release
```

gRPC 服務監聽 `127.0.0.1:50051`。

### 使用 MaskablePPO 訓練（推薦）

```bash
PYTHONPATH=python-env/src python -m joker_env.train_sb3 \
  --timesteps 100000 \
  --checkpoint python-env/models/ppo \
  --tensorboard-log python-env/logs/ppo
```

常用參數（v10.1）：

| 參數 | 預設值 | 說明 |
|------|--------|------|
| `--timesteps` | 50000 | 總訓練步數 |
| `--checkpoint` | - | 模型儲存路徑 |
| `--save-interval` | 25000 | 檢查點間隔 |
| `--n-envs` | 1 | 並行環境數 |
| `--n-steps` | 512 | 每環境 rollout 長度 |
| `--batch-size` | 256 | Minibatch 大小 |
| `--learning-rate` | 3e-4 | 學習率 |
| `--ent-coef` | 0.06 | Entropy 係數 |
| `--gamma` | 0.95 | 折扣因子 |
| `--gae-lambda` | 0.92 | GAE lambda |
| `--n-epochs` | 3 | 每 rollout PPO 更新次數 |
| `--clip-range-vf` | 0.2 | Value function 裁剪範圍 |
| `--target-kl` | 0.02 | 提前停止 KL 閾值 |
| `--net-arch` | 128 128 | MLP 隱藏層 |
| `--batch-env` | 關閉 | 使用 JokerBatchVecEnv 高效能模式 |

完整參數列表：`python -m joker_env.train_sb3 --help`

## 獎勵系統（v10.0）

獎勵計算在 Python 端（`python-env/src/joker_env/reward.py`）：

| 事件 | 範圍 | 說明 |
|------|------|------|
| 遊戲勝利 | +5.0 | 終端目標，最高獎勵 |
| 遊戲失敗 | -2.0 ~ -0.5 | 依進度調整懲罰 |
| Ante 進度 | +0.48 ~ +2.27 | 漸進式增長（0.15×a^1.5） |
| 過關 | +0.25 ~ +0.75 | Ante 調整加成 |
| Boss 過關 | +0.0 ~ +0.15 | 依 Boss 難度加成（v7.0） |
| 出牌 | +0.02 ~ +0.17 | 基礎獎勵 + 正規化分數獎勵 |
| 棄牌 | -0.05 ~ -0.02 | 加重成本，防止棄牌循環 |
| 購買 Joker | -0.3 ~ +0.3 | 階段權重 + 經濟懲罰 |
| 跳過 Blind | -0.2 ~ +0.25 | 狀態感知 + 動態機會成本（v10.0） |
| 重整商店 | 動態 | 商店品質感知的 Reroll 預算（v10.0） |
| Joker 協同 | +0.0 ~ +0.12 | 協同群組 + Build 對齊（v7.0） |
| 分數效率 | +0.0 ~ +0.06 | 超額分數獎勵（v6.9） |

核心特性：
- **終端獎勵主導**：勝利獎勵（5.0）壓過所有中間獎勵累積
- **Reward Hacking 防護**：空棄牌（-0.05）、購買失敗（-0.05）、no-op（-0.03）
- **商店品質評分**：稀有度（40%）、協同（30%）、成本效益（20%）、特殊加成（10%）
- **Reroll 預算追蹤**：前 2 次正常，第 3 次起遞減獎勵
- **Joker 貢獻追蹤**：xMult 權重最高（0.5）、chips 最低（0.2）

## FPS 優化歷程

| 階段 | FPS | 加速比 | 技術 |
|------|-----|--------|------|
| 初始 | 665 | 1.0x | 基礎 gRPC |
| Proto 零拷貝 | — | — | raw_data 零拷貝反序列化 |
| 向量化觀測 | — | — | 批次觀測分割 |
| torch.compile | — | — | JIT 編譯策略網路 |
| f64→f32 修復 | — | — | Action mask dtype 修正 |
| **最終** | **18,184** | **27.3x** | 所有優化結合 |

## 專案結構

```
joker-guide/
├── rust-engine/src/
│   ├── main.rs              # gRPC 服務入口
│   ├── game/                # 遊戲核心邏輯
│   │   ├── joker_def.rs     # 聲明式 Joker 效果（164 個）
│   │   ├── joker.rs         # Joker 實現
│   │   ├── scoring.rs       # 計分引擎
│   │   ├── blinds.rs        # Blind/Boss/Ante
│   │   ├── cards.rs         # Card/Enhancement/Seal/Edition
│   │   └── ...              # 其他遊戲系統
│   └── service/             # gRPC 服務層
│       ├── state.rs         # 遊戲狀態管理
│       ├── observation.rs   # 觀測向量構建
│       └── action_mask.rs   # 合法動作生成
├── python-env/src/joker_env/
│   ├── env.py               # Gymnasium 環境包裝
│   ├── reward.py            # 獎勵計算（v10.0）
│   ├── callbacks.py         # 訓練 Callbacks
│   ├── train_sb3.py         # MaskablePPO 訓練
│   ├── batch_vec_env.py     # 高效能向量化環境
│   └── train.py             # 基礎 REINFORCE 訓練
├── proto/
│   └── joker_guide.proto    # gRPC 協議定義
├── data/                    # 遊戲資料（JSON 參考檔）
├── experiments/             # 實驗記錄與研究筆記
└── train.sh                 # 並發訓練腳本
```

## 測試

```bash
# Rust 測試（195 個）
cd rust-engine && cargo test

# Python 獎勵測試
cd python-env && pytest tests/
```

### 測試覆蓋

| 模組 | 內容 |
|------|------|
| `game/joker.rs` | Joker 效果計算、狀態累積 |
| `game/scoring.rs` | 計分引擎、牌型識別 |
| `service/action_mask.rs` | 狀態門控、合法動作生成 |
| `reward.py` | 70 個獎勵函數單元測試 |

## 實驗追蹤

- **TensorBoard**：`train.sh` 自動啟動，訪問 http://localhost:6006
- **Log 目錄**：`python-env/logs/run_YYYYMMDD_HHMMSS/`（時間戳命名）
- **檢查點**：儲存至 `python-env/experiments/checkpoints.jsonl`
- **報告腳本**：`python scripts/checkpoint_report.py --tail 10`
- **實驗記錄**：`experiments/autoresearch_log.md`、`experiments/daily_log_*.md`

## Proto 重新生成

修改 `proto/joker_guide.proto` 後：

```bash
./scripts/gen_proto.sh
```

## 系統需求

- Rust 1.70+
- Python 3.10+
- 依賴套件：`gymnasium`、`torch`、`stable-baselines3`、`sb3-contrib`、`grpcio`
