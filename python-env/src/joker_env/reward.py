"""
獎勵計算系統 - Python 端 (v10.0)

從 Rust reward.rs 移植，為 RL 訓練提供形狀良好的獎勵信號，支持完整遊戲（Ante 1-8）

獎勵範圍設計（v10.0 - 商店品質感知的 Reroll/Skip 策略）：

v10.0 新增功能（基於 super-analyst-pro 分析報告）：
1. reroll_reward_v2：商店品質感知的 Reroll 獎勵
   - 早期（Ante ≤ 3）+ 低 Joker 數（≤ 2）= 探索模式，Reroll 可獲正向獎勵
   - 商店品質低時鼓勵 Reroll，品質高時懲罰 Reroll
   - Reroll 預算追蹤：前 2 次正常，第 3 次開始遞減獎勵
   - 預期效果：Reroll 率從 0% → 2-4%，Ante 提升 20-30%

2. skip_blind_reward_v2：狀態感知的 Skip Blind 獎勵
   - 動態機會成本：商店品質高 + 有錢 = 機會成本高
   - 風險溢價基於 Joker 數量，而非僅 Ante
   - Context Multiplier：Joker 快滿 + 沒錢 = 商店價值低 = Skip 更有吸引力
   - 預期效果：Skip 率從 2.42% → 8-10%，Ante 提升 5-8%

3. shop_quality_score（Rust 端計算）：
   - 稀有度權重 (40%)：Common=0.2, Uncommon=0.5, Rare=0.8, Legendary=1.0
   - 協同效果 (30%)：與已擁有 Joker 的協同關係
   - 成本效益 (20%)：價格合理性
   - 特殊加成 (10%)：xMult/Boss Killer/經濟 Joker

4. reroll_count_this_shop：Reroll 預算追蹤，防止過度 Reroll

獎勵範圍設計（v7.0 - Boss/Joker 協同獎勵）：

v7.0 新增功能（輔助性正向獎勵，不主導現有結構）：
1. Boss 難度獎勵：困難 Boss 過關給予額外 0.0~0.15 獎勵
   - 難度係數根據 Boss 類型（Wall=1.4, Needle=1.4, Violet=1.5）
   - 效率加成：剩餘出牌次數越多獎勵越高
2. Joker 協同獎勵：持有協同組合在 CASH_OUT 時給予 0.0~0.12 獎勵
   - 協同群組（diamond_synergy, pair_power, scaling_xmult 等）
   - Build 對齊獎勵（Joker 能力匹配主導風格）
3. 牌型針對性獎勵：打出匹配 Joker 能力的牌型給予 0.0~0.10 獎勵
   - 根據持有 Joker 對應的 build 計算匹配度

獎勵範圍設計（v6.9 - Joker 貢獻與高效出牌獎勵）：

v6.9 新增功能（純正向獎勵，不增加懲罰）：
1. Joker 貢獻獎勵：當 Joker 對分數有顯著貢獻時給予 0.0~0.08 獎勵
   - x_mult 權重最高（0.5），因為乘法加成是後期關鍵
   - chips 權重最低（0.2），因為影響較小
2. 分數效率獎勵：當單次出牌超過預期分數時給予 0.0~0.06 獎勵
   - 預期分數 = blind_target / 4（假設 4 次出牌過關）
   - 只獎勵超額部分，不懲罰低於預期

獎勵範圍設計（v6.7 - 修復 Joker 經濟循環）：
設計原則：
- 終端獎勵主導：勝利=5.0，確保長期目標壓過短期收益
- Joker 保護機制：低 Joker 數量時嚴禁賣出，持有 Joker 給予獎勵
- 牌型品質獎勵：大幅強化牌型差距（3.5x），解決 95% High Card/Pair 問題
- 棄牌改善獎勵：啟用 hand_setup_reward，鼓勵有目的的棄牌
- 效率獎勵：早期過關給予額外獎勵

v6.7 核心修復（針對「買 1 個 Joker → 金幣耗盡 → 無法過 Boss」循環）：
1. Joker 購買階梯重新設計：首個 ×1.5（降低），第 2 個 ×3.0（大幅提升）
2. 商店存錢獎勵：每步給予小獎勵，鼓勵累積金幣
3. Joker 不足懲罰加重：-0.03/個（原 -0.01）
4. Boss 準備獎勵：Big Blind 過關後有錢給額外獎勵
5. Ante 3 最低要求提高到 3 個 Joker

v6.4 核心修復（針對 95% High Card/Pair 問題）：
1. HAND_TYPE_BONUSES 放大 3.5 倍（對抗 VecNormalize 壓縮）
2. 啟用 hand_setup_reward()（原本存在但未被調用）
3. 棄牌反循環保護：連續棄牌 > 2 次給予累進懲罰
4. 效率獎勵：剩餘出牌次數越多過關，獎勵越高
5. Pair 獎勵降為 0（基線），不再鼓勵安全策略

v6.3 強化 Boss 獎勵（針對 Boss Clear 0% 瓶頸）：
1. Boss 基礎獎勵從 0.50 提升到 0.80
2. Boss 額外加成從 0.05 提升到 0.10
3. 效率獎勵從 0.01/play 提升到 0.02/play
4. Boss 階段每個動作 +0.0001（鼓勵積極面對）

v6.0-6.2 歷史修復：
- sell_joker_reward: Joker <= 2 時賣出重罰
- joker_holding_bonus: 持有 Joker 給予獎勵
- 超額獎勵使用對數縮放

| 模組                     | 範圍             | 說明                              |
|--------------------------|------------------|-----------------------------------|
| 遊戲結束 (game_end)      | -2.0 ~ 5.0       | 勝利=5.0，失敗依進度懲罰          |
| Ante 進度                | 0.56 ~ 1.76+里程碑| 更陡峭曲線 + 里程碑獎勵          |
| 過關 (blind_clear)       | 0.25 ~ 1.50      | Ante 係數 0.15，後期更高          |
| 出牌 (play_reward)       | -0.05 ~ 0.35     | 牌型獎勵 + 效率獎勵（v6.4）       |
| 棄牌 (discard_reward)    | -0.07 ~ 0.06     | 改善獎勵 - 反循環懲罰（v6.4）     |
| 購買 Joker               | -0.3 ~ 0.5       | 早期購買加倍獎勵                  |
| Skip Blind/Tag           | -0.20 ~ 0.25     | 提高機會成本，調整風險係數        |
| 消耗品使用               | 0.0 ~ 0.25       | Spectral 後期乘數更強             |
| 金幣狀態 (money_reward)  | 0.0 ~ 0.2        | 利息閾值階梯獎勵                  |
| Joker 持有獎勵           | -0.05 ~ 0.08     | 持有 Joker 給予獎勵               |
| Reroll 決策              | -0.15 ~ 0.0      | 考慮利息損失（純經濟懲罰）        |
| 出售 Joker               | -0.5 ~ 0.1       | 低數量嚴罰，僅滿槽可正向          |
| Voucher 購買             | -0.25 ~ 0.3      | 含階段權重、經濟懲罰              |
"""

from dataclasses import dataclass
from typing import Optional
import math

# Action types (與 env.py 一致)
ACTION_TYPE_SELECT = 0
ACTION_TYPE_PLAY = 1
ACTION_TYPE_DISCARD = 2
ACTION_TYPE_SELECT_BLIND = 3
ACTION_TYPE_CASH_OUT = 4
ACTION_TYPE_BUY_JOKER = 5
ACTION_TYPE_NEXT_ROUND = 6
ACTION_TYPE_REROLL = 7
ACTION_TYPE_SELL_JOKER = 8
ACTION_TYPE_SKIP_BLIND = 9
ACTION_TYPE_USE_CONSUMABLE = 10
ACTION_TYPE_BUY_VOUCHER = 11
ACTION_TYPE_BUY_PACK = 12

# Stage constants
STAGE_PRE_BLIND = 0
STAGE_BLIND = 1
STAGE_POST_BLIND = 2
STAGE_SHOP = 3
STAGE_END = 4

# Blind type constants
BLIND_SMALL = 0
BLIND_BIG = 1
BLIND_BOSS = 2

# Game end constants
GAME_END_NONE = 0
GAME_END_WIN = 1
GAME_END_LOSE = 2

# ============================================================================
# v7.0: Boss Blind 常量（對應 Rust blinds.rs BossBlind::to_int）
# ============================================================================

BOSS_HOOK = 0       # 每手開始隨機棄 2 張
BOSS_WALL = 1       # 需要 4x 分數
BOSS_WHEEL = 2      # 1/7 牌面朝下
BOSS_ARM = 3        # 降低牌型等級
BOSS_FLINT = 4      # 基礎 chips/mult 減半
BOSS_CLUB = 5       # 梅花不計分
BOSS_DIAMOND = 6    # 方塊不計分
BOSS_HEART = 7      # 紅心不計分
BOSS_SPADE = 8      # 黑桃不計分
BOSS_PSYCHIC = 9    # 必須出 5 張
BOSS_MOUTH = 10     # 只能出一種牌型
BOSS_EYE = 11       # 不能重複牌型
BOSS_PLANT = 12     # Face Card 不計分
BOSS_SERPENT = 13   # 每次出牌後抽 3 棄 3
BOSS_OX = 14        # 出特定牌型失去 $1
BOSS_HOUSE = 15     # 第一手面朝下
BOSS_MARK = 16      # Face Card 面朝下
BOSS_FISH = 17      # 面朝下牌打亂
BOSS_MANACLE = 18   # 手牌上限 -1
BOSS_PILLAR = 19    # 打過的牌不再計分
BOSS_NEEDLE = 20    # 只有 1 次出牌機會
BOSS_HEAD = 21      # 紅心只能第一手出
BOSS_VIOLET = 22    # 需要 6x 分數
BOSS_CRIMSON = 23   # hand 數 -1
BOSS_CERULEAN = 24  # 強制使用消耗品
BOSS_AMBER = 25     # 無法使用消耗品
BOSS_VERDANT = 26   # 所有牌回合開始面朝下

# Boss 分類
BOSS_SUIT_DISABLE = {BOSS_CLUB, BOSS_DIAMOND, BOSS_HEART, BOSS_SPADE}
BOSS_HAND_RESTRICT = {BOSS_PSYCHIC, BOSS_MOUTH, BOSS_EYE}
BOSS_SCORE_MULT = {BOSS_WALL, BOSS_FLINT, BOSS_VIOLET}
BOSS_SHOWDOWN = {BOSS_VIOLET, BOSS_CRIMSON, BOSS_CERULEAN, BOSS_AMBER, BOSS_VERDANT}

