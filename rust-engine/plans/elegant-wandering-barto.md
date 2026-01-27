# Balatro Rust Engine 完整實作計劃

## 執行摘要

**目標**: 將 Balatro Rust 引擎從 48% 完成度提升至 95%+
**預估工期**: 12-16 天 (~108 小時)
**優先級**: 訓練影響 > 遊戲保真度 > 代碼架構

---

## 當前狀態

| 系統 | 完成度 | 缺失項目 |
|------|--------|----------|
| Joker 效果 | 47% (71/150) | 79 個 Joker 效果未實作 |
| Boss Blind 效果 | 22% (6/27) | 21 個 Boss 效果未實作 |
| 消耗品系統 | 0% (0/52) | 全部未實作 |
| 牌型等級系統 | 10% | 只有追蹤，無升級邏輯 |
| 卡包系統 | 20% | 購買可用，開啟未實作 |
| Tag 效果 | 14% (3/22) | 19 個 Tag 效果未實作 |

---

## 實作階段

### Phase 1: 基礎系統 (Day 1-2, ~9h)

**目標**: 建立消耗品系統的基礎依賴

#### 1.1 HandLevels 升級系統
- **檔案**: `src/game/consumables.rs`
- **任務**:
  - [ ] 擴展 `HandLevels` 結構，加入 `upgrade(hand_type, chips_bonus, mult_bonus)`
  - [ ] 在 `score_hand()` 中應用等級加成
  - [ ] 更新 observation 包含所有 13 個牌型等級

```rust
// consumables.rs 擴展
impl HandLevels {
    pub fn upgrade(&mut self, hand_type: usize, chips: i32, mult: i32) {
        if hand_type < self.levels.len() {
            self.levels[hand_type] += 1;
            self.chips_bonus[hand_type] += chips;
            self.mult_bonus[hand_type] += mult;
        }
    }
}
```

#### 1.2 Quick Wins - Boss Blind 效果
- **檔案**: `src/service/scoring.rs`, `src/game/blinds.rs`
- **任務**:
  - [ ] TheWall: 4x 分數要求 (已有 score_multiplier)
  - [ ] TheFlint: 減半 chips/mult (已有部分實作)
  - [ ] TheClub/Diamond/Heart/Spade: 花色禁用 (啟用 disables_suit 檢查)
  - [ ] ThePlant: 人頭牌禁用 (啟用 disables_face_cards 檢查)

---

### Phase 2: 消耗品核心 (Day 3-6, ~31h)

**目標**: 實作完整消耗品系統

#### 2.1 消耗品類型定義
- **檔案**: `src/game/consumables.rs` (重寫)
- **任務**:
  - [ ] 定義 `ConsumableType` enum (Tarot/Planet/Spectral)
  - [ ] 定義 `TarotId` enum (22 張)
  - [ ] 定義 `PlanetId` enum (12 張)
  - [ ] 定義 `SpectralId` enum (18 張)
  - [ ] 實作 `Consumable` 結構

```rust
pub enum ConsumableType {
    Tarot(TarotId),
    Planet(PlanetId),
    Spectral(SpectralId),
}

pub struct Consumable {
    pub id: ConsumableType,
}
```

#### 2.2 Planet 卡效果 (12 張) - 優先級最高
| Planet | 牌型 | Chips | Mult |
|--------|------|-------|------|
| Pluto | High Card | +10 | +1 |
| Mercury | Pair | +15 | +1 |
| Uranus | Two Pair | +20 | +1 |
| Venus | Three of a Kind | +20 | +2 |
| Saturn | Straight | +30 | +3 |
| Jupiter | Flush | +15 | +2 |
| Earth | Full House | +25 | +2 |
| Mars | Four of a Kind | +30 | +3 |
| Neptune | Straight Flush | +40 | +4 |

#### 2.3 Tarot 卡效果 (22 張)
- **分類實作**:
  - 金幣類: The Hermit (錢翻倍), Temperance (Joker 售價)
  - 創造類: The Emperor (生成 Tarot), High Priestess (生成 Planet)
  - 花色轉換: Star/Moon/Sun/World
  - 強化類: Magician/Empress/Hierophant/Lovers/Chariot/Justice/Devil/Tower
  - 牌型操作: Strength (+1 點數), Hanged Man (銷毀), Death (轉換)

