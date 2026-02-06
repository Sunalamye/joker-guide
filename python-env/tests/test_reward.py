"""
reward.py 單元測試

測試獎勵函數的正確性和邊界條件
"""

import pytest
import sys
from pathlib import Path

# 添加 src 到 path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from joker_env.reward import (
    play_reward,
    discard_reward,
    blind_clear_reward,
    joker_buy_reward,
    skip_blind_reward,
    reroll_reward,
    sell_joker_reward,
    money_reward,
    game_end_reward,
    ante_progress_reward,
    estimate_joker_value,
    get_tag_value,
    RewardCalculator,
    BLIND_SMALL, BLIND_BIG, BLIND_BOSS,
    GAME_END_WIN, GAME_END_LOSE, GAME_END_NONE,
    TAG_NEGATIVE, TAG_SPEED, TAG_ECONOMY,
    # v7.0 新增
    boss_clear_difficulty_bonus,
    joker_synergy_reward,
    hand_type_targeting_reward,
    BOSS_HOOK, BOSS_WALL, BOSS_NEEDLE, BOSS_VIOLET, BOSS_FLINT,
    BOSS_DIFFICULTY,
    JOKER_SYNERGY_GROUPS,
    BUILD_PAIRS, BUILD_STRAIGHT, BUILD_FLUSH,
    HAND_PAIR, HAND_FLUSH, HAND_STRAIGHT, HAND_STRAIGHT_FLUSH,
    # 未測試函數
    consumable_use_reward,
    voucher_buy_reward,
    joker_holding_bonus,
    TAROT_COUNT, PLANET_COUNT,
)


class TestPlayReward:
    """出牌獎勵測試"""

    def test_zero_score(self):
        """零分給予懲罰（v5.1: 0 分是嚴重問題）"""
        assert play_reward(0, 1000) == -0.03

    def test_zero_required(self):
        """目標為零返回基礎出牌獎勵"""
        assert play_reward(100, 0) == 0.02

    def test_partial_progress(self):
        """部分進度給予比例獎勵"""
        reward = play_reward(500, 1000)
        assert 0.05 < reward < 0.12  # 50% 進度 + base + hand_type_bonus

    def test_exact_target(self):
        """剛好達標給予基礎獎勵"""
        reward = play_reward(1000, 1000)
        assert 0.13 < reward < 0.22

    def test_overkill_bonus(self):
        """超額有額外獎勵"""
        exact_reward = play_reward(1000, 1000)
        overkill_reward = play_reward(2000, 1000)
        assert overkill_reward > exact_reward

    def test_max_cap(self):
        """獎勵不超過上限 0.3"""
        reward = play_reward(10000, 1000)
        assert reward <= 0.3


class TestDiscardReward:
    """棄牌獎勵測試"""

    def test_zero_cards(self):
        """空棄牌（no-op）給予懲罰（v5.1 防護）"""
        assert discard_reward(0, 3) == -0.05

    def test_any_discard_flat_penalty(self):
        """有棄牌時統一懲罰 -0.02（v5.1 簡化設計）"""
        assert discard_reward(2, 3) == -0.02
        assert discard_reward(5, 3) == -0.02

    def test_discard_penalty_regardless_of_remaining(self):
        """棄牌懲罰不受剩餘次數影響（v5.1 扁平懲罰）"""
        assert discard_reward(2, 2) == -0.02
        assert discard_reward(2, 0) == -0.02

    def test_max_cap(self):
        """獎勵不超過上限 0.05"""
        reward = discard_reward(1, 0)
        assert reward <= 0.05


