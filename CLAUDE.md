# Joker Guide - Balatro RL Training Project

## Quick Reference

```bash
# 啟動訓練
./train.sh 4 --timesteps 100000

# Rust 測試
cd rust-engine && cargo test

# Python 測試
cd python-env && pytest

# 重新生成 proto
cd python-env && python -m grpc_tools.protoc -I../proto --python_out=src/joker_env/proto --grpc_python_out=src/joker_env/proto ../proto/joker_guide.proto
```

---

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

## Claude 開發指南

### 代碼風格
- **Rust**: 遵循 `cargo fmt` 格式
- **Python**: 遵循 PEP 8，使用 black formatter
- 中文註釋用於業務邏輯，英文用於 API 文件

### Commit 規範
- `feat`: 新功能
- `fix`: 修復錯誤
- `docs`: 文件更新
- `refactor`: 重構

### 測試要求
- Rust 變更需通過 `cargo test`
- 獎勵函數變更需通過 `pytest tests/`

### 禁止操作
- 不可直接修改 `.proto` 而不更新 Rust/Python 兩端
- 不可移除 EnvInfo 欄位（會破壞訓練兼容性）
- 不可修改 `reward.py` 中的版本號而不更新相應邏輯

---

## Rust Engine (`rust-engine/`)

**職責：純遊戲環境，不計算獎勵**

### 目錄結構
```
rust-engine/
├── Cargo.toml          # 依賴：tokio, tonic, prost, rand
├── build.rs            # tonic-build for proto compilation
├── src/
│   ├── lib.rs          # Proto module inclusion
│   ├── main.rs         # gRPC 服務實現 (EnvService)
│   ├── game/           # 核心遊戲邏輯 (13 模組)
│   │   ├── constants.rs    # OBS_SIZE=1130, ACTION_MASK_SIZE=46
│   │   ├── cards.rs        # Card, Enhancement, Seal, Edition
│   │   ├── blinds.rs       # Stage, BlindType, BossBlind (27 types)
│   │   ├── hand_types.rs   # HandId, HandScore
│   │   ├── scoring.rs      # JokerRules, score_hand_with_rules
│   │   ├── joker.rs        # JokerId (164 jokers), JokerSlot
│   │   ├── joker_def.rs    # 聲明式效果系統
│   │   ├── shop.rs         # Shop, ShopItem
│   │   ├── tags.rs         # Tag, TagId (25 types)
│   │   ├── decks.rs        # DeckType (16 types)
│   │   ├── stakes.rs       # Stake (8 difficulty levels)
│   │   ├── vouchers.rs     # VoucherId (36 vouchers)
│   │   ├── consumables.rs  # Tarot/Planet/Spectral
│   │   └── packs.rs        # PackType, PackContents
│   └── service/        # gRPC 服務層 (4 模組)
│       ├── state.rs        # EnvState (game state container)
│       ├── observation.rs  # Tensor 建構
│       ├── action_mask.rs  # 合法動作計算
│       └── scoring.rs      # 出牌評分計算
```

### 核心功能
- 遊戲狀態管理（牌組、手牌、Joker、商店）
- 動作驗證與執行
- Action Mask 生成（合法動作遮罩）
- gRPC 服務端（Session-based 狀態管理）