# Boss 難度係數（用於 boss_clear_difficulty_bonus）
# 基準 1.0 = 標準難度，>1.0 = 更困難
BOSS_DIFFICULTY = {
    BOSS_HOOK: 0.8,       # Easy: 可預測
    BOSS_WALL: 1.4,       # Very Hard: 4x 分數
    BOSS_WHEEL: 0.9,      # Easy-Medium: 隨機但可管理
    BOSS_ARM: 1.0,        # Medium: 降級影響長期
    BOSS_FLINT: 1.3,      # Hard: 減半需要更強組合
    BOSS_CLUB: 1.1,       # Medium: 花色禁用
    BOSS_DIAMOND: 1.1,
    BOSS_HEART: 1.1,
    BOSS_SPADE: 1.1,
    BOSS_PSYCHIC: 1.2,    # Hard: 限制組合靈活性
    BOSS_MOUTH: 1.2,      # Hard: 限制策略
    BOSS_EYE: 1.1,        # Medium-Hard: 需要多種牌型
    BOSS_PLANT: 1.2,      # Hard: 限制高分牌
    BOSS_SERPENT: 0.9,    # Easy-Medium: 有時幫助
    BOSS_OX: 0.85,        # Easy: 經濟懲罰較輕
    BOSS_HOUSE: 0.95,     # Easy-Medium: 首手隨機
    BOSS_MARK: 1.0,       # Medium
    BOSS_FISH: 0.95,      # Easy-Medium
    BOSS_MANACLE: 1.1,    # Medium-Hard: 手牌限制
    BOSS_PILLAR: 1.3,     # Hard: 無法重複高分牌
    BOSS_NEEDLE: 1.4,     # Very Hard: 只有 1 次機會
    BOSS_HEAD: 1.0,       # Medium
    BOSS_VIOLET: 1.5,     # Showdown: 6x 分數
    BOSS_CRIMSON: 1.3,    # Showdown: 出牌機會減少
    BOSS_CERULEAN: 1.1,   # Showdown: 消耗品限制
    BOSS_AMBER: 1.2,      # Showdown: 無消耗品
    BOSS_VERDANT: 1.2,    # Showdown: 視野限制
}

# Tag ID constants (對應 Rust tags.rs)
TAG_UNCOMMON = 0
TAG_RARE = 1
TAG_NEGATIVE = 2
TAG_FOIL = 3
TAG_HOLOGRAPHIC = 4
TAG_POLYCHROME = 5
TAG_INVESTMENT = 6
TAG_VOUCHER = 7
TAG_BOSS = 8
TAG_STANDARD = 9
TAG_BUFFOON = 10
TAG_METEOR = 11
TAG_ETHEREAL = 12
TAG_CELESTIAL = 13
TAG_COUPON = 14
TAG_DOUBLE = 15
TAG_JUGGLE = 16
TAG_D6 = 17
TAG_TOP_UP = 18
TAG_SPEED = 19
TAG_ORBITAL = 20
TAG_ECONOMY = 21
TAG_HANDY = 22
TAG_GARBAGE = 23
TAG_CHARM = 24

# Tag 價值映射（基於遊戲影響力估算）
# 高價值：Joker 強化 / 稀有 Joker
# 中價值：經濟 / 商店優惠
# 低價值：跳過商店 / 小獎勵
TAG_VALUES = {
    TAG_UNCOMMON: 0.25,      # 免費 Uncommon Joker
    TAG_RARE: 0.40,          # 免費 Rare Joker
    TAG_NEGATIVE: 0.52,      # Negative 版本 = 額外槽位，最高價值
    TAG_FOIL: 0.20,          # Foil 版本 +50 chips
    TAG_HOLOGRAPHIC: 0.25,   # Holo 版本 +10 mult
    TAG_POLYCHROME: 0.35,    # Polychrome 版本 ×1.5 mult
    TAG_INVESTMENT: 0.18,    # +$25 回合結束（延遲）
    TAG_VOUCHER: 0.30,       # 免費 Voucher
    TAG_BOSS: 0.15,          # 重抽 Boss（情境性）
    TAG_STANDARD: 0.12,      # Standard Pack
    TAG_BUFFOON: 0.20,       # Buffoon Pack (Joker)
    TAG_METEOR: 0.15,        # Meteor Pack
    TAG_ETHEREAL: 0.18,      # Ethereal Pack (Spectral)
    TAG_CELESTIAL: 0.15,     # Celestial Pack (Planet)
    TAG_COUPON: 0.20,        # 商店 50% off
    TAG_DOUBLE: 0.22,        # 複製下一個 Tag（依賴下一個）
    TAG_JUGGLE: 0.10,        # +3 手牌大小（暫時）
    TAG_D6: 0.15,            # 免費 Reroll
    TAG_TOP_UP: 0.12,        # 2 個消耗品
    TAG_SPEED: 0.06,         # $25 但跳過商店（風險）
    TAG_ORBITAL: 0.20,       # 升級牌型
    TAG_ECONOMY: 0.12,       # +$10
    TAG_HANDY: 0.10,         # $1 per hand（基礎）
    TAG_GARBAGE: 0.08,       # $1 per discard（基礎）
    TAG_CHARM: 0.18,         # Mega Arcana Pack
}

# 計算加權平均 Tag 價值（假設均勻分布）
AVG_TAG_VALUE = sum(TAG_VALUES.values()) / len(TAG_VALUES)  # ≈ 0.20

# ============================================================================
# Hand types / build tracking (aligned with Rust HandId order)
# ============================================================================

HAND_HIGH_CARD = 0
HAND_PAIR = 1
HAND_TWO_PAIR = 2
HAND_THREE_KIND = 3
HAND_STRAIGHT = 4
HAND_FLUSH = 5
HAND_FULL_HOUSE = 6
HAND_FOUR_KIND = 7
HAND_STRAIGHT_FLUSH = 8
HAND_ROYAL_FLUSH = 9
HAND_FIVE_KIND = 10
HAND_FLUSH_HOUSE = 11
HAND_FLUSH_FIVE = 12

BUILD_PAIRS = 0
BUILD_STRAIGHT = 1
BUILD_FLUSH = 2

_HAND_STRENGTH_ORDER = {
    HAND_HIGH_CARD: 0,
    HAND_PAIR: 1,
    HAND_TWO_PAIR: 2,
    HAND_THREE_KIND: 3,
    HAND_STRAIGHT: 4,
    HAND_FLUSH: 5,
    HAND_FULL_HOUSE: 6,
    HAND_FOUR_KIND: 7,
    HAND_STRAIGHT_FLUSH: 8,
    HAND_ROYAL_FLUSH: 9,
    HAND_FIVE_KIND: 10,
    HAND_FLUSH_HOUSE: 11,
    HAND_FLUSH_FIVE: 12,
}

# v6.5: 牌型品質獎勵 — 與過關獎勵對齊
# 核心思路：Four Kind (+0.60) 應接近 Boss 過關 (+0.80)，因為打出 Four Kind 幾乎等於過關
# Pair 改為小懲罰，不再是安全基線
HAND_TYPE_BONUSES = {
    HAND_HIGH_CARD: -0.08,        # 加重懲罰（原 -0.05）
    HAND_PAIR: -0.02,             # 小懲罰，不再是安全基線（原 0.00）
    HAND_TWO_PAIR: 0.05,          # 略微正向
    HAND_THREE_KIND: 0.20,        # 2x 放大（原 0.10）
    HAND_STRAIGHT: 0.28,          # 2x 放大（原 0.14）
    HAND_FLUSH: 0.32,             # 2x 放大（原 0.16）
    HAND_FULL_HOUSE: 0.40,        # 2x 放大（原 0.20）
    HAND_FOUR_KIND: 0.60,         # 關鍵！接近 Boss 過關（原 0.28）
    HAND_STRAIGHT_FLUSH: 0.75,    # 超高獎勵（原 0.35）
    HAND_ROYAL_FLUSH: 0.85,       # 頂級獎勵（原 0.42）
    HAND_FIVE_KIND: 0.75,         # 同 Straight Flush（原 0.35）
    HAND_FLUSH_HOUSE: 0.85,       # 同 Royal Flush（原 0.42）
    HAND_FLUSH_FIVE: 1.00,        # 最強牌型，超過 Boss 過關（原 0.52）
}

_BUILD_HANDS = {
    BUILD_PAIRS: {
        HAND_PAIR,
        HAND_TWO_PAIR,
        HAND_THREE_KIND,
        HAND_FULL_HOUSE,
        HAND_FOUR_KIND,
        HAND_FIVE_KIND,
        HAND_FLUSH_HOUSE,
    },
    BUILD_STRAIGHT: {HAND_STRAIGHT, HAND_STRAIGHT_FLUSH, HAND_ROYAL_FLUSH},
    BUILD_FLUSH: {HAND_FLUSH, HAND_STRAIGHT_FLUSH, HAND_ROYAL_FLUSH, HAND_FLUSH_HOUSE, HAND_FLUSH_FIVE},
}

# ============================================================================
# v7.0: 擴展 Joker Build Support（對應 Rust joker.rs JokerId）
# ============================================================================

# Joker build support mapping - 每個 Joker 支援的 build 風格
JOKER_BUILD_SUPPORT = {
    # Pair 類（Joker 獎勵 pairs/sets）
    5: BUILD_PAIRS,     # JollyJoker: +8 Mult if contains Pair
    6: BUILD_PAIRS,     # ZanyJoker: +12 Mult if contains Three of a Kind
    10: BUILD_PAIRS,    # SlyJoker: +50 Chips if contains Pair
    11: BUILD_PAIRS,    # WilyJoker: +100 Chips if contains Three of a Kind
    111: BUILD_PAIRS,   # The_Duo: X2 Mult if contains Pair
    112: BUILD_PAIRS,   # The_Trio: X3 Mult if contains Three of a Kind
    113: BUILD_PAIRS,   # The_Family: X4 Mult if contains Four of a Kind

    # Straight 類（Joker 獎勵 straight）
    8: BUILD_STRAIGHT,   # CrazyJoker: +12 Mult if contains Straight
    13: BUILD_STRAIGHT,  # DeviousJoker: +100 Chips if contains Straight
    114: BUILD_STRAIGHT, # The_Order: X3 Mult if contains Straight
    29: BUILD_STRAIGHT,  # FourFingers: 允許 4 卡 Straight
    131: BUILD_STRAIGHT, # Shortcut: 允許跳躍 Straight

    # Flush 類（Joker 獎勵 flush 或特定花色）
    9: BUILD_FLUSH,      # DrollJoker: +10 Mult if contains Flush
    14: BUILD_FLUSH,     # CraftyJoker: +80 Chips if contains Flush
    115: BUILD_FLUSH,    # The_Tribe: X2 Mult if contains Flush
    1: BUILD_FLUSH,      # GreedyJoker: +3 per Diamond
    2: BUILD_FLUSH,      # LustyJoker: +3 Mult per Heart
    3: BUILD_FLUSH,      # WrathJoker: +3 Mult per Spade (if < 4)
    4: BUILD_FLUSH,      # GluttonJoker: +3 Mult per Club (if < 4)
    30: BUILD_FLUSH,     # Smeared: Hearts/Diamonds 視為同花色, Clubs/Spades 視為同花色
}