class TestBlindClearReward:
    """過關獎勵測試"""

    def test_small_blind(self):
        """Small Blind 基礎獎勵（v6.3: base=0.20）"""
        reward = blind_clear_reward(0, BLIND_SMALL, 1)
        assert 0.20 <= reward <= 0.25

    def test_boss_blind_higher(self):
        """Boss Blind 獎勵更高"""
        small = blind_clear_reward(0, BLIND_SMALL, 1)
        boss = blind_clear_reward(0, BLIND_BOSS, 1)
        assert boss > small

    def test_efficiency_bonus(self):
        """剩餘出牌次數有效率獎勵"""
        no_plays = blind_clear_reward(0, BLIND_SMALL, 1)
        with_plays = blind_clear_reward(3, BLIND_SMALL, 1)
        assert with_plays > no_plays

    def test_late_game_scaling(self):
        """後期過關獎勵稍高"""
        early = blind_clear_reward(0, BLIND_SMALL, 1)
        late = blind_clear_reward(0, BLIND_SMALL, 8)
        assert late >= early

    def test_range(self):
        """獎勵範圍 0.20~1.50（v6.3: Boss 大幅提升 + Ante 倍率）"""
        reward = blind_clear_reward(4, BLIND_BOSS, 8, boss_blind_id=1)
        assert 0.20 <= reward <= 1.50


class TestJokerBuyReward:
    """購買 Joker 獎勵測試"""

    def test_no_purchase(self):
        """購買失敗不給獎勵"""
        assert joker_buy_reward(5, 10, 1, 2, 2, 5) == 0.0

    def test_successful_purchase(self):
        """成功購買給予獎勵"""
        reward = joker_buy_reward(5, 20, 1, 2, 3, 5)
        assert reward != 0.0

    def test_high_cost_ratio_penalty(self):
        """後期高成本占比有經濟懲罰（v6.8: 早期無經濟懲罰）"""
        # 早期 Ante <= 3 完全無經濟懲罰，所以用 Ante 5 測試
        rich = joker_buy_reward(5, 50, 5, 2, 3, 5)  # 10% 資金
        poor = joker_buy_reward(5, 10, 5, 2, 3, 5)  # 50% 資金
        assert rich > poor

    def test_early_game_bonus(self):
        """早期購買更有價值"""
        early = joker_buy_reward(5, 20, 1, 2, 3, 5)
        late = joker_buy_reward(5, 20, 8, 2, 3, 5)
        assert early > late

    def test_slot_pressure(self):
        """接近滿槽有懲罰"""
        with_space = joker_buy_reward(5, 20, 1, 2, 3, 5)
        almost_full = joker_buy_reward(5, 20, 1, 4, 5, 5)
        assert with_space > almost_full

    def test_range(self):
        """獎勵範圍 -0.3~0.5（v6.0: 上限提高到 0.5）"""
        reward = joker_buy_reward(5, 10, 1, 0, 1, 5)
        assert -0.3 <= reward <= 0.5


class TestSkipBlindReward:
    """跳過 Blind 獎勵測試"""

    def test_small_blind_skip(self):
        """跳過 Small Blind（v5.0 調整後更保守）"""
        reward = skip_blind_reward(BLIND_SMALL, 1)
        assert reward > -0.20

    def test_late_game_penalty(self):
        """後期跳過風險更高"""
        early = skip_blind_reward(BLIND_SMALL, 1)
        late = skip_blind_reward(BLIND_SMALL, 8)
        assert early > late

    def test_tag_value_affects_reward(self):
        """不同 Tag 價值影響獎勵"""
        negative_tag = skip_blind_reward(BLIND_SMALL, 1, tag_id=TAG_NEGATIVE)
        speed_tag = skip_blind_reward(BLIND_SMALL, 1, tag_id=TAG_SPEED)
        assert negative_tag > speed_tag

    def test_range(self):
        """獎勵範圍 -0.20~0.25（v5.0 調整）"""
        reward = skip_blind_reward(BLIND_SMALL, 1, tag_id=TAG_NEGATIVE)
        assert -0.20 <= reward <= 0.25


class TestRerollReward:
    """Reroll 獎勵測試"""

    def test_always_negative(self):
        """Reroll 總是懲罰（純經濟成本）"""
        reward = reroll_reward(2, 20, 1)
        assert reward <= 0.0

    def test_no_money_penalty(self):
        """沒錢還 reroll 懲罰更重"""
        with_money = reroll_reward(2, 20, 1)
        no_money = reroll_reward(2, 0, 1)
        assert no_money < with_money

    def test_interest_loss_penalty(self):
        """跨越利息閾值有額外懲罰"""
        no_loss = reroll_reward(2, 30, 1)  # 30 -> 28, 仍 $25+
        with_loss = reroll_reward(2, 26, 1)  # 26 -> 24, 失去 $5 利息
        assert no_loss > with_loss

    def test_range(self):
        """獎勵範圍 -0.15~0.0"""
        reward = reroll_reward(5, 10, 1)
        assert -0.15 <= reward <= 0.0