### 返回數據（EnvInfo）
```protobuf
message EnvInfo {
  // 基本狀態
  int32 episode_step = 1;
  int64 chips = 2;           // 當前分數
  int64 mult = 3;            // 預留給 mult 顯示
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

  // 動作細節（v5.0 新增）
  int32 cards_played = 19;      // 這次出牌的卡片數量
  int32 cards_discarded = 20;   // 這次棄牌的卡片數量
  int32 hand_type = 21;         // 打出的牌型 ID (-1 = 無)

  // Skip Blind 相關
  int32 tag_id = 22;            // 跳過 Blind 獲得的 Tag ID (-1 = 無)

  // 消耗品相關
  int32 consumable_id = 23;     // 使用的消耗品 ID (-1 = 無)

  // Joker 交易相關
  int32 joker_sold_id = 24;     // 賣出的 Joker ID (-1 = 無)
  int32 best_shop_joker_cost = 25;  // 商店中最強 Joker 的成本 (0 = 無)

  // v6.4: 手牌潛力指標
  float flush_potential = 26;      // 同花潛力 [0, 1]
  float straight_potential = 27;   // 順子潛力 [0, 1]
  float pairs_potential = 28;      // 對子潛力 [0, 1]

  // v6.9: Joker 貢獻追蹤
  float joker_chip_contrib = 29;   // Joker chips 貢獻比例 [0, 1]
  float joker_mult_contrib = 30;   // Joker mult 貢獻比例 [0, 1]
  float joker_xmult_contrib = 31;  // Joker x_mult 正規化值 [0, 1]
  float score_efficiency = 32;     // 分數效率

  // v7.0: Boss Blind 識別
  int32 boss_blind_id = 33;        // Boss Blind ID (0-26), -1 = 無 Boss
}
```

### 設計原則
- **reward 永遠返回 0.0** — 獎勵由 Python 端計算
- 專注於遊戲規則正確性
- 提供足夠的狀態信息供 Python 計算獎勵

---

## Python Environment (`python-env/`)

**職責：獎勵計算、訓練、策略優化**

### 目錄結構
```
python-env/
├── pyproject.toml          # 依賴：gymnasium, torch, sb3-contrib, grpcio
├── src/joker_env/
│   ├── __init__.py         # 導出 JokerGymEnv, JokerGymDictEnv
│   ├── client.py           # gRPC 客戶端（JokerEnvClient）
│   ├── env.py              # Gymnasium 環境包裝器
│   ├── reward.py           # 獎勵計算模組 (v6.0)
│   ├── train_sb3.py        # MaskablePPO 訓練腳本
│   ├── callbacks.py        # SB3 callbacks（Metrics, Entropy Schedule）
│   └── proto/              # gRPC 生成代碼
├── tests/
│   └── test_reward.py      # 獎勵函數測試
└── models/                 # 模型檢查點
```

### 獎勵函數 (`reward.py` v7.0)

| 函數 | 用途 | 觸發條件 | 獎勵範圍 |
|------|------|----------|----------|
| `play_reward()` | 出牌獎勵 | action_type == PLAY | +0.02 ~ +0.25 |
| `hand_type_bonus()` | 牌型品質獎勵 | 出牌時 | -0.01 ~ +0.12 |
| `discard_reward()` | 棄牌懲罰 | action_type == DISCARD | -0.05 ~ -0.02 |
| `blind_clear_reward()` | 過關獎勵 | blind_cleared == true | +0.25 ~ +1.05 |
| `boss_clear_difficulty_bonus()` | Boss 難度獎勵 (v7.0) | Boss 過關 | 0.0 ~ +0.15 |
| `ante_progress_reward()` | Ante 進度獎勵 | Ante 提升時 | +0.56 ~ +2.56 |
| `game_end_reward()` | 遊戲結束獎勵 | game_end != 0 | -2.0 ~ +5.0 |
| `money_reward()` | 利息閾值獎勵 | CASH_OUT | 0.0 ~ +0.2 |
| `joker_holding_bonus()` | Joker 持有獎勵 | CASH_OUT | -0.08 ~ +0.08 |
| `joker_synergy_reward()` | Joker 協同獎勵 (v7.0) | CASH_OUT | 0.0 ~ +0.12 |
| `joker_shortage_penalty()` | Joker 不足懲罰 | 商店階段 | -0.03 ~ 0.0 |
| `joker_buy_reward()` | 購買 Joker | BUY_JOKER 成功 | -0.3 ~ +0.5 |
| `sell_joker_reward()` | 賣出 Joker | action_type == SELL_JOKER | **-0.5 ~ +0.1** |
| `skip_blind_reward_v2()` | 狀態感知 Skip (v10.0) | action_type == SKIP_BLIND | -0.25 ~ +0.30 |
| `reroll_reward_v2()` | 商店品質感知 Reroll (v10.0) | action_type == REROLL | -0.15 ~ +0.05 |
| `consumable_use_reward()` | 使用消耗品 | action_type == USE_CONSUMABLE | 0.0 ~ +0.25 |
| `voucher_buy_reward()` | 購買 Voucher | BUY_VOUCHER 成功 | -0.25 ~ +0.3 |
| `hand_type_targeting_reward()` | 牌型針對性 (v7.0) | PLAY（含匹配 Joker）| 0.0 ~ +0.10 |