# v7.0: Joker 協同群組 - 有協同效果的 Joker 組合
# 擁有同群組的多個 Joker 會獲得協同獎勵
JOKER_SYNERGY_GROUPS = {
    # Diamond 協同：獎勵方塊花色
    "diamond_synergy": {1, 57, 85},  # GreedyJoker, Opal, RoughGem

    # Pair Power：強化對子/組合
    "pair_power": {5, 10, 111, 6, 11, 112, 113},

    # Straight Masters：強化順子
    "straight_masters": {8, 13, 114, 29, 131},

    # Flush Kings：強化同花
    "flush_kings": {9, 14, 115, 30},

    # Scaling X-Mult：累積乘法加成
    "scaling_xmult": {97, 120, 129, 23, 64},  # Vampire, Hologram, Constellation, Cavendish, Campfire

    # Economy：經濟型 Joker
    "economy": {45, 46, 47, 48, 88},  # GoldenJoker, ToTheMoon, Satellite, BullMoney, GoldBar

    # Face Card：獎勵 Face Card
    "face_card": {58, 59, 79, 138},  # SockAndBuskin, Mime, Photograph, TribouletFool

    # Retrigger：重複觸發
    "retrigger": {61, 62, 63, 53},  # Dusk, Hack, SockAndBuskin (different index), Blueprint

    # Boss Killer：對抗 Boss 的特效
    "boss_killer": {68, 118},  # Chicot (禁用 Boss), Matador (+$8 if Boss)
}

# 將 Joker ID 映射到其所屬的協同群組
_JOKER_TO_SYNERGY: dict[int, list[str]] = {}
for group_name, joker_ids in JOKER_SYNERGY_GROUPS.items():
    for jid in joker_ids:
        if jid not in _JOKER_TO_SYNERGY:
            _JOKER_TO_SYNERGY[jid] = []
        _JOKER_TO_SYNERGY[jid].append(group_name)


def blind_progress_signal(
    score_before: int,
    score_after: int,
    blind_target: int,
    plays_left: int,
    total_plays: int = 4,
) -> float:
    """
    Blind 內進度獎勵（細粒度）

    - 進度獎勵：Δscore / target
    - 里程碑：跨過 80% 增加小獎勵
    - 節奏：在預期節奏之前達到更高進度，給額外獎勵
    """
    if blind_target <= 0:
        return 0.0
    if score_after <= score_before:
        return 0.0

    before_ratio = score_before / blind_target
    after_ratio = score_after / blind_target
    progress = max(0.0, after_ratio - before_ratio)

    reward = progress * 0.03

    # 80% 里程碑
    if before_ratio < 0.8 <= after_ratio:
        reward += 0.01

    # 節奏獎勵：越早達標越好
    total_plays = max(1, total_plays)
    plays_used = max(0, total_plays - max(0, plays_left))
    expected_ratio = plays_used / total_plays
    if after_ratio > expected_ratio + 0.2:
        reward += 0.005

    return min(reward, 0.05)


def hand_setup_reward(prev_hand: int, new_hand: int, had_discard: bool) -> float:
    """
    棄牌後牌型改善/變差獎勵（v6.4 修改：加入變差懲罰）

    - 改善：+0.02 ~ +0.08（根據提升幅度）
    - 不變：0
    - 變差：-0.01（輕微懲罰，避免過度保守）
    """
    if not had_discard:
        return 0.0
    if prev_hand < 0 or new_hand < 0:
        return 0.0

    prev_strength = _HAND_STRENGTH_ORDER.get(prev_hand, -1)
    new_strength = _HAND_STRENGTH_ORDER.get(new_hand, -1)
    if prev_strength < 0 or new_strength < 0:
        return 0.0

    # v6.4: 變差懲罰
    if new_strength < prev_strength:
        return -0.01  # 輕微懲罰，避免阻礙策略性棄牌

    # 不變
    if new_strength == prev_strength:
        return 0.0

    # 改善獎勵
    diff = new_strength - prev_strength
    if diff <= 2:
        reward = 0.02
    elif diff <= 4:
        reward = 0.04
    else:
        reward = 0.06 + min(0.02, (diff - 5) * 0.01)

    return min(reward, 0.08)


def potential_change_reward(
    old_flush: float, old_straight: float, old_pairs: float,
    new_flush: float, new_straight: float, new_pairs: float
) -> float:
    """
    v6.4: 手牌潛力變化獎勵（Potential-Based Shaping）

    計算手牌潛力的綜合變化，獎勵提升潛力的棄牌決策。

    潛力權重（根據 Balatro 分數）：
    - Flush: 0.40（Flush 分數高且相對容易達成）
    - Straight: 0.35（Straight 分數高但較難達成）
    - Pairs: 0.25（Pairs 是保底選項）

    返回範圍：約 -0.05 ~ +0.05
    """
    # 加權計算綜合潛力
    old_potential = 0.40 * old_flush + 0.35 * old_straight + 0.25 * old_pairs
    new_potential = 0.40 * new_flush + 0.35 * new_straight + 0.25 * new_pairs

    # 潛力差分，縮放係數 0.1（避免過度主導）
    delta = new_potential - old_potential
    return delta * 0.1  # 範圍約 -0.05 ~ +0.05


class BuildTracker:
    """追蹤主導 build（pairs / straight / flush）"""

    def __init__(self) -> None:
        self.reset()

    def reset(self) -> None:
        self._counts = {BUILD_PAIRS: 0, BUILD_STRAIGHT: 0, BUILD_FLUSH: 0}
        self._total_hands = 0

    def record_hand(self, hand_type: int) -> None:
        self._total_hands += 1
        for build, hands in _BUILD_HANDS.items():
            if hand_type in hands:
                self._counts[build] += 1

    def get_build_weights(self) -> dict[str, float]:
        if self._total_hands == 0:
            return {"pairs": 1.0 / 3, "straight": 1.0 / 3, "flush": 1.0 / 3}
        return {
            "pairs": self._counts[BUILD_PAIRS] / self._total_hands,
            "straight": self._counts[BUILD_STRAIGHT] / self._total_hands,
            "flush": self._counts[BUILD_FLUSH] / self._total_hands,
        }

    def get_dominant_build(self) -> Optional[int]:
        if self._total_hands < 5:
            return None
        weights = self.get_build_weights()
        best = max(
            (BUILD_PAIRS, weights["pairs"]),
            (BUILD_STRAIGHT, weights["straight"]),
            (BUILD_FLUSH, weights["flush"]),
            key=lambda x: x[1],
        )
        return best[0] if best[1] >= 0.6 else None

    def joker_build_bonus(self, joker_id: int, joker_slots: int) -> float:
        dominant = self.get_dominant_build()
        if dominant is None:
            return 0.0
        support = JOKER_BUILD_SUPPORT.get(joker_id)
        if support is None:
            return 0.0
        if support == dominant:
            return 0.02 + 0.01 * min(joker_slots, 5) / 5.0
        return -0.02

# ============================================================================
# Joker Tier 評估系統（簡化版）
# ============================================================================

# Joker 稀有度基礎價值（基於對遊戲勝率的影響）
# 1=Common, 2=Uncommon, 3=Rare, 4=Legendary
JOKER_RARITY_VALUES = {
    1: 0.08,   # Common: 基礎加成，穩定但不強
    2: 0.15,   # Uncommon: 條件加成，更有潛力
    3: 0.25,   # Rare: 強力效果，顯著提升
    4: 0.40,   # Legendary: 遊戲改變級別
}

# 基於成本估算稀有度（成本越高通常稀有度越高）
# Balatro Joker 成本範圍：2-10
def estimate_joker_rarity_from_cost(cost: int) -> int:
    """
    從 Joker 成本估算稀有度

    Args:
        cost: Joker 購買成本

    Returns:
        估算的稀有度 (1-4)
    """
    if cost <= 4:
        return 1  # Common
    elif cost <= 6:
        return 2  # Uncommon
    elif cost <= 8:
        return 3  # Rare
    else:
        return 4  # Legendary


def estimate_joker_value(cost: int, joker_id: Optional[int] = None) -> float:
    """
    估算 Joker 的價值

    Args:
        cost: Joker 購買成本
        joker_id: Joker ID（可選，用於精確評估）

    Returns:
        估算的 Joker 價值 (0.08 ~ 0.40)
    """
    # 未來可以添加基於 joker_id 的精確評估
    # 目前基於成本估算
    rarity = estimate_joker_rarity_from_cost(cost)
    base_value = JOKER_RARITY_VALUES.get(rarity, 0.1)

    # 成本調整：高成本通常意味著更強效果
    cost_bonus = min((cost - 4) * 0.01, 0.05) if cost > 4 else 0.0

    return base_value + cost_bonus


@dataclass
class StepInfo:
    """從 EnvInfo 解析的狀態信息"""
    # 基本狀態
    episode_step: int = 0
    chips: int = 0  # 當前分數
    blind_target: int = 0

    # 擴展狀態
    ante: int = 1
    stage: int = STAGE_PRE_BLIND
    blind_type: int = -1  # -1 = None
    plays_left: int = 0
    discards_left: int = 0
    money: int = 0

    # 事件追蹤
    score_delta: int = 0
    money_delta: int = 0
    last_action_type: int = -1
    last_action_cost: int = 0

    # Joker 狀態
    joker_count: int = 0
    joker_slot_limit: int = 5

    # 遊戲結束狀態
    game_end: int = GAME_END_NONE
    blind_cleared: bool = False

    # 動作細節
    cards_played: int = 0
    cards_discarded: int = 0
    hand_type: int = -1  # -1 = 無

    # Skip Blind 相關
    tag_id: int = -1  # -1 = 無

    # 消耗品相關
    consumable_id: int = -1  # -1 = 無（Tarot: 0-21, Planet: 22-33, Spectral: 34-51）

    # Joker 交易相關
    joker_sold_id: int = -1  # 賣出的 Joker ID (-1 = 無)
    best_shop_joker_cost: int = 0  # 商店中最強 Joker 的成本

    # v6.4: 手牌潛力指標（從 Rust 計算）
    flush_potential: float = 0.0    # 同花潛力 [0, 1]
    straight_potential: float = 0.0  # 順子潛力 [0, 1]
    pairs_potential: float = 0.0     # 對子潛力 [0, 1]

    # v6.9: Joker 貢獻追蹤（用於高分獎勵計算）
    joker_chip_contrib: float = 0.0    # Joker chips 貢獻比例 [0, 1]
    joker_mult_contrib: float = 0.0    # Joker mult 貢獻比例 [0, 1]
    joker_xmult_contrib: float = 0.0   # Joker x_mult 正規化值 [0, 1]
    score_efficiency: float = 0.0      # 分數效率：score_delta / (blind_target / 4)

    # v7.0: Boss Blind 識別
    boss_blind_id: int = -1  # Boss Blind ID (0-26), -1 = 無 Boss