class TestSellJokerReward:
    """出售 Joker 獎勵測試"""

    def test_slot_pressure_bonus(self):
        """滿槽時出售有獎勵"""
        not_full = sell_joker_reward(3, 1, 3, 5)
        full = sell_joker_reward(3, 1, 5, 5)
        assert full > not_full

    def test_late_game_scaling(self):
        """後期出售更可接受"""
        early = sell_joker_reward(3, 1, 3, 5)
        late = sell_joker_reward(3, 8, 3, 5)
        assert late >= early

    def test_range(self):
        """獎勵範圍 -0.2~0.2"""
        reward = sell_joker_reward(5, 5, 5, 5)
        assert -0.2 <= reward <= 0.2


class TestMoneyReward:
    """金幣獎勵測試"""

    def test_interest_tier_bonus(self):
        """達到利息閾值有獎勵"""
        low = money_reward(3, 1)
        high = money_reward(25, 1)
        assert high > low

    def test_early_game_emphasis(self):
        """早期金幣更重要"""
        early = money_reward(15, 1)
        late = money_reward(15, 8)
        assert early > late

    def test_range(self):
        """獎勵範圍 0~0.2"""
        reward = money_reward(50, 1)
        assert 0.0 <= reward <= 0.2


class TestGameEndReward:
    """遊戲結束獎勵測試"""

    def test_win_reward(self):
        """勝利給予最大獎勵（v4.0: 提升到 5.0）"""
        assert game_end_reward(GAME_END_WIN, 8) == 5.0

    def test_lose_penalty(self):
        """失敗給予懲罰"""
        reward = game_end_reward(GAME_END_LOSE, 1)
        assert reward < 0

    def test_late_lose_less_penalty(self):
        """後期失敗懲罰較輕"""
        early_lose = game_end_reward(GAME_END_LOSE, 1)
        late_lose = game_end_reward(GAME_END_LOSE, 7)
        assert late_lose > early_lose

    def test_none_no_reward(self):
        """遊戲未結束不給獎勵"""
        assert game_end_reward(GAME_END_NONE, 5) == 0.0


class TestAnteProgressReward:
    """Ante 進度獎勵測試"""

    def test_no_progress(self):
        """同 Ante 無獎勵"""
        assert ante_progress_reward(3, 3) == 0.0

    def test_late_game_higher(self):
        """後期進度更有價值"""
        early = ante_progress_reward(1, 2)
        late = ante_progress_reward(7, 8)
        assert late > early

    def test_range(self):
        """獎勵範圍（v5.2: 更陡峭曲線 + 里程碑獎勵）"""
        reward = ante_progress_reward(1, 8)
        # v5.2: 1→8 = 二次曲線 + 里程碑(0.3+0.5+0.8) ≈ 9.72
        assert reward <= 10.0


class TestJokerValueEstimation:
    """Joker 價值估算測試"""

    def test_cost_based_rarity(self):
        """成本越高，估值越高"""
        cheap = estimate_joker_value(3)
        expensive = estimate_joker_value(9)
        assert expensive > cheap

    def test_value_range(self):
        """估值在合理範圍"""
        for cost in [2, 5, 8, 10]:
            value = estimate_joker_value(cost)
            assert 0.05 <= value <= 0.50


class TestTagValueMapping:
    """Tag 價值映射測試"""

    def test_negative_tag_highest(self):
        """Negative Tag 價值最高"""
        negative = get_tag_value(TAG_NEGATIVE)
        economy = get_tag_value(TAG_ECONOMY)
        assert negative > economy

    def test_speed_tag_lowest(self):
        """Speed Tag 價值較低"""
        speed = get_tag_value(TAG_SPEED)
        economy = get_tag_value(TAG_ECONOMY)
        assert speed < economy

    def test_unknown_returns_average(self):
        """未知 Tag 返回平均值"""
        avg = get_tag_value(None)
        assert 0.15 < avg < 0.25