### 獎勵設計原則
1. **終端獎勵主導** — Win=+5.0, Lose=-2.0~-0.5（依 Ante 進度縮放）
2. **Joker 保護機制** — 低數量時嚴禁賣出 (-0.3~-0.5)，持有給予獎勵 (v6.0)
3. **牌型品質獎勵** — Flush/Straight +0.03, Full House +0.04, Four Kind +0.06 (v6.0)
4. **早期購買加倍** — 首 2 個 Joker 價值提升 2.5x (v6.0)
5. **非線性經濟懲罰** — 使用 `log1p` 平滑懲罰
6. **利息閾值獎勵** — $5/$10/$15/$20/$25 階梯獎勵
7. **反作弊機制** — No-op 偵測 (-0.03)、空棄牌懲罰 (-0.05)
8. **Boss 難度獎勵** — 困難 Boss (Wall, Needle, Violet) 過關額外 +0.10~0.15 (v7.0)
9. **Joker 協同獎勵** — 持有協同群組 Joker 獲得額外獎勵 (v7.0)
10. **商店品質感知 Reroll** — 早期低品質商店鼓勵 Reroll，高品質商店懲罰 (v10.0)
11. **狀態感知 Skip** — 動態機會成本考慮商店品質和 Joker 數量 (v10.0)

### Tag 價值系統

Skip Blind 決策的關鍵，25 種 Tag 的價值映射：

| Tag 類型 | 價值 | 說明 |
|----------|------|------|
| TAG_NEGATIVE | 0.52 | 最高（額外 Joker 槽位）|
| TAG_RARE | 0.40 | 免費 Rare Joker |
| TAG_POLYCHROME | 0.35 | ×1.5 mult |
| TAG_VOUCHER | 0.30 | 免費 Voucher |
| ... | ... | ... |
| TAG_SPEED | 0.06 | 最低（跳過商店風險）|

**平均 Tag 價值 ≈ 0.20**

### BuildTracker（Playstyle 追蹤）

追蹤玩家風格以優化 Joker 購買決策：
- 追蹤 pairs / straight / flush 三種 build
- 當某 build 超過 60% 且樣本 >= 5 時，識別為 dominant build
- 購買匹配 build 的 Joker 獲得 +0.02~0.03 獎勵

### v7.0 Boss 難度系統

Boss Blind 難度係數（用於 `boss_clear_difficulty_bonus()`）：

| Boss 類型 | 難度 | 說明 |
|-----------|------|------|
| BOSS_HOOK (0) | 0.8 | Easy - 可預測的棄牌 |
| BOSS_WALL (1) | 1.4 | Very Hard - 需要 4x 分數 |
| BOSS_NEEDLE (20) | 1.4 | Very Hard - 只有 1 次出牌機會 |
| BOSS_VIOLET (22) | 1.5 | Showdown - 需要 6x 分數 |
| BOSS_FLINT (4) | 1.3 | Hard - 基礎分數減半 |

難度 > 0.8 的 Boss 過關會獲得額外獎勵：`(difficulty - 0.8) × 0.2 × efficiency_mult`

### v7.0 Joker 協同群組

定義有協同效果的 Joker 組合：

| 群組名稱 | Joker IDs | 說明 |
|----------|-----------|------|
| pair_power | 5, 10, 111, 6, 11, 112, 113 | 強化 Pair/Set 的 Joker |
| straight_masters | 8, 13, 114, 29, 131 | 強化 Straight 的 Joker |
| flush_kings | 9, 14, 115, 30 | 強化 Flush 的 Joker |
| scaling_xmult | 97, 120, 129, 23, 64 | 累積乘法 Joker |
| boss_killer | 68, 118 | 對抗 Boss 的 Joker (Chicot, Matador) |