def parse_env_info(info: dict) -> StepInfo:
    """從 gRPC EnvInfo 解析狀態"""
    return StepInfo(
        episode_step=info.get("episode_step", 0),
        chips=info.get("chips", 0),
        blind_target=info.get("blind_target", 0),
        ante=info.get("ante", 1),
        stage=info.get("stage", STAGE_PRE_BLIND),
        blind_type=info.get("blind_type", -1),
        plays_left=info.get("plays_left", 0),
        discards_left=info.get("discards_left", 0),
        money=info.get("money", 0),
        score_delta=info.get("score_delta", 0),
        money_delta=info.get("money_delta", 0),
        last_action_type=info.get("last_action_type", -1),
        last_action_cost=info.get("last_action_cost", 0),
        joker_count=info.get("joker_count", 0),
        joker_slot_limit=info.get("joker_slot_limit", 5),
        game_end=info.get("game_end", GAME_END_NONE),
        blind_cleared=info.get("blind_cleared", False),
        cards_played=info.get("cards_played", 0),
        cards_discarded=info.get("cards_discarded", 0),
        hand_type=info.get("hand_type", -1),
        tag_id=info.get("tag_id", -1),
        consumable_id=info.get("consumable_id", -1),
        joker_sold_id=info.get("joker_sold_id", -1),
        best_shop_joker_cost=info.get("best_shop_joker_cost", 0),
        # v6.4: 手牌潛力指標
        flush_potential=info.get("flush_potential", 0.0),
        straight_potential=info.get("straight_potential", 0.0),
        pairs_potential=info.get("pairs_potential", 0.0),
        # v6.9: Joker 貢獻追蹤
        joker_chip_contrib=info.get("joker_chip_contrib", 0.0),
        joker_mult_contrib=info.get("joker_mult_contrib", 0.0),
        joker_xmult_contrib=info.get("joker_xmult_contrib", 0.0),
        score_efficiency=info.get("score_efficiency", 0.0),
        # v7.0: Boss Blind 識別
        boss_blind_id=info.get("boss_blind_id", -1),
    )


def clamp(value: float, min_val: float, max_val: float) -> float:
    """限制值在範圍內"""
    return max(min_val, min(max_val, value))


def stage_weight_early(ante: int) -> float:
    """早期更重要的階段權重"""
    if ante <= 2:
        return 1.3
    elif ante <= 4:
        return 1.1
    elif ante <= 6:
        return 1.0
    else:
        return 0.8


def stage_weight_late(ante: int) -> float:
    """後期更重要的階段權重"""
    if ante <= 2:
        return 0.9
    elif ante <= 4:
        return 1.0
    elif ante <= 6:
        return 1.1
    else:
        return 1.2


# ============================================================================
# 核心獎勵函數
# ============================================================================

def hand_type_bonus(hand_type: int, ante: int) -> float:
    """
    牌型品質獎勵（v6.0 新增）

    鼓勵打出更強牌型，而非一直打 Pair
    - High Card: -0.01（輕微懲罰）
    - Pair: 0.0（基線）
    - Three Kind+: 正向獎勵
    - 後期 Ante 需要更強牌型
    """
    if hand_type < 0:
        return 0.0
    base = HAND_TYPE_BONUSES.get(hand_type, 0.0)
    # 後期 Ante 牌型獎勵更高
    ante_mult = 1.0 + (ante - 1) * 0.1
    return base * ante_mult


def play_reward(score_gained: int, required: int, hand_type: int = -1, ante: int = 1, plays_left: int = 3) -> float:
    """
    出牌獎勵：正規化到 -0.05~0.35（v6.4 - 加入效率獎勵）

    設計原則：
    - 基礎獎勵 +0.02：鼓勵模型嘗試出牌
    - 進度獎勵：根據得分比例給予額外獎勵
    - 牌型獎勵：打出更強牌型給予額外獎勵（v6.4 放大 3.5x）
    - 低分懲罰：得分低於預期節奏時懲罰
    - 效率獎勵：剩餘出牌次數越多過關，獎勵越高（v6.4 新增）
    """
    # 基礎出牌獎勵
    base_play_bonus = 0.02

    if required <= 0:
        return base_play_bonus

    # 低分懲罰 — 如果得分遠低於應有節奏
    # 預期每次出牌應達到 (required / 4) 的分數
    expected_per_play = required / 4.0
    if score_gained <= 0:
        # 0 分是嚴重問題
        return -0.03
    elif score_gained < expected_per_play * 0.3:
        # 得分低於預期的 30%：懲罰
        low_score_penalty = -0.02
    elif score_gained < expected_per_play * 0.5:
        # 得分低於預期的 50%：輕微懲罰
        low_score_penalty = -0.01
    else:
        low_score_penalty = 0.0

    ratio = score_gained / required

    if ratio >= 1.0:
        # 超額獎勵（使用對數縮放）
        base = 0.12
        overkill_bonus = 0.05 * math.log1p(ratio - 1.0)
        progress_reward = base + overkill_bonus
    else:
        # 未達標：線性獎勵進度
        progress_reward = ratio * 0.12

    # 牌型品質獎勵（v6.4: 放大 3.5x）
    type_bonus = hand_type_bonus(hand_type, ante)

    # v6.4: 效率獎勵 — 剩餘出牌次數越多過關，獎勵越高
    # 鼓勵早期就打出高分，而非拖到最後一手
    efficiency_bonus = 0.0
    if ratio >= 1.0 and plays_left >= 2:
        # 還剩 2+ 次出牌就過關 = 效率獎勵
        # plays_left=2: +0.02, plays_left=3: +0.04, plays_left=4: +0.06
        efficiency_bonus = 0.02 * (plays_left - 1)

    reward = base_play_bonus + progress_reward + type_bonus + low_score_penalty + efficiency_bonus
    return clamp(reward, -0.10, 1.00)  # v6.5: 上限提高到 1.0（配合強牌型獎勵）


def discard_reward(cards_discarded: int, discards_left: int) -> float:
    """
    棄牌獎勵：加重棄牌成本以防止「棄牌循環」（v5.1）

    - 空棄牌（cards_discarded==0）懲罰 -0.05（阻斷 no-op 漏洞）
    - 有棄牌：懲罰 -0.03（加重成本，原 -0.01 太輕導致過度棄牌）

    設計原則：
    - 棄牌成本需高於「不確定性規避」的心理收益
    - Joker 連動效果會通過 score_delta/money_delta 自動體現
    - 配合 play_reward 的 +0.02 基礎獎勵，形成「出牌優於棄牌」的激勵
    """
    if cards_discarded == 0:
        return -0.05  # 懲罰空棄牌（no-op exploit 防護）

    # 加重棄牌成本，打破棄牌循環
    return -0.02


def blind_clear_reward(
    plays_left: int,
    blind_type: int,
    ante: int,
    boss_blind_id: Optional[int] = None
) -> float:
    """
    過關獎勵：強化 Boss 獎勵（v6.3）

    設計原則：
    - Boss 過關獎勵大幅提升（是 Small 的 4 倍）
    - 過關獎勵隨 Ante 顯著增加
    - Ante 3+ 的過關獎勵明顯高於早期

    v6.3 修改：Boss 基礎獎勵從 0.50 提升到 0.80
    """
    # 基礎獎勵（v6.3: Boss 大幅提升）
    base = {
        BLIND_SMALL: 0.20,   # 略降
        BLIND_BIG: 0.30,     # 略降
        BLIND_BOSS: 0.80,    # 大幅提升（原 0.50）
    }.get(blind_type, 0.20)

    # Boss 難度加成
    boss_bonus = 0.0
    if blind_type == BLIND_BOSS:
        boss_bonus = 0.10  # 提高（原 0.05）

    # 效率獎勵（剩餘出牌次數）
    efficiency = plays_left * 0.02  # 提高（原 0.01）

    # Ante 階段權重（v5.2: 更陡峭的增長）
    # Ante 1: 1.0, Ante 3: 1.3, Ante 5: 1.6, Ante 8: 2.05
    ante_mult = 1.0 + (ante - 1) * 0.15

    return clamp((base + boss_bonus + efficiency) * ante_mult, 0.20, 1.50)


def ante_progress_reward(old_ante: int, new_ante: int) -> float:
    """
    Ante 進度獎勵：強化中後期增長（v5.2）

    設計原則：
    - 使用更陡峭的曲線讓中後期獎勵更明顯
    - 公式：reward = 0.12 × a^2 + 0.08 × a
    - 累積獎勵：1→2: 0.56, 2→3: 0.80, 3→4: 1.04, 7→8: 1.76
    - 加入里程碑獎勵：Ante 3/5/7 額外 +0.3/+0.5/+0.8
    """
    def ante_value(a: int) -> float:
        # 更陡峭的曲線：0.12 × a^2 + 0.08 × a
        if a < 1:
            return 0.0
        return 0.12 * (a ** 2) + 0.08 * a

    base_reward = ante_value(new_ante) - ante_value(old_ante)

    # 里程碑獎勵：鼓勵突破關鍵 Ante
    milestone_bonus = 0.0
    if old_ante < 3 <= new_ante:
        milestone_bonus += 0.3  # 首次進入 Ante 3
    if old_ante < 5 <= new_ante:
        milestone_bonus += 0.5  # 首次進入 Ante 5
    if old_ante < 7 <= new_ante:
        milestone_bonus += 0.8  # 首次進入 Ante 7

    return base_reward + milestone_bonus


def game_end_reward(game_end: int, ante: int) -> float:
    """
    遊戲結束獎勵（v4.0 - 強化終端信號）

    設計原則：
    - 勝利獎勵大幅提升到 5.0（超過所有中間獎勵累積）
    - 失敗懲罰根據進度調整：-2.0（Ante 1 失敗）到 -0.5（Ante 8 失敗）
    - 這確保終端目標（勝利）是最重要的信號
    """
    if game_end == GAME_END_WIN:
        return 5.0  # 大幅提升勝利獎勵
    elif game_end == GAME_END_LOSE:
        # 進度越高，懲罰越小（鼓勵嘗試更高 Ante）
        progress = ante / 8.0
        # Ante 1 失敗: -2.0, Ante 8 失敗: -0.5
        return -2.0 + 1.5 * progress
    return 0.0