class TestBlindProgressSignal:
    """Blind 內進度信號測試"""

    def test_no_reward_for_zero_target(self):
        """目標為零不給獎勵"""
        from joker_env.reward import blind_progress_signal
        reward = blind_progress_signal(0, 500, 0, 3)
        assert reward == 0.0

    def test_no_reward_for_no_progress(self):
        """無進度不給獎勵"""
        from joker_env.reward import blind_progress_signal
        reward = blind_progress_signal(500, 500, 1000, 3)
        assert reward == 0.0

    def test_progress_reward(self):
        """有進度給獎勵"""
        from joker_env.reward import blind_progress_signal
        # 從 0 到 500，目標 1000，進度 50%
        reward = blind_progress_signal(0, 500, 1000, 3)
        assert reward > 0

    def test_milestone_bonus(self):
        """達到 80% 里程碑有獎勵"""
        from joker_env.reward import blind_progress_signal
        # 從 70% 到 85%
        below_milestone = blind_progress_signal(700, 750, 1000, 2)
        cross_milestone = blind_progress_signal(750, 850, 1000, 2)
        assert cross_milestone > below_milestone

    def test_pace_bonus(self):
        """節奏快有額外獎勵"""
        from joker_env.reward import blind_progress_signal
        # 第一次出牌就達到 50%（快於預期）
        fast_pace = blind_progress_signal(0, 500, 1000, 3, total_plays=4)
        assert fast_pace > 0

    def test_max_cap(self):
        """獎勵不超過 0.05"""
        from joker_env.reward import blind_progress_signal
        # 大幅進度
        reward = blind_progress_signal(0, 1000, 1000, 3)
        assert reward <= 0.05


class TestHandSetupReward:
    """Hand setup 獎勵測試"""

    def test_no_reward_without_discard(self):
        """沒有棄牌不給獎勵"""
        from joker_env.reward import hand_setup_reward, HAND_PAIR, HAND_FLUSH
        reward = hand_setup_reward(HAND_PAIR, HAND_FLUSH, had_discard=False)
        assert reward == 0.0

    def test_no_reward_without_prev_hand(self):
        """沒有上一手牌不給獎勵"""
        from joker_env.reward import hand_setup_reward, HAND_FLUSH
        reward = hand_setup_reward(-1, HAND_FLUSH, had_discard=True)
        assert reward == 0.0

    def test_downgrade_penalty(self):
        """牌型變差給予輕微懲罰（v6.4 新增）"""
        from joker_env.reward import hand_setup_reward, HAND_FLUSH, HAND_PAIR
        reward = hand_setup_reward(HAND_FLUSH, HAND_PAIR, had_discard=True)
        assert reward == -0.01

    def test_small_improvement_reward(self):
        """小改善（1-2 級）給 0.02"""
        from joker_env.reward import hand_setup_reward, HAND_PAIR, HAND_TWO_PAIR
        reward = hand_setup_reward(HAND_PAIR, HAND_TWO_PAIR, had_discard=True)
        assert reward == 0.02

    def test_medium_improvement_reward(self):
        """中改善（3-4 級）給 0.04"""
        from joker_env.reward import hand_setup_reward, HAND_PAIR, HAND_FLUSH
        reward = hand_setup_reward(HAND_PAIR, HAND_FLUSH, had_discard=True)
        assert reward == 0.04

    def test_large_improvement_reward(self):
        """大改善（5+ 級）給更多"""
        from joker_env.reward import hand_setup_reward, HAND_HIGH_CARD, HAND_FULL_HOUSE
        reward = hand_setup_reward(HAND_HIGH_CARD, HAND_FULL_HOUSE, had_discard=True)
        assert 0.06 <= reward <= 0.08

    def test_max_cap(self):
        """獎勵不超過 0.08"""
        from joker_env.reward import hand_setup_reward, HAND_HIGH_CARD, HAND_FLUSH_FIVE
        reward = hand_setup_reward(HAND_HIGH_CARD, HAND_FLUSH_FIVE, had_discard=True)
        assert reward <= 0.08