持有同群組 >= 2 個 Joker 時，每個額外匹配 +0.02 獎勵（在 CASH_OUT 時評估）。

### RewardCalculator 使用
```python
from joker_env.reward import RewardCalculator

calculator = RewardCalculator()
calculator.reset()  # 每個 episode 開始時

# 每個 step
reward = calculator.calculate(info_dict)
```

---

## 強化學習設計

### 觀測空間（~1,651 維度）

| 組件 | 維度 | 編碼 | 說明 |
|------|------|------|------|
| `scalars` | 32 | 正規化浮點 | 分數進度、ante、stage、plays/discards、money 等 |
| `selection_mask` | 8 | Binary | 當前選中的手牌 |
| `hand` | 168 (8×21) | One-hot | 8 張牌：13 rank + 4 suit + enhancement/seal/edition |
| `hand_type` | 13 | One-hot | 當前最佳牌型 |
| `deck` | 52 | Counts | 牌組剩餘卡牌 |
| `jokers` | 765 (5×153) | One-hot | 5 槽位：150 ID one-hot + enabled/eternal/negative |
| `shop` | 302 (2×151) | One-hot + cost | 商店 Joker |
| `boss_blind` | 27 | One-hot | 當前 Boss Blind |
| `deck_type` | 16 | One-hot | 起始牌組變體 |
| `stake` | 8 | One-hot | 難度等級 |
| `vouchers` | 36 | Binary | 已擁有 Voucher |
| `consumables` | 104 (2×52) | One-hot | 消耗品槽位 |
| `tags` | 25 | Counts | Tag 庫存 |

**特徵提取器** (`JokerFeaturesExtractor`)：將 150 維 Joker ID one-hot 壓縮為 32 維 learned embeddings。

### 動作空間（MultiDiscrete）

```python
ACTION_PARAM_SIZES = [
    13,           # action_type
    2, 2, 2, 2, 2, 2, 2, 2,  # card selection (8 binary)
    3,            # blind selection
    2,            # shop joker purchase slot
    5,            # sell joker slot
    1,            # reroll
    1,            # skip blind
    2,            # use consumable slot
    1,            # buy voucher
    2,            # buy pack slot
]
```

### Action Mask（46 維度）

| 偏移 | 大小 | 組件 | 閘門邏輯 |
|------|------|------|----------|
| 0-12 | 13 | Action types | Stage-aware |
| 13-28 | 16 | Card selection | `in_blind` only |
| 29-31 | 3 | Blind selection | `in_pre_blind` + 順序 |
| 32-33 | 2 | Buy Joker | `in_shop AND money >= cost AND slots` |
| 34-38 | 5 | Sell Joker | `in_shop AND joker exists AND NOT eternal` |
| 39 | 1 | Reroll | `in_shop AND money >= cost` |
| 40 | 1 | Skip Blind | `in_pre_blind AND NOT Boss` |
| 41-42 | 2 | Use Consumable | 消耗品存在且非 Amber Boss |
| 43 | 1 | Buy Voucher | `in_shop AND money >= cost` |
| 44-45 | 2 | Buy Pack | `in_shop AND money >= cost` |

**重要**：PLAY/DISCARD 需要 `selected_mask > 0`（必須先用 SELECT 選卡）。

### 訓練配置（PPO）

| 參數 | 值 | 說明 |
|------|------|------|
| `gamma` | 0.95 | ~200 step episodes 的適中 horizon |
| `gae_lambda` | 0.92 | Advantage 估計的 bias-variance 平衡 |
| `ent_coef` | 0.05 → 0.005 | 線性衰減（EntropyScheduleCallback）|
| `learning_rate` | 3e-4 | 標準 PPO 學習率 |
| `n_steps` | 256 | Rollout buffer 大小 |
| `batch_size` | 64 | Mini-batch for SGD |
| `target_kl` | 0.015 | Early stopping 閾值 |
| `VecNormalize` | reward only | 獎勵標準化至 ~N(0,1) |

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