def money_reward(money: int, ante: int) -> float:
    """
    金幣獎勵：考慮利息閾值（0~0.2）

    - 每 $5 一級利息，最高 $25
    - 接近下一閾值有額外獎勵
    """
    # 利息閾值獎勵
    interest_tier = clamp(int(money / 5), 0, 5)
    base_interest = interest_tier * 0.025

    # 接近下一閾值獎勵
    next_threshold = (interest_tier + 1) * 5
    gap_to_next = max(0, next_threshold - money)
    threshold_bonus = 0.015 if interest_tier < 5 and gap_to_next <= 2 else 0.0

    # 階段權重
    stage_weights = {1: 1.4, 2: 1.4, 3: 1.1, 4: 1.1, 5: 0.8, 6: 0.8, 7: 0.5, 8: 0.5}
    stage_weight = stage_weights.get(ante, 1.0)

    return min((base_interest + threshold_bonus) * stage_weight, 0.2)


def smooth_economic_penalty(cost_ratio: float) -> float:
    """
    平滑的經濟懲罰曲線

    使用對數函數確保平滑過渡，避免在任何點產生陡峭跳變。

    範圍：0 (cost_ratio=0) → ~0.10 (cost_ratio=1.0)
    - cost_ratio=0.25: ~0.03
    - cost_ratio=0.50: ~0.05
    - cost_ratio=0.75: ~0.06
    - cost_ratio=1.00: ~0.07
    """
    if cost_ratio <= 0:
        return 0.0
    # log1p(x) = log(1+x)，確保 x=0 時結果為 0
    return 0.05 * math.log1p(cost_ratio * 3)