class TestBuildTracker:
    """Build 追蹤器測試"""

    def test_initial_weights_uniform(self):
        """初始權重均勻分布"""
        from joker_env.reward import BuildTracker
        tracker = BuildTracker()
        weights = tracker.get_build_weights()
        assert weights["pairs"] == pytest.approx(0.33, abs=0.01)
        assert weights["straight"] == pytest.approx(0.33, abs=0.01)
        assert weights["flush"] == pytest.approx(0.33, abs=0.01)

    def test_record_pair_hands(self):
        """記錄 pair-based 牌型"""
        from joker_env.reward import BuildTracker, HAND_PAIR, HAND_TWO_PAIR, HAND_THREE_KIND
        tracker = BuildTracker()
        for _ in range(5):
            tracker.record_hand(HAND_PAIR)
        tracker.record_hand(HAND_TWO_PAIR)
        tracker.record_hand(HAND_THREE_KIND)

        weights = tracker.get_build_weights()
        assert weights["pairs"] > weights["straight"]
        assert weights["pairs"] > weights["flush"]

    def test_dominant_build_detection(self):
        """檢測主導 Build"""
        from joker_env.reward import BuildTracker, HAND_FLUSH, BUILD_FLUSH
        tracker = BuildTracker()
        for _ in range(6):
            tracker.record_hand(HAND_FLUSH)

        assert tracker.get_dominant_build() == BUILD_FLUSH

    def test_no_dominant_with_few_hands(self):
        """樣本太少時無主導 Build"""
        from joker_env.reward import BuildTracker, HAND_FLUSH
        tracker = BuildTracker()
        tracker.record_hand(HAND_FLUSH)
        tracker.record_hand(HAND_FLUSH)

        assert tracker.get_dominant_build() is None

    def test_joker_build_bonus_matching(self):
        """Joker 匹配 Build 有獎勵"""
        from joker_env.reward import BuildTracker, HAND_FLUSH
        tracker = BuildTracker()
        # 建立 Flush 主導 build
        for _ in range(6):
            tracker.record_hand(HAND_FLUSH)

        # DrollJoker (id=9) 支持 Flush
        bonus = tracker.joker_build_bonus(9, 5)
        assert bonus > 0

    def test_joker_build_bonus_mismatching(self):
        """Joker 不匹配 Build 有懲罰"""
        from joker_env.reward import BuildTracker, HAND_FLUSH
        tracker = BuildTracker()
        # 建立 Flush 主導 build
        for _ in range(6):
            tracker.record_hand(HAND_FLUSH)

        # JollyJoker (id=5) 支持 Pairs，不支持 Flush
        penalty = tracker.joker_build_bonus(5, 4)
        assert penalty < 0

    def test_joker_build_bonus_generic(self):
        """不在 JOKER_BUILD_SUPPORT 中的 Joker 無獎勵也無懲罰"""
        from joker_env.reward import BuildTracker, HAND_FLUSH
        tracker = BuildTracker()
        for _ in range(6):
            tracker.record_hand(HAND_FLUSH)

        # id=99 不在 JOKER_BUILD_SUPPORT 中，應返回 0.0
        bonus = tracker.joker_build_bonus(99, 5)
        assert bonus == 0.0

    def test_reset_clears_tracker(self):
        """重置清除追蹤數據"""
        from joker_env.reward import BuildTracker, HAND_PAIR
        tracker = BuildTracker()
        for _ in range(5):
            tracker.record_hand(HAND_PAIR)
        tracker.reset()

        assert tracker._total_hands == 0
        weights = tracker.get_build_weights()
        assert weights["pairs"] == pytest.approx(0.33, abs=0.01)


class TestRewardCalculator:
    """獎勵計算器整合測試"""

    def test_reset_clears_state(self):
        """重置清除內部狀態"""
        calc = RewardCalculator()
        calc.calculate({"episode_step": 1, "ante": 1})
        calc.reset()
        assert calc._prev_info is None

    def test_sequential_calls(self):
        """連續調用追蹤狀態"""
        calc = RewardCalculator()
        calc.calculate({"episode_step": 1, "ante": 1, "money": 10})
        calc.calculate({"episode_step": 2, "ante": 1, "money": 15})
        assert calc._prev_info is not None
        assert calc._prev_info.money == 15