## Joker 系統架構 (`rust-engine/src/game/joker_def.rs`)

**重構後的聲明式 Joker 效果系統**

### 核心概念

```
JokerDef = 元數據 + 效果定義 + 初始狀態 + 觸發器

效果定義 = 基礎效果類型 × 觸發條件 × 作用目標
```

### JokerState（統一狀態系統）

取代原有 30+ 個專屬狀態欄位，精簡為 4 種通用狀態：

| 狀態類型 | 用途 | 使用的 Joker |
|----------|------|--------------|
| `None` | 無狀態 | 大多數 Joker |
| `Accumulator { chips, mult, x_mult }` | 累積加成 | Vampire, Hologram, Constellation, Campfire, etc. |
| `Counter { current, threshold, bonus_mult }` | 計數觸發 | Yorick, Obelisk, Selzer, LoyaltyCard, etc. |
| `Target { suit, rank, value }` | 目標追蹤 | AncientJoker, Castle, TheIdol, ToDoList |

### EffectDef（效果類型）

| 類型 | 說明 | 範例 |
|------|------|------|
| `Fixed` | 固定加成 | Joker (+4 Mult), Stuntman (+250 Chips) |
| `Conditional` | 條件觸發 | JollyJoker (+8 Mult on Pair) |
| `CountBonus` | 計數加成 | GreedyJoker (+$3 per Diamond) |
| `PerCard` | 每張牌加成 | Fibonacci (+8 Mult for A/2/3/5/8) |
| `Stateful` | 狀態相關 | AbstractJoker (+3 Mult per Joker) |
| `RuleModifier` | 規則修改 | FourFingers, Shortcut, Smeared |
| `Retrigger` | 重複觸發 | Dusk, Hack, Sock and Buskin |
| `PowerMultiply` | 乘法加成 | Cavendish, Card Sharp |

### GameEvent（事件觸發系統）

| 事件 | 觸發時機 |
|------|----------|
| `BlindSelected` | 選擇進入 Blind |
| `BlindSkipped` | 跳過 Blind |
| `HandPlayed` | 出牌後 |
| `CardDiscarded` | 棄牌後 |
| `PlanetUsed` | 使用 Planet 卡 |
| `RoundEnded` | 回合結束 |
| `JokerSold` | 賣出 Joker |
| `TarotUsed` | 使用 Tarot 卡 |
| `BlindCleared` | 過關後 |

### 添加新 Joker

只需在 `JOKER_DEFINITIONS` 添加一個條目：

```rust
JokerDef {
    id: JokerId::NewJoker,
    cost: 6,
    rarity: Rarity::Uncommon,
    effect: EffectDef::CountBonus {
        filter: CardFilter::Suit(DIAMOND),
        per_card: BonusDef::Money(3),
    },
    initial_state: JokerState::None,
    triggers: &[],
}
```

### 關鍵 API

| 函數 | 用途 |
|------|------|
| `compute_joker_effect_v2()` | 計算 Joker 效果（評分時）|
| `trigger_joker_slot_events()` | 處理事件觸發（非評分時）|
| `get_joker_def()` | 獲取 Joker 定義 |

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

## 常見問題排解

| 錯誤 | 原因 | 解決方案 |
|------|------|----------|
| `protoc not found` | 缺少 protobuf 編譯器 | `apt install protobuf-compiler` |
| `ModuleNotFoundError: grpc_tools` | Python 缺少 gRPC 工具 | `pip install grpcio-tools` |
| `address already in use` | 舊 server 未關閉 | `pkill -f rust-engine` |
| ActionMasker import 失敗 | sb3_contrib 版本 | 使用 `sb3_contrib.common.wrappers` |
| `CUDA out of memory` | GPU 記憶體不足 | 減少 `n_envs` 或使用 CPU |

---

## 為什麼分離 Reward 計算？

1. **迭代速度** — 調整獎勵無需重新編譯 Rust
2. **實驗靈活性** — 可以快速測試不同獎勵設計
3. **關注點分離** — Rust 專注遊戲規則，Python 專注訓練
4. **可測試性** — 獎勵函數可以獨立單元測試