#### 2.4 Spectral 卡效果 (18 張)
- **優先實作**:
  - Black Hole (所有牌型 +1 級)
  - Immolate (銷毀 5 牌，得 $20)
  - Wraith (創造稀有 Joker，金幣歸零)
  - The Soul (創造傳奇 Joker)

#### 2.5 Action 處理更新
- **檔案**: `src/main.rs`
- **任務**:
  - [ ] 實作 `ACTION_TYPE_USE_CONSUMABLE` 效果分發
  - [ ] 更新 observation 消耗品編碼

---

### Phase 3: Joker 狀態系統 (Day 7-9, ~19h)

**目標**: 支援有狀態的 Joker

#### 3.1 JokerState 結構
- **檔案**: `src/game/joker.rs`
- **任務**:
  - [ ] 在 `JokerSlot` 加入 `state: JokerState`
  - [ ] 定義計數器類型 (counter, degradation, trigger_count)

```rust
pub struct JokerState {
    pub counter: i32,      // 通用計數器
    pub x_mult_mod: f32,   // X Mult 修正
    pub chips_mod: i32,    // Chips 修正
}
```

#### 3.2 計數器 Joker (優先)
| Joker | 狀態類型 | 效果 |
|-------|----------|------|
| GreenJoker | counter | +1 Mult/手 (每輪重置) |
| RideTheBus | counter | +1 Mult/連續非人頭手 |
| IceCream | counter | +100 Chips, -5/手 |
| Popcorn | counter | +20 Mult, -4/輪 |
| Wee | counter | +8 Chips/輪 |
| Merry | counter | +3 Mult/輪 |

#### 3.3 縮放 Joker
| Joker | 狀態類型 | 效果 |
|-------|----------|------|
| Ramen | x_mult_mod | X2 Mult, -0.01/棄牌 |
| SteakJoker | counter | X2 Mult, 售價 -$1/輪 |
| Campfire | x_mult_mod | +0.25 X Mult/賣牌 |

---

### Phase 4: Boss Blind 完整實作 (Day 10-11, ~21h)

**目標**: 實作所有 27 個 Boss Blind 效果

#### 4.1 分數修正類 (已部分實作)
- [ ] TheWall: 4x 分數
- [ ] VioletVessel: 6x 分數 (Ante 8)
- [ ] TheFlint: chips/mult 減半

#### 4.2 禁用類
- [ ] TheClub/Diamond/Heart/Spade: 花色禁用
- [ ] ThePlant: 人頭牌禁用
- [ ] TheHead: 紅心只能首手出

#### 4.3 面朝下類
- [ ] TheWheel: 1/7 牌朝下
- [ ] TheHouse: 首手朝下
- [ ] TheMark: 人頭牌朝下
- [ ] Verdant: 全部朝下 (Ante 8)

#### 4.4 限制類
- [ ] ThePsychic: 必須 5 張
- [ ] TheEye: 不能重複牌型
- [ ] TheMouth: 只能一種牌型
- [ ] TheNeedle: 只能出 1 手
- [ ] ThePillar: 已出牌不再計分

#### 4.5 經濟/特殊類
- [ ] TheOx: 出 Ante 數張牌失 $1
- [ ] TheManacle: 手牌上限 -1
- [ ] TheArm: 降低牌型等級
- [ ] Cerulean: 強制使用消耗品 (Ante 8)
- [ ] Amber: 禁用消耗品 (Ante 8)

---

### Phase 5: 卡包系統 (Day 12-13, ~16h)

**目標**: 實作卡包開啟邏輯

#### 5.1 卡包類型
- **檔案**: `src/service/state.rs`
```rust
pub enum PackType {
    Arcana,     // Tarot 包
    Celestial,  // Planet 包
    Spectral,   // Spectral 包
    Standard,   // 普通牌包
    Buffoon,    // Joker 包
}
```