# ============================================================================
# v7.0: Boss 難度、Joker 協同、牌型針對性獎勵測試
# ============================================================================

class TestBossClearDifficultyBonus:
    """Boss 過關難度獎勵測試（v7.0）"""

    def test_no_boss_no_bonus(self):
        """無 Boss 時不給獎勵"""
        assert boss_clear_difficulty_bonus(-1, 3) == 0.0

    def test_easy_boss_no_bonus(self):
        """簡單 Boss（難度 <= 0.8）不給額外獎勵"""
        # BOSS_HOOK 難度 0.8
        assert boss_clear_difficulty_bonus(BOSS_HOOK, 0) == 0.0

    def test_hard_boss_gives_bonus(self):
        """困難 Boss 給予獎勵"""
        # BOSS_WALL 難度 1.4
        reward = boss_clear_difficulty_bonus(BOSS_WALL, 0)
        assert reward > 0.0

    def test_needle_boss_high_bonus(self):
        """Needle Boss（難度 1.4）給予較高獎勵"""
        reward = boss_clear_difficulty_bonus(BOSS_NEEDLE, 0)
        assert reward > 0.1

    def test_violet_boss_highest_bonus(self):
        """Violet Boss（難度 1.5）給予最高獎勵"""
        reward = boss_clear_difficulty_bonus(BOSS_VIOLET, 0)
        assert reward >= boss_clear_difficulty_bonus(BOSS_WALL, 0)

    def test_efficiency_bonus(self):
        """剩餘出牌次數提高獎勵"""
        no_plays_left = boss_clear_difficulty_bonus(BOSS_WALL, 0)
        with_plays_left = boss_clear_difficulty_bonus(BOSS_WALL, 3)
        assert with_plays_left > no_plays_left

    def test_range(self):
        """獎勵範圍 0.0 ~ 0.15"""
        # 最高難度 + 最多剩餘出牌
        max_reward = boss_clear_difficulty_bonus(BOSS_VIOLET, 4)
        assert 0.0 <= max_reward <= 0.15


class TestJokerSynergyReward:
    """Joker 協同獎勵測試（v7.0）"""

    def test_empty_jokers_no_reward(self):
        """無 Joker 不給獎勵"""
        assert joker_synergy_reward([], None) == 0.0

    def test_single_joker_no_synergy(self):
        """單個 Joker 無協同獎勵"""
        reward = joker_synergy_reward([5], None)  # JollyJoker
        assert reward == 0.0

    def test_pair_synergy_group(self):
        """Pair 協同群組有獎勵"""
        # pair_power 群組：5, 10, 111, 6, 11, 112, 113
        reward = joker_synergy_reward([5, 10, 111], None)
        assert reward > 0.0

    def test_build_alignment_bonus(self):
        """Build 對齊有額外獎勵"""
        # 有主導 build 時，匹配的 Joker 獲得額外獎勵
        reward_with_build = joker_synergy_reward([5, 10], BUILD_PAIRS)
        reward_without_build = joker_synergy_reward([5, 10], None)
        assert reward_with_build >= reward_without_build

    def test_boss_killer_bonus(self):
        """Boss Killer Joker 面對 Boss 有獎勵"""
        # Chicot=68, Matador=118
        reward_with_boss = joker_synergy_reward([68], None, boss_blind_id=BOSS_WALL)
        reward_without_boss = joker_synergy_reward([68], None, boss_blind_id=-1)
        assert reward_with_boss > reward_without_boss

    def test_range(self):
        """獎勵範圍 0.0 ~ 0.12"""
        # 最大可能協同
        max_reward = joker_synergy_reward([5, 10, 111, 6, 11], BUILD_PAIRS, BOSS_WALL)
        assert 0.0 <= max_reward <= 0.12