def joker_buy_reward(
    cost: int,
    money_before: int,
    ante: int,
    joker_count_before: int,
    joker_count_after: int,
    joker_slot_limit: int,
    joker_id: Optional[int] = None
) -> float:
    """
    購買 Joker 獎勵（-0.3~0.5）（v6.0 - 提高早期購買價值）

    考慮因素：
    - Joker 價值（基於成本估算稀有度）
    - 早期購買獎勵加倍（v6.0 核心修復）
    - 經濟懲罰（成本占比，使用平滑曲線）
    - 利息損失
    - 槽位壓力
    """
    # 是否成功購買
    if joker_count_after <= joker_count_before:
        return 0.0

    # Joker 價值估算（基於成本和可選的 joker_id）
    joker_value = estimate_joker_value(cost, joker_id)

    # v6.7: 調整購買階梯 — 強調第 2-3 個 Joker 的重要性
    # 避免「花光買 1 個」的陷阱，鼓勵存錢買更多
    if ante <= 3 and joker_count_after == 1:
        joker_value *= 1.5  # 首個 Joker：降低（原 2.5），避免花光所有錢
    elif ante <= 3 and joker_count_after == 2:
        joker_value *= 3.0  # 第 2 個 Joker：大幅提升！這是關鍵突破點
    elif ante <= 4 and joker_count_after == 3:
        joker_value *= 2.5  # 第 3 個 Joker：重要（原 1.8）
    elif ante <= 4 and joker_count_after <= 4:
        joker_value *= 1.5  # 第 4 個 Joker：穩定提升

    # v6.8 MVF: 早期（Ante <= 3）完全移除經濟懲罰
    # 這是關鍵修復：打破「買 1 個 Joker → 金幣耗盡 → 無法過 Boss」的惡性循環
    # 專家分析顯示：19-20M 步的突破（18% Ante 2+）發生在 v6.3 時代，
    # 當時經濟懲罰較輕；v6.4 加重懲罰後導致 policy collapse
    if ante <= 3:
        economic_penalty = 0.0
        interest_loss = 0.0
    else:
        cost_ratio = min(cost / money_before, 1.0) if money_before > 0 else 1.0
        economic_penalty = smooth_economic_penalty(cost_ratio)
        # 利息損失
        money_after = money_before - cost
        interest_before = min(money_before // 5, 5)
        interest_after = min(money_after // 5, 5)
        interest_loss = 0.02 * (interest_before - interest_after) if interest_after < interest_before else 0.0

    # 階段權重（早期購買 Joker 更有價值）
    stage_mult = stage_weight_early(ante)

    # 槽位考量：接近滿槽時需要更好的 Joker 才值得
    slot_ratio = joker_count_after / joker_slot_limit
    slot_penalty = 0.03 * slot_ratio if slot_ratio > 0.8 else 0.0

    # 獎勵計算：Joker 價值 × 階段權重 - 各種懲罰
    reward = joker_value * stage_mult - economic_penalty - interest_loss - slot_penalty
    return clamp(reward, -0.3, 0.5)  # 上限提高到 0.5


def get_tag_value(tag_id: Optional[int] = None) -> float:
    """
    獲取 Tag 的價值

    Args:
        tag_id: Tag ID，None 則返回平均值
    """
    if tag_id is not None and tag_id in TAG_VALUES:
        return TAG_VALUES[tag_id]
    return AVG_TAG_VALUE


def skip_blind_reward(
    blind_type: int,
    ante: int,
    tag_id: Optional[int] = None
) -> float:
    """
    跳過 Blind 獎勵

    Args:
        blind_type: Blind 類型
        ante: 當前 Ante
        tag_id: 獲得的 Tag ID（可選，None 則使用平均值）

    獎勵設計：
    - Tag 價值基於 TAG_VALUES 映射（0.06 ~ 0.52）
    - 機會成本考慮 Blind 獎金
    - 後期跳過風險更高（風險調整）
    """
    # 機會成本（跳過 Blind 放棄的獎金 + 過關獎勵差距 + 商店訪問價值）
    # v5.0: 提高成本以反映商店機會損失
    opportunity_cost = {
        BLIND_SMALL: 0.18,  # 放棄 $3 + 過關獎勵 + 商店訪問價值
        BLIND_BIG: 0.25,    # 放棄 $5 + 更高分數獎勵 + 商店訪問
        BLIND_BOSS: 2.0,    # 實質禁止跳過 Boss
    }.get(blind_type, 0.18)

    # 風險調整（v5.0: 早期更保守以累積 Joker）
    risk_adjustments = {
        1: 0.7, 2: 0.8,     # 早期：需要商店累積 Joker，更保守
        3: 0.9, 4: 1.0,     # 中期：可以適度 skip
        5: 0.85, 6: 0.7,    # 中後期：趨於保守
        7: 0.5, 8: 0.3,     # 後期：幾乎不該 skip
    }
    risk_adjustment = risk_adjustments.get(ante, 0.7)

    # Tag 價值（使用具體值或平均值）
    tag_value = get_tag_value(tag_id)

    reward = (tag_value * risk_adjustment) - opportunity_cost
    # v5.0: 上限低於 Clear 最小獎勵 0.25，確保 Clear 優於 Skip
    return clamp(reward, -0.20, 0.25)


def skip_blind_reward_v2(
    blind_type: int,
    ante: int,
    tag_id: Optional[int],
    shop_quality: float,
    joker_count: int,
    money: int,
) -> float:
    """
    狀態感知的 Skip Blind 獎勵（v10.0）

    設計原則：
    - 動態機會成本：商店品質高 + 有錢購買 = 機會成本高
    - 風險溢價基於 Joker 數量，而非僅 Ante
    - Context Multiplier 調整 Tag 價值

    公式：
    Skip_EV = E[Tag] × Context_Mult - Dynamic_Opportunity_Cost - Risk_Premium

    Args:
        blind_type: Blind 類型 (0=Small, 1=Big, 2=Boss)
        ante: 當前 Ante
        tag_id: 獲得的 Tag ID（可選）
        shop_quality: 商店品質分數 [0, 1]
        joker_count: 當前 Joker 數量
        money: 當前金錢

    Returns:
        獎勵值 (-0.25 ~ +0.30)
    """
    # 1. 基礎機會成本
    base_cost = {
        BLIND_SMALL: 0.18,
        BLIND_BIG: 0.25,
        BLIND_BOSS: 2.0,  # 禁止跳過 Boss
    }.get(blind_type, 0.18)

    # 2. Context Multiplier（商店邊際效用）
    # Joker 快滿 + 沒錢 = 商店價值低 = Skip 更有吸引力
    # Joker 少 + 有錢 = 商店價值高 = Skip 代價大
    if joker_count >= 4 and money < 4:
        context_mult = 1.3  # 商店對你沒什麼用
    elif joker_count <= 1:
        context_mult = 0.5  # 急需商店買 Joker
    elif joker_count <= 2 and money >= 5:
        context_mult = 0.7  # 還需要商店
    else:
        context_mult = 1.0  # 正常

    # 3. Dynamic Opportunity Cost
    # 商店品質高 + 能買得起 = 機會成本高
    can_afford = 1.0 if money >= 5 else 0.3
    shop_factor = shop_quality * (1 - joker_count / 5) * can_afford
    dynamic_cost = base_cost + shop_factor * 0.15

    # 4. Risk Premium（基於 Joker 數量而非 Ante）
    # Joker 少 = 高風險，不該跳過
    if joker_count <= 1:
        risk_premium = 0.25
    elif joker_count <= 2:
        risk_premium = 0.15
    elif joker_count <= 3:
        risk_premium = 0.08
    elif joker_count >= 5:
        risk_premium = 0.0  # 滿了，可以 Skip
    else:
        risk_premium = 0.05

    # 5. 階段風險調整（後期更保守）
    ante_factor = {
        1: 0.9, 2: 0.95,
        3: 1.0, 4: 1.0,
        5: 0.9, 6: 0.8,
        7: 0.6, 8: 0.4,
    }.get(ante, 0.8)

    # 6. Tag 價值
    tag_value = get_tag_value(tag_id)

    # 最終計算
    reward = (tag_value * context_mult * ante_factor) - dynamic_cost - risk_premium

    return clamp(reward, -0.25, 0.30)


def reroll_reward(
    reroll_cost: int,
    money_before: int,
    ante: int,
) -> float:
    """
    Reroll 懲罰

    - Reroll 本身是花錢行為，給予小懲罰
    - 考慮利息損失
    - 是否值得由後續購買決策體現
    """
    if money_before <= 0:
        return -0.1  # 沒錢還 reroll 是錯誤決策

    cost_ratio = min(reroll_cost / money_before, 1.0)

    # 利息損失懲罰
    money_after = money_before - reroll_cost
    interest_before = min(money_before // 5, 5)
    interest_after = min(money_after // 5, 5)
    interest_loss_penalty = 0.03 * (interest_before - interest_after) if interest_after < interest_before else 0.0

    # 階段權重（早期 reroll 更可接受）
    stage_mult = stage_weight_early(ante)

    # 基礎懲罰：reroll 花錢但不保證收益
    base_penalty = -0.02 - cost_ratio * 0.03

    return clamp(base_penalty * stage_mult - interest_loss_penalty, -0.15, 0.0)


def reroll_reward_v2(
    reroll_cost: int,
    money_before: int,
    ante: int,
    shop_quality: float,
    joker_count: int,
    joker_slot_limit: int,
    reroll_count_this_shop: int,
) -> float:
    """
    商店品質感知的 Reroll 獎勵（v10.0）

    設計原則：
    - 早期（Ante ≤ 3）且 Joker 數量不足時，Reroll 是正向投資
    - 商店品質低時鼓勵 Reroll，品質高時懲罰 Reroll
    - Reroll 預算追蹤：前 2 次正常，第 3 次開始遞減

    ROI 分析（見 super-analyst-pro 報告）：
    - 早期 Reroll 找 xMult Joker 的 ROI: +0.21 ~ +0.41
    - 當前系統純懲罰導致 0% Reroll 使用率

    Args:
        reroll_cost: Reroll 費用
        money_before: Reroll 前金錢
        ante: 當前 Ante
        shop_quality: 商店品質分數 [0, 1]
        joker_count: 當前 Joker 數量
        joker_slot_limit: Joker 槽位上限
        reroll_count_this_shop: 本次商店訪問的 Reroll 次數

    Returns:
        獎勵值 (-0.15 ~ +0.05)
    """
    if money_before <= 0:
        return -0.1  # 沒錢還 Reroll 是錯誤決策

    # === 基礎計算 ===
    cost_ratio = min(reroll_cost / money_before, 1.0)

    # 利息損失懲罰
    money_after = money_before - reroll_cost
    interest_before = min(money_before // 5, 5)
    interest_after = min(money_after // 5, 5)
    interest_loss_penalty = 0.03 * (interest_before - interest_after) if interest_after < interest_before else 0.0

    # === v10.0: 商店品質感知 ===

    # 1. 早期 + 低 Joker 數 = 探索模式
    is_exploration_phase = ante <= 3 and joker_count <= 2

    if is_exploration_phase:
        # 低品質商店 = 強烈鼓勵 Reroll
        # 高品質商店 = 輕微懲罰（已經夠好了）
        quality_factor = (0.5 - shop_quality) * 0.08  # [-0.04, +0.04]
        base_reward = 0.01 + quality_factor  # [-0.03, +0.05]
    else:
        # 後期：回歸原始邏輯，但考慮商店品質
        # 低品質商店 = 懲罰減少
        # 高品質商店 = 懲罰增加（不應該 Reroll 好商店）
        quality_penalty = (shop_quality - 0.5) * 0.04  # [-0.02, +0.02]
        base_reward = -0.02 - cost_ratio * 0.03 + quality_penalty

    # 2. Joker 飽和度調整
    saturation = joker_count / max(joker_slot_limit, 1)
    if saturation >= 0.8:
        # 快滿了，Reroll 價值降低
        base_reward -= 0.02

    # 3. Reroll 預算追蹤（防止 Reroll 成癮）
    if reroll_count_this_shop >= 3:
        # 第 3 次及以後，遞減獎勵
        over_budget_penalty = 0.03 * (reroll_count_this_shop - 2)
        base_reward -= over_budget_penalty
    if reroll_count_this_shop >= 5:
        # 嚴重懲罰：過度 Reroll
        base_reward -= 0.05

    # 4. 階段權重
    stage_mult = stage_weight_early(ante)
    final_reward = base_reward * stage_mult - interest_loss_penalty

    return clamp(final_reward, -0.15, 0.05)


def sell_joker_reward(
    money_gained: int,
    ante: int,
    joker_count_before: int,
    joker_slot_limit: int,
    joker_sold_id: int = -1
) -> float:
    """
    出售 Joker 獎勵（-0.5~0.1）（v6.0 - 嚴格控制賣出）

    核心原則（v6.0 重大修改）：
    - Joker 數量 <= 2 時賣出 = 重罰（這是致命錯誤）
    - 只有槽位真正滿且是弱 Joker 時才可能正向
    - 打破「賣 Joker 換錢」的套利迴路

    Args:
        money_gained: 賣出獲得的金幣
        ante: 當前 Ante
        joker_count_before: 賣出前的 Joker 數量
        joker_slot_limit: Joker 槽位上限
        joker_sold_id: 賣出的 Joker ID（用於精確評估損失）
    """
    # v6.0 核心修復：低 Joker 數量保護機制
    # 這是最關鍵的修復 — 嚴禁在 Joker 不足時賣出
    if joker_count_before <= 1:
        return -0.5  # 嚴重懲罰：只有 1 個還賣，幾乎是自殺行為
    if joker_count_before <= 2:
        return -0.3  # 重罰：只有 2 個還賣，高風險行為
    if joker_count_before <= 3 and ante <= 3:
        return -0.2  # 中等懲罰：早期 3 個也不該賣

    # 只有真正滿槽才有正向可能
    if joker_count_before >= joker_slot_limit:
        slot_bonus = 0.05  # 降低（原 0.08），滿槽時賣出勉強接受
    elif joker_count_before >= joker_slot_limit - 1:
        slot_bonus = 0.0   # 接近滿也不獎勵（原 0.03）
    else:
        slot_bonus = -0.08  # 未滿槽賣出 = 懲罰

    # 金幣收益價值（降低，打破套利動機）
    money_value = min(money_gained / 20.0, 0.05)  # 原 /12, max 0.1

    # 損失懲罰（提高）
    if joker_sold_id >= 0:
        estimated_cost = money_gained * 2
        rarity = estimate_joker_rarity_from_cost(estimated_cost)
        loss_penalty = JOKER_RARITY_VALUES.get(rarity, 0.08) * 1.5  # 提高 50%
    else:
        loss_penalty = 0.12  # 未知 Joker 懲罰更高（原 0.08）

    # 階段調整：早期賣出懲罰加重
    if ante <= 2:
        stage_mult = 1.5  # 早期懲罰加重 50%
    elif ante <= 4:
        stage_mult = 1.2  # 中期懲罰加重 20%
    else:
        stage_mult = 1.0  # 後期正常

    reward = (slot_bonus + money_value - loss_penalty) * stage_mult
    return clamp(reward, -0.5, 0.1)  # 範圍調整：下限 -0.5，上限 0.1


# 消耗品類型範圍（對應 Rust consumables.rs 的 to_global_index）
TAROT_COUNT = 22
PLANET_COUNT = 12
SPECTRAL_COUNT = 18


# ============================================================================
# v6.0 新增：Joker 持有與不足獎懲
# ============================================================================

def joker_holding_bonus(joker_count: int, ante: int) -> float:
    """
    持有 Joker 獎勵（v6.0 新增，在 CASH_OUT 時給予）

    核心原則：持有 Joker 是正確策略，需要直接獎勵
    - 0 個 Joker：懲罰
    - 每個 Joker：給予小獎勵
    - 早期持有更有價值

    Args:
        joker_count: 當前持有的 Joker 數量
        ante: 當前 Ante
    """
    if joker_count == 0:
        # 懲罰：0 個 Joker 進入商店是危險信號
        if ante <= 2:
            return -0.08  # 早期 0 Joker 更嚴重
        return -0.05

    # 每個 Joker 給予小獎勵
    base_bonus = joker_count * 0.015

    # 早期持有更有價值
    if ante <= 2:
        stage_mult = 1.5
    elif ante <= 4:
        stage_mult = 1.2
    else:
        stage_mult = 1.0

    return min(base_bonus * stage_mult, 0.08)


def shop_saving_reward(money: int, joker_count: int, ante: int, best_shop_joker_cost: int) -> float:
    """
    商店存錢獎勵（v6.7 新增）

    鼓勵在商店階段累積金幣，為購買 Joker 做準備。
    只有在 Joker 不足時才給予存錢獎勵。

    Args:
        money: 當前金幣
        joker_count: 當前 Joker 數量
        ante: 當前 Ante
        best_shop_joker_cost: 商店中最佳 Joker 的成本（0 = 無）

    Returns:
        存錢獎勵 (0.0 ~ 0.04)
    """
    # 如果 Joker 已經足夠，不需要存錢獎勵
    min_jokers = {1: 1, 2: 2, 3: 3, 4: 3, 5: 3, 6: 3, 7: 3, 8: 3}
    required = min_jokers.get(ante, 2)
    if joker_count >= required:
        return 0.0

    # 目標：存夠錢買 Joker
    # 如果知道商店最佳 Joker 成本，用它；否則假設 $6
    target_cost = best_shop_joker_cost if best_shop_joker_cost > 0 else 6

    # 接近目標金額的獎勵
    if money >= target_cost:
        # 已經夠錢買了！給予小獎勵鼓勵購買
        return 0.02
    elif money >= target_cost - 2:
        # 接近目標（差 $1-2）
        return 0.015
    elif money >= target_cost // 2:
        # 有一半了
        return 0.01
    else:
        # 還差很多
        return 0.005


def joker_shortage_penalty(joker_count: int, ante: int) -> float:
    """
    Joker 不足懲罰（v6.7 加重）

    持續施加壓力：Joker 數量低於 Ante 要求時給予懲罰
    這創造了持續的「購買 Joker」動機

    Args:
        joker_count: 當前持有的 Joker 數量
        ante: 當前 Ante
    """
    # v6.7: 提高最低要求（特別是早期）
    min_jokers = {1: 1, 2: 2, 3: 3, 4: 3, 5: 3, 6: 3, 7: 3, 8: 3}
    required = min_jokers.get(ante, 2)

    shortage = max(0, required - joker_count)
    if shortage == 0:
        return 0.0

    # v6.7: 加重懲罰：每缺 1 個 = -0.03（原 -0.01）
    return -0.03 * shortage


# ============================================================================
# v6.9 新增：Joker 貢獻與分數效率獎勵（純正向）
# ============================================================================

def joker_contribution_reward(info: StepInfo) -> float:
    """
    Joker 貢獻獎勵（v6.9 新增，純正向）

    當 Joker 對分數有顯著貢獻時給予獎勵，鼓勵學會利用 Joker 組合。

    設計原則：
    - 只有在成功出牌且得分時才給予獎勵
    - x_mult 權重最高（0.5），因為乘法加成是後期關鍵
    - 獎勵範圍 0.0 ~ 0.08（純正向，不懲罰低貢獻）

    Args:
        info: 當前步驟的狀態信息

    Returns:
        獎勵值 (0.0 ~ 0.08)
    """
    # 只有得分時才給獎勵
    if info.score_delta <= 0:
        return 0.0

    # 加權計算綜合貢獻分數
    # x_mult 權重最高，因為是後期勝率關鍵
    contribution_score = (
        0.2 * info.joker_chip_contrib +
        0.3 * info.joker_mult_contrib +
        0.5 * info.joker_xmult_contrib
    )

    # 基礎獎勵：貢獻分數 × 0.08
    base_reward = contribution_score * 0.08

    return base_reward  # 永遠 >= 0


def score_efficiency_reward(info: StepInfo) -> float:
    """
    分數效率獎勵（v6.9 新增，純正向）

    當單次出牌分數超過預期時給予獎勵。
    預期分數 = blind_target / 4（假設 4 次出牌過關）

    設計原則：
    - 只有超過預期（效率 > 1.0）才給獎勵
    - 不懲罰低於預期的出牌（已有 play_reward 處理）
    - 獎勵上限 0.06（避免過度主導）

    Args:
        info: 當前步驟的狀態信息

    Returns:
        獎勵值 (0.0 ~ 0.06)
    """
    # 無效情況不給獎勵
    if info.score_delta <= 0 or info.blind_target <= 0:
        return 0.0

    # score_efficiency 已由 Rust 計算：score_delta / (blind_target / 4)
    efficiency = info.score_efficiency

    # 只獎勵超過預期的情況（效率 > 1.0）
    if efficiency <= 1.0:
        return 0.0  # 不懲罰低於預期

    # 超額部分（最多 1.0）
    overkill = min(efficiency - 1.0, 1.0)

    # 基礎獎勵：超額比例 × 0.06
    base_reward = overkill * 0.06

    return base_reward  # 永遠 >= 0


# ============================================================================
# v7.0 新增：Boss 難度獎勵、Joker 協同獎勵、牌型針對性獎勵
# ============================================================================

def boss_clear_difficulty_bonus(boss_blind_id: int, plays_left: int) -> float:
    """
    Boss 過關難度獎勵（v7.0 新增）

    根據 Boss 難度給予額外獎勵，鼓勵 agent 學會應對困難 Boss。

    設計原則：
    - 只有過關時才給獎勵（blind_cleared == True）
    - 難度係數 > 1.0 的 Boss 才有額外獎勵
    - 效率加成：剩餘出牌次數越多，獎勵越高

    Args:
        boss_blind_id: Boss Blind ID (0-26), -1 = 無 Boss
        plays_left: 剩餘出牌次數

    Returns:
        獎勵值 (0.0 ~ 0.15)
    """
    if boss_blind_id < 0:
        return 0.0

    difficulty = BOSS_DIFFICULTY.get(boss_blind_id, 1.0)

    # 只有難度 > 0.8 才有基礎獎勵
    if difficulty <= 0.8:
        return 0.0

    # 基礎獎勵：(難度 - 0.8) * 0.2
    # Wall(1.4): 0.12, Needle(1.4): 0.12, Violet(1.5): 0.14
    base_bonus = (difficulty - 0.8) * 0.2

    # 效率加成：剩餘出牌次數 * 0.02
    # 鼓勵用更少的出牌機會過關
    efficiency_mult = 1.0 + plays_left * 0.05

    return clamp(base_bonus * efficiency_mult, 0.0, 0.15)


def _count_synergy_matches(joker_ids: set[int]) -> int:
    """
    計算 Joker 集合中的協同配對數量

    遍歷所有協同群組，計算有多少群組有 >= 2 個 Joker。
    """
    synergy_count = 0
    for group_name, group_jokers in JOKER_SYNERGY_GROUPS.items():
        matches = joker_ids & group_jokers
        if len(matches) >= 2:
            synergy_count += len(matches) - 1  # 每多一個匹配 +1
    return synergy_count


def _count_build_matching_jokers(hand_type: int, joker_ids: list[int]) -> int:
    """
    計算有多少 Joker 支援當前打出的牌型 build
    """
    if hand_type < 0:
        return 0

    # 確定牌型對應的 build
    hand_build = None
    for build, hands in _BUILD_HANDS.items():
        if hand_type in hands:
            hand_build = build
            break

    if hand_build is None:
        return 0

    # 計算支援該 build 的 Joker 數量
    matching_count = 0
    for jid in joker_ids:
        if JOKER_BUILD_SUPPORT.get(jid) == hand_build:
            matching_count += 1

    return matching_count


def joker_synergy_reward(
    joker_ids: list[int],
    dominant_build: Optional[int] = None,
    boss_blind_id: int = -1
) -> float:
    """
    Joker 協同獎勵（v7.0 新增）

    獎勵持有有協同效果的 Joker 組合，在 CASH_OUT 時評估。

    設計原則：
    - 協同群組配對：每個有 2+ 匹配的群組給予 +0.02
    - Build 對齊：Joker 能力匹配主導風格給予額外獎勵
    - Boss 克制：持有 Boss Killer 群組面對 Boss 給予獎勵

    Args:
        joker_ids: 當前持有的 Joker ID 列表
        dominant_build: 主導 build 風格（None = 未確定）
        boss_blind_id: 當前 Boss Blind ID（-1 = 無 Boss）

    Returns:
        獎勵值 (0.0 ~ 0.12)
    """
    if not joker_ids:
        return 0.0

    joker_set = set(joker_ids)
    reward = 0.0

    # 1. 協同群組獎勵
    synergy_count = _count_synergy_matches(joker_set)
    reward += synergy_count * 0.02

    # 2. Build 對齊獎勵
    if dominant_build is not None:
        aligned_count = sum(
            1 for jid in joker_ids
            if JOKER_BUILD_SUPPORT.get(jid) == dominant_build
        )
        if aligned_count >= 2:
            reward += 0.02 * (aligned_count - 1)

    # 3. Boss 克制獎勵
    # 持有 Chicot(68) 或 Matador(118) 面對 Boss 時給予獎勵
    if boss_blind_id >= 0:
        boss_killers = joker_set & JOKER_SYNERGY_GROUPS.get("boss_killer", set())
        if boss_killers:
            reward += 0.02 * len(boss_killers)

    return clamp(reward, 0.0, 0.12)


def hand_type_targeting_reward(
    hand_type: int,
    boss_blind_id: int,
    joker_ids: list[int]
) -> float:
    """
    牌型針對性獎勵（v7.0 新增）

    獎勵打出匹配 Joker 能力的牌型，鼓勵 agent 學會組合利用。

    設計原則：
    - 每個匹配 Joker 給予 +0.02
    - 特殊組合（如 SuperPosition + Straight Flush）給予額外獎勵
    - 不懲罰不匹配的情況

    Args:
        hand_type: 打出的牌型 ID
        boss_blind_id: 當前 Boss Blind ID（用於未來特殊獎勵）
        joker_ids: 當前持有的 Joker ID 列表

    Returns:
        獎勵值 (0.0 ~ 0.10)
    """
    if hand_type < 0 or not joker_ids:
        return 0.0

    # 計算匹配 Joker 數量
    matching_jokers = _count_build_matching_jokers(hand_type, joker_ids)

    # 基礎獎勵：每個匹配 Joker +0.02
    reward = matching_jokers * 0.02

    # 特殊組合獎勵：Straight Flush 或更高牌型配合相關 Joker
    if hand_type >= HAND_STRAIGHT_FLUSH:
        # 同時有 Straight 和 Flush 相關 Joker = 額外獎勵
        has_straight_joker = any(
            JOKER_BUILD_SUPPORT.get(jid) == BUILD_STRAIGHT
            for jid in joker_ids
        )
        has_flush_joker = any(
            JOKER_BUILD_SUPPORT.get(jid) == BUILD_FLUSH
            for jid in joker_ids
        )
        if has_straight_joker and has_flush_joker:
            reward += 0.04  # 特殊組合獎勵

    return clamp(reward, 0.0, 0.10)


def consumable_use_reward(ante: int, consumable_id: int = -1) -> float:
    """
    消耗品使用獎勵（0~0.25）

    根據消耗品類型給予不同獎勵：
    - Spectral (34-51): 0.18 基礎（效果最強，包含稀有轉換）
    - Planet (22-33): 0.12 基礎（穩定的牌型升級）
    - Tarot (0-21): 0.10 基礎（基礎卡片增強）

    Args:
        ante: 當前 Ante
        consumable_id: 消耗品全域 ID（-1 表示未知，使用平均值）
    """
    # 根據 consumable_id 確定類型和基礎獎勵
    if consumable_id < 0:
        # 未知類型，使用平均值
        base_value = 0.12
    elif consumable_id < TAROT_COUNT:
        # Tarot (0-21)
        base_value = 0.10
    elif consumable_id < TAROT_COUNT + PLANET_COUNT:
        # Planet (22-33)
        base_value = 0.12
    else:
        # Spectral (34-51)
        base_value = 0.18

    # 階段調整（消耗品後期價值更高）
    stage_mult = stage_weight_late(ante)

    return clamp(base_value * stage_mult, 0.0, 0.25)


def voucher_buy_reward(cost: int, money_before: int, ante: int) -> float:
    """
    Voucher 購買獎勵（-0.25~0.3）

    簡化版，使用平滑經濟懲罰曲線
    """
    # 平滑經濟懲罰（Voucher 略高於 Joker，因為 Voucher 成本固定 $10）
    cost_ratio = min(cost / money_before, 1.0) if money_before > 0 else 1.0
    economic_penalty = smooth_economic_penalty(cost_ratio) * 1.2  # Voucher 懲罰係數略高

    # 階段權重（早期更有價值）
    stage_mults = {1: 1.4, 2: 1.4, 3: 1.2, 4: 1.2, 5: 1.0, 6: 1.0, 7: 0.7, 8: 0.7}
    stage_mult = stage_mults.get(ante, 1.0)

    # 假設平均 Voucher 價值 0.15
    avg_voucher_value = 0.15

    reward = avg_voucher_value * stage_mult - economic_penalty
    return clamp(reward, -0.25, 0.3)


# ============================================================================
# 主要獎勵計算函數
# ============================================================================

class RewardCalculator:
    """獎勵計算器：根據遊戲狀態計算獎勵"""

    def __init__(self):
        self._prev_info: Optional[StepInfo] = None
        self._build_tracker = BuildTracker()
        # v6.4: 棄牌改善獎勵狀態追蹤
        self._prev_hand_type: int = -1  # 上一次的最佳牌型
        self._consecutive_discards: int = 0  # 連續棄牌計數器（反循環保護）
        # v6.4: 手牌潛力追蹤
        self._prev_flush_potential: float = 0.0
        self._prev_straight_potential: float = 0.0
        self._prev_pairs_potential: float = 0.0

    def reset(self):
        """重置內部狀態"""
        self._prev_info = None
        self._build_tracker.reset()
        # v6.4: 重置棄牌追蹤狀態
        self._prev_hand_type = -1
        self._consecutive_discards = 0
        self._prev_flush_potential = 0.0
        self._prev_straight_potential = 0.0
        self._prev_pairs_potential = 0.0

    def calculate(self, info_dict: dict) -> float:
        """
        根據當前和上一步狀態計算獎勵

        Args:
            info_dict: 從 gRPC EnvInfo 解析的字典

        Returns:
            計算的獎勵值
        """
        info = parse_env_info(info_dict)
        prev = self._prev_info

        reward = 0.0
        action_type = info.last_action_type

        # 遊戲結束
        if info.game_end != GAME_END_NONE:
            reward += game_end_reward(info.game_end, info.ante)

        # Blind 過關
        elif info.blind_cleared:
            reward += blind_clear_reward(
                info.plays_left,
                info.blind_type,
                info.ante
            )
            # v7.0: Boss 難度獎勵
            if info.blind_type == BLIND_BOSS:
                reward += boss_clear_difficulty_bonus(
                    info.boss_blind_id,
                    info.plays_left
                )
            # v6.7: Big Blind 過關後的 Boss 準備獎勵
            # 鼓勵在進入 Boss 前累積足夠金幣
            if info.blind_type == BLIND_BIG:
                if info.money >= 10:
                    reward += 0.08  # 有 $10+ 進入 Boss = 有機會買 Joker
                elif info.money >= 6:
                    reward += 0.04  # 有 $6+ = 勉強能買
                # $5 以下不給額外獎勵
            # v6.4: 過關後重置棄牌追蹤狀態
            self._consecutive_discards = 0
            self._prev_hand_type = -1
            self._prev_flush_potential = 0.0
            self._prev_straight_potential = 0.0
            self._prev_pairs_potential = 0.0

        # 根據動作類型計算獎勵
        elif action_type == ACTION_TYPE_PLAY:
            if info.score_delta > 0:
                # v6.4: 傳入 hand_type, ante, plays_left 以計算牌型+效率獎勵
                reward += play_reward(
                    info.score_delta,
                    info.blind_target,
                    hand_type=info.hand_type,
                    ante=info.ante,
                    plays_left=info.plays_left
                )
            if prev is not None:
                total_plays = max(1, prev.plays_left + 1)
                reward += blind_progress_signal(
                    prev.chips,
                    info.chips,
                    info.blind_target,
                    info.plays_left,
                    total_plays=total_plays,
                )
            if info.hand_type >= 0:
                self._build_tracker.record_hand(info.hand_type)

            # v6.9: Joker 貢獻獎勵和分數效率獎勵（純正向）
            reward += joker_contribution_reward(info)
            reward += score_efficiency_reward(info)

            # v7.0: 牌型針對性獎勵（基於 build 追蹤）
            # 使用 BuildTracker 的主導 build 來評估牌型匹配度
            dominant_build = self._build_tracker.get_dominant_build()
            if dominant_build is not None and info.hand_type >= 0:
                # 檢查牌型是否匹配主導 build
                if info.hand_type in _BUILD_HANDS.get(dominant_build, set()):
                    # 匹配主導 build = 獎勵
                    reward += 0.03
                    # 更強牌型 + 匹配 = 額外獎勵
                    if info.hand_type >= HAND_STRAIGHT_FLUSH:
                        reward += 0.02

            # v6.4: 出牌後重置棄牌追蹤狀態
            self._consecutive_discards = 0
            self._prev_hand_type = -1
            self._prev_flush_potential = 0.0
            self._prev_straight_potential = 0.0
            self._prev_pairs_potential = 0.0

        elif action_type == ACTION_TYPE_DISCARD:
            # 使用實際棄牌數量（來自 Rust 端）
            base_discard_penalty = discard_reward(info.cards_discarded, info.discards_left)
            reward += base_discard_penalty

            # v6.4: 棄牌改善獎勵（帶反循環保護）
            self._consecutive_discards += 1

            # 初始化追蹤狀態（首次棄牌時從前一步獲取）
            if self._prev_hand_type < 0 and prev is not None:
                self._prev_hand_type = prev.hand_type
                self._prev_flush_potential = prev.flush_potential
                self._prev_straight_potential = prev.straight_potential
                self._prev_pairs_potential = prev.pairs_potential

            if self._consecutive_discards <= 2:
                # 只獎勵前 2 次連續棄牌
                # 1. 牌型改善/變差獎勵
                setup_bonus = hand_setup_reward(
                    self._prev_hand_type,
                    info.hand_type,
                    had_discard=(info.cards_discarded > 0)
                )
                reward += setup_bonus

                # 2. v6.4: 潛力變化獎勵（Potential-Based Shaping）
                potential_bonus = potential_change_reward(
                    self._prev_flush_potential,
                    self._prev_straight_potential,
                    self._prev_pairs_potential,
                    info.flush_potential,
                    info.straight_potential,
                    info.pairs_potential
                )
                reward += potential_bonus
            else:
                # 第 3 次及之後連續棄牌：額外懲罰（防止棄牌循環）
                reward -= 0.02 * (self._consecutive_discards - 2)

            # 更新手牌追蹤狀態
            self._prev_hand_type = info.hand_type
            self._prev_flush_potential = info.flush_potential
            self._prev_straight_potential = info.straight_potential
            self._prev_pairs_potential = info.pairs_potential

        elif action_type == ACTION_TYPE_BUY_JOKER:
            # Fix: 檢查購買是否成功（joker_count 增加且 money 減少）
            # 防止失敗的購買嘗試不受懲罰（reward hacking 防護）
            if prev is not None:
                if info.joker_count > prev.joker_count and info.money_delta < 0:
                    reward += joker_buy_reward(
                        info.last_action_cost,
                        prev.money,
                        info.ante,
                        prev.joker_count,
                        info.joker_count,
                        info.joker_slot_limit
                    )
                elif info.joker_count <= prev.joker_count:
                    # 購買失敗：懲罰無效動作
                    reward -= 0.05

        elif action_type == ACTION_TYPE_SELL_JOKER:
            if prev is not None:
                # 直接使用 money_delta（Rust 端已計算）和 joker_sold_id
                reward += sell_joker_reward(
                    info.money_delta,
                    info.ante,
                    prev.joker_count,
                    info.joker_slot_limit,
                    info.joker_sold_id
                )

        elif action_type == ACTION_TYPE_REROLL:
            # v10.0: 使用商店品質感知的 Reroll 獎勵
            reward += reroll_reward_v2(
                info.last_action_cost,
                prev.money if prev else info.money,
                info.ante,
                getattr(info, 'shop_quality_score', 0.5),
                info.joker_count,
                info.joker_slot_limit,
                getattr(info, 'reroll_count_this_shop', 0)
            )

        elif action_type == ACTION_TYPE_SKIP_BLIND:
            # v10.0: 使用狀態感知的 Skip 獎勵
            tag_id_or_none = info.tag_id if info.tag_id >= 0 else None
            reward += skip_blind_reward_v2(
                info.blind_type,
                info.ante,
                tag_id_or_none,
                getattr(info, 'shop_quality_score', 0.5),
                info.joker_count,
                info.money
            )

        elif action_type == ACTION_TYPE_USE_CONSUMABLE:
            # 使用 consumable_id 精確計算獎勵（-1 表示未知，使用平均值）
            reward += consumable_use_reward(info.ante, info.consumable_id)

        elif action_type == ACTION_TYPE_BUY_VOUCHER:
            # Fix: 檢查購買是否成功（cost > 0 且 money 減少）
            # 防止失敗的購買嘗試獲得正向獎勵（reward hacking 防護）
            if prev is not None and info.last_action_cost > 0 and info.money_delta < 0:
                reward += voucher_buy_reward(
                    info.last_action_cost,
                    prev.money,
                    info.ante
                )
            elif info.last_action_cost == 0:
                # 購買失敗：懲罰無效動作
                reward -= 0.05

        elif action_type == ACTION_TYPE_BUY_PACK:
            # Fix: 檢查購買是否成功（cost > 0 且 money 減少）
            # 防止失敗的購買嘗試獲得正向獎勵
            if info.last_action_cost > 0 and info.money_delta < 0:
                reward += 0.05
            elif info.last_action_cost == 0:
                # 購買失敗：懲罰無效動作
                reward -= 0.03

        elif action_type == ACTION_TYPE_CASH_OUT:
            # Cash out 後給予金幣狀態獎勵
            reward += money_reward(info.money, info.ante)
            # v6.0: Joker 持有獎勵 — 鼓勵保留 Joker
            reward += joker_holding_bonus(info.joker_count, info.ante)
            # v7.0: Joker 協同獎勵（簡化版 - 基於數量和 build 對齊）
            # 當持有 3+ Joker 且有主導 build 時給予協同獎勵
            dominant_build = self._build_tracker.get_dominant_build()
            if info.joker_count >= 3 and dominant_build is not None:
                # 假設協同程度與 Joker 數量和主導 build 強度相關
                build_weights = self._build_tracker.get_build_weights()
                max_weight = max(build_weights.values())
                # 協同獎勵：Joker 數量 × build 權重
                synergy_bonus = 0.02 * min(info.joker_count - 2, 3) * max_weight
                reward += clamp(synergy_bonus, 0.0, 0.08)

        elif action_type == ACTION_TYPE_NEXT_ROUND:
            # 檢查是否進入新 Ante
            if prev is not None and info.ante > prev.ante:
                reward += ante_progress_reward(prev.ante, info.ante)

        # 通用 no-op 偵測：任何未產生狀態變化的動作施加小懲罰
        # 防止 agent 利用 SELECT/DISCARD/PLAY 的 no-op 漏洞
        if (action_type in (ACTION_TYPE_SELECT, ACTION_TYPE_DISCARD, ACTION_TYPE_PLAY)
                and info.score_delta == 0
                and info.money_delta == 0
                and info.cards_played == 0
                and info.cards_discarded == 0
                and not info.blind_cleared
                and info.game_end == GAME_END_NONE):
            reward -= 0.03

        # v6.0: Joker 不足懲罰 — 持續施加壓力
        # 只在商店階段檢查（避免戰鬥中干擾）
        if info.stage == STAGE_SHOP:
            reward += joker_shortage_penalty(info.joker_count, info.ante)
            # v6.7: 存錢獎勵 — 鼓勵累積金幣買 Joker
            reward += shop_saving_reward(
                info.money,
                info.joker_count,
                info.ante,
                info.best_shop_joker_cost
            )

        # v6.3: Boss 階段動作獎勵 — 鼓勵積極面對 Boss
        # 只有 PLAY 和 DISCARD 才給獎勵（SELECT 不算）
        if (info.stage == STAGE_BLIND
            and info.blind_type == BLIND_BOSS
            and action_type in (ACTION_TYPE_PLAY, ACTION_TYPE_DISCARD)):
            reward += 0.0001

        # 保存當前狀態作為下一步的參考
        self._prev_info = info

        return reward


# 創建全局計算器實例
_calculator = RewardCalculator()


def calculate_reward(info_dict: dict) -> float:
    """計算獎勵的便捷函數"""
    return _calculator.calculate(info_dict)


def reset_reward_calculator():
    """重置獎勵計算器狀態"""
    _calculator.reset()