#### 5.2 卡包開啟流程
- [ ] 定義 `PackOpeningState` 狀態
- [ ] 生成卡包內容 (根據類型)
- [ ] 實作選牌邏輯 (簡化版: 自動選第一張)
- [ ] 更新 `ACTION_TYPE_BUY_PACK` 處理

---

### Phase 6: 剩餘 Joker 效果 (Day 14-16, ~24h)

**目標**: 實作剩餘 79 個 Joker 效果

#### 6.1 規則修改類 (高優先)
| Joker | 效果 |
|-------|------|
| FourFingers | 4 張可組順子/同花 |
| Shortcut | 順子可跳 1 點 |
| Splash | 所有牌計入所有牌型 |
| Pareidolia | 所有牌視為人頭牌 |

#### 6.2 觸發器類
| Joker | 效果 |
|-------|------|
| Cartomancer | Skip Blind 生成 Tarot |
| Astronomer | Skip Blind 生成 Planet |
| Vagabond | ≤4 張出牌生成 Tarot |
| SpaceJoker | 1/4 機率升級牌型等級 |

#### 6.3 Meta-Joker (複雜，可延後)
| Joker | 效果 |
|-------|------|
| Blueprint | 複製右邊 Joker 能力 |
| Brainstorm | 複製最左 Joker 能力 |

---

### Phase 7: Tag 效果 (Day 16+, ~12h)

**目標**: 實作 22 個 Tag 效果

#### 7.1 立即效果 (已部分實作)
- [ ] EconomyTag: +$10
- [ ] SpeedTag: +$25

#### 7.2 創造效果
- [ ] UncommonTag: 獲得 Uncommon Joker
- [ ] RareTag: 獲得 Rare Joker
- [ ] NegativeTag: 獲得 Negative Joker
- [ ] BuffoonTag: 獲得 Buffoon 包

#### 7.3 商店效果
- [ ] CouponTag: 下個商店打折
- [ ] D6Tag: 免費 Reroll

---

## 關鍵檔案修改清單

| 檔案 | 修改類型 | 優先級 |
|------|----------|--------|
| `src/game/consumables.rs` | 重寫 | P0 |
| `src/game/joker.rs` | 擴展 JokerState + 79 效果 | P0 |
| `src/game/blinds.rs` | 加入效果應用邏輯 | P0 |
| `src/service/scoring.rs` | Boss Blind 效果整合 | P0 |
| `src/main.rs` | 消耗品/卡包 action 處理 | P0 |
| `src/service/state.rs` | 卡包狀態、手牌等級 | P1 |
| `src/service/observation.rs` | 消耗品/Joker 狀態編碼 | P1 |
| `src/game/tags.rs` | Tag 效果實作 | P2 |

---

## 驗證計劃

### 單元測試
```bash
# 每個 Phase 完成後執行
cargo test

# 特定模組測試
cargo test consumables
cargo test joker
cargo test blinds
```

### 整合測試
```bash
# 啟動 gRPC 服務
cargo run --release &

# Python 端驗證
cd ../python-env
pytest tests/test_reward.py -v
python -c "from joker_env import JokerEnv; env = JokerEnv(); env.reset()"
```

### 訓練驗證
```bash
# 短期訓練測試
python train.py --episodes=1000 --validate
```

---

## 風險與緩解

| 風險 | 影響 | 緩解措施 |
|------|------|----------|
| Observation 空間變化 | 需重新訓練模型 | 批次更新，一次性調整 |
| 消耗品複雜度 | 延遲交付 | 先實作 Planet，其他漸進 |
| Joker 交互 bug | 錯誤獎勵信號 | 每個 Joker 單獨測試 |

---

## 成功指標

| 指標 | 當前 | 目標 |
|------|------|------|
| Joker 效果 | 71/150 (47%) | 150/150 (100%) |
| Boss Blind 效果 | 6/27 (22%) | 27/27 (100%) |
| 消耗品 | 0/52 (0%) | 52/52 (100%) |
| Tag 效果 | 3/22 (14%) | 22/22 (100%) |
| 整體保真度 | 48% | 95%+ |