class TestHandTypeTargetingReward:
    """牌型針對性獎勵測試（v7.0）"""

    def test_no_hand_type_no_reward(self):
        """無牌型不給獎勵"""
        assert hand_type_targeting_reward(-1, -1, [5, 10]) == 0.0

    def test_no_jokers_no_reward(self):
        """無 Joker 不給獎勵"""
        assert hand_type_targeting_reward(HAND_PAIR, -1, []) == 0.0

    def test_matching_joker_gives_reward(self):
        """匹配的 Joker 給予獎勵"""
        # JollyJoker(5) 支持 PAIRS，打出 PAIR 應該有獎勵
        reward = hand_type_targeting_reward(HAND_PAIR, -1, [5])
        assert reward > 0.0

    def test_multiple_matching_jokers(self):
        """多個匹配 Joker 獎勵更高"""
        single = hand_type_targeting_reward(HAND_PAIR, -1, [5])
        multiple = hand_type_targeting_reward(HAND_PAIR, -1, [5, 10])  # 兩個 pair Joker
        assert multiple > single

    def test_straight_flush_special_bonus(self):
        """Straight Flush 配合相關 Joker 有特殊獎勵"""
        # 同時有 straight 和 flush Joker
        reward = hand_type_targeting_reward(
            HAND_STRAIGHT_FLUSH, -1, [8, 9]  # CrazyJoker(straight) + DrollJoker(flush)
        )
        assert reward > 0.0

    def test_range(self):
        """獎勵範圍 0.0 ~ 0.10"""
        # 最大可能匹配
        max_reward = hand_type_targeting_reward(HAND_STRAIGHT_FLUSH, -1, [8, 13, 9, 14])
        assert 0.0 <= max_reward <= 0.10


class TestBossDifficultyConstants:
    """Boss 難度常量測試（v7.0）"""

    def test_all_bosses_have_difficulty(self):
        """所有 Boss 都有難度係數"""
        for boss_id in range(27):
            assert boss_id in BOSS_DIFFICULTY

    def test_difficulty_range(self):
        """難度係數在合理範圍"""
        for boss_id, diff in BOSS_DIFFICULTY.items():
            assert 0.5 <= diff <= 2.0, f"Boss {boss_id} difficulty {diff} out of range"

    def test_showdown_bosses_harder(self):
        """Showdown Boss 難度較高"""
        showdown_ids = [22, 23, 24, 25, 26]  # BOSS_VIOLET to BOSS_VERDANT
        for boss_id in showdown_ids:
            assert BOSS_DIFFICULTY[boss_id] >= 1.1


class TestJokerSynergyGroups:
    """Joker 協同群組常量測試（v7.0）"""

    def test_all_groups_have_members(self):
        """所有群組都有成員"""
        for group_name, members in JOKER_SYNERGY_GROUPS.items():
            assert len(members) >= 2, f"Group {group_name} has less than 2 members"

    def test_pair_power_group(self):
        """Pair Power 群組包含正確的 Joker"""
        pair_group = JOKER_SYNERGY_GROUPS.get("pair_power", set())
        assert 5 in pair_group   # JollyJoker
        assert 111 in pair_group  # The_Duo

    def test_boss_killer_group(self):
        """Boss Killer 群組包含正確的 Joker"""
        boss_killer = JOKER_SYNERGY_GROUPS.get("boss_killer", set())
        assert 68 in boss_killer  # Chicot
        assert 118 in boss_killer  # Matador


# ============================================================================
# 新增測試：consumable_use_reward, voucher_buy_reward, joker_holding_bonus
# ============================================================================

