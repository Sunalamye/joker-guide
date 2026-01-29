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
)


class TestPlayReward:
    """出牌獎勵測試"""

    def test_zero_score(self):
        """零分不給獎勵"""
        assert play_reward(0, 1000) == 0.0

    def test_zero_required(self):
        """目標為零不給獎勵"""
        assert play_reward(100, 0) == 0.0

    def test_partial_progress(self):
        """部分進度給予比例獎勵"""
        reward = play_reward(500, 1000)
        assert 0.1 < reward < 0.15  # 50% 進度

    def test_exact_target(self):
        """剛好達標給予基礎獎勵"""
        reward = play_reward(1000, 1000)
        assert 0.2 < reward < 0.3

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
        """不棄牌不給獎勵"""
        assert discard_reward(0, 3) == 0.0

    def test_precise_discard(self):
        """精準棄牌（1-2 張）獎勵最高"""
        reward_2 = discard_reward(2, 3)
        reward_5 = discard_reward(5, 3)
        assert reward_2 > reward_5

    def test_last_discard_bonus(self):
        """最後一次棄牌有額外獎勵"""
        reward_with_left = discard_reward(2, 2)
        reward_last = discard_reward(2, 0)
        assert reward_last > reward_with_left

    def test_max_cap(self):
        """獎勵不超過上限 0.05"""
        reward = discard_reward(1, 0)
        assert reward <= 0.05


class TestBlindClearReward:
    """過關獎勵測試"""

    def test_small_blind(self):
        """Small Blind 基礎獎勵（v5.0 提升）"""
        reward = blind_clear_reward(0, BLIND_SMALL, 1)
        assert 0.25 < reward < 0.4

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
        """獎勵範圍 0.25~0.75（v5.0 調整）"""
        reward = blind_clear_reward(4, BLIND_BOSS, 8, boss_blind_id=1)
        assert 0.25 <= reward <= 0.75


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
        """高成本占比有經濟懲罰（同成本，不同資金）"""
        # 同樣成本，資金多時獎勵更高
        rich = joker_buy_reward(5, 50, 1, 2, 3, 5)  # 10% 資金
        poor = joker_buy_reward(5, 10, 1, 2, 3, 5)  # 50% 資金
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
        """獎勵範圍 -0.3~0.3"""
        reward = joker_buy_reward(5, 10, 1, 0, 1, 5)
        assert -0.3 <= reward <= 0.3


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
        """勝利給予最大獎勵"""
        assert game_end_reward(GAME_END_WIN, 8) == 1.0

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
        """獎勵範圍（v5.0 漸進式公式）"""
        reward = ante_progress_reward(1, 8)
        # v5.0: 1→8 累積約 3.94
        assert reward <= 4.5


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

    def test_no_reward_for_no_improvement(self):
        """牌型沒改善不給獎勵"""
        from joker_env.reward import hand_setup_reward, HAND_FLUSH, HAND_PAIR
        reward = hand_setup_reward(HAND_FLUSH, HAND_PAIR, had_discard=True)
        assert reward == 0.0

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
        """通用 Joker 無獎勵也無懲罰"""
        from joker_env.reward import BuildTracker, HAND_FLUSH
        tracker = BuildTracker()
        for _ in range(6):
            tracker.record_hand(HAND_FLUSH)

        # 假設 id=1 是通用 Joker（未在 JOKER_BUILD_SUPPORT 中）
        bonus = tracker.joker_build_bonus(1, 5)
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


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