class TestConsumableUseReward:
    """消耗品使用獎勵測試"""

    def test_unknown_consumable_uses_average(self):
        """未知消耗品使用平均基礎值 0.12"""
        reward = consumable_use_reward(ante=3, consumable_id=-1)
        assert 0.10 <= reward <= 0.15

    def test_tarot_base_value(self):
        """Tarot (id 0-21) 基礎值 0.10"""
        reward = consumable_use_reward(ante=3, consumable_id=10)
        assert 0.08 <= reward <= 0.12

    def test_planet_base_value(self):
        """Planet (id 22-33) 基礎值 0.12"""
        reward = consumable_use_reward(ante=3, consumable_id=TAROT_COUNT + 5)
        assert 0.10 <= reward <= 0.15

    def test_spectral_highest_value(self):
        """Spectral (id 34+) 基礎值最高 0.18"""
        reward = consumable_use_reward(ante=3, consumable_id=TAROT_COUNT + PLANET_COUNT + 1)
        assert reward > consumable_use_reward(ante=3, consumable_id=10)

    def test_late_game_scaling(self):
        """後期消耗品更有價值（stage_weight_late）"""
        early = consumable_use_reward(ante=1, consumable_id=10)
        late = consumable_use_reward(ante=8, consumable_id=10)
        assert late > early

    def test_range(self):
        """獎勵範圍 0.0 ~ 0.25"""
        for cid in [-1, 0, 10, 21, 22, 33, 34, 51]:
            for ante in [1, 4, 8]:
                reward = consumable_use_reward(ante=ante, consumable_id=cid)
                assert 0.0 <= reward <= 0.25, f"Out of range for cid={cid}, ante={ante}: {reward}"


class TestVoucherBuyReward:
    """Voucher 購買獎勵測試"""

    def test_early_game_higher_value(self):
        """早期購買 Voucher 更有價值"""
        early = voucher_buy_reward(cost=10, money_before=20, ante=1)
        late = voucher_buy_reward(cost=10, money_before=20, ante=8)
        assert early > late

    def test_high_cost_ratio_penalty(self):
        """高成本占比有經濟懲罰"""
        rich = voucher_buy_reward(cost=10, money_before=50, ante=3)
        poor = voucher_buy_reward(cost=10, money_before=11, ante=3)
        assert rich > poor

    def test_zero_money_lowest_reward(self):
        """零資金時獎勵最低（經濟懲罰最大）"""
        zero = voucher_buy_reward(cost=10, money_before=0, ante=3)
        rich = voucher_buy_reward(cost=10, money_before=50, ante=3)
        assert zero < rich

    def test_affordable_purchase_positive(self):
        """資金充裕時購買為正向獎勵"""
        reward = voucher_buy_reward(cost=10, money_before=50, ante=1)
        assert reward > 0

    def test_range(self):
        """獎勵範圍 -0.25 ~ 0.3"""
        for cost in [5, 10, 15]:
            for money in [0, 5, 10, 20, 50]:
                for ante in [1, 4, 8]:
                    reward = voucher_buy_reward(cost=cost, money_before=money, ante=ante)
                    assert -0.25 <= reward <= 0.3, \
                        f"Out of range: cost={cost}, money={money}, ante={ante}: {reward}"


class TestJokerHoldingBonus:
    """Joker 持有獎勵測試"""

    def test_zero_jokers_penalty(self):
        """0 個 Joker 應有懲罰"""
        reward = joker_holding_bonus(0, ante=1)
        assert reward < 0

    def test_zero_jokers_early_worse(self):
        """早期 0 Joker 懲罰更重"""
        early = joker_holding_bonus(0, ante=1)
        late = joker_holding_bonus(0, ante=5)
        assert early < late  # 早期懲罰更重 = 值更負

    def test_holding_jokers_positive(self):
        """持有 Joker 給正向獎勵"""
        reward = joker_holding_bonus(3, ante=3)
        assert reward > 0

    def test_more_jokers_more_reward(self):
        """更多 Joker 獎勵更高"""
        few = joker_holding_bonus(1, ante=3)
        many = joker_holding_bonus(4, ante=3)
        assert many > few

    def test_early_game_multiplier(self):
        """早期持有倍率更高"""
        early = joker_holding_bonus(3, ante=1)
        late = joker_holding_bonus(3, ante=8)
        assert early > late

    def test_max_cap(self):
        """獎勵上限 0.08"""
        reward = joker_holding_bonus(5, ante=1)
        assert reward <= 0.08

    def test_range(self):
        """獎勵範圍 -0.08 ~ 0.08"""
        for count in range(6):
            for ante in [1, 3, 5, 8]:
                reward = joker_holding_bonus(count, ante=ante)
                assert -0.08 <= reward <= 0.08, \
                    f"Out of range: count={count}, ante={ante}: {reward}"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
