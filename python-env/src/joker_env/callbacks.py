"""
Custom callbacks for SB3 training with detailed metrics logging.

使用方式：
```python
from stable_baselines3 import PPO
from joker_env.env import JokerGymEnv
from joker_env.callbacks import JokerMetricsCallback

env = JokerGymEnv()
callback = JokerMetricsCallback(verbose=1)
model = PPO("MlpPolicy", env)
model.learn(total_timesteps=100000, callback=callback)
```
"""

from typing import Dict, Any, Optional
import numpy as np
from stable_baselines3.common.callbacks import BaseCallback


class JokerMetricsCallback(BaseCallback):
    """
    自定義 Callback，將 JokerGymEnv 的詳細 metrics 記錄到 TensorBoard。

    Features:
    - 追蹤每個 episode 的詳細統計
    - 定期打印聚合統計到控制台
    - 將所有 metrics 記錄到 TensorBoard
    """

    def __init__(
        self,
        verbose: int = 0,
        log_freq: int = 10,  # 每 N 個 episode 打印一次
        tb_log_freq: int = 1,  # 每 N 個 episode 記錄到 TB
    ):
        super().__init__(verbose)
        self.log_freq = log_freq
        self.tb_log_freq = tb_log_freq
        self.episode_count = 0
        self.episode_rewards = []
        self.episode_lengths = []

        # 追蹤聚合統計
        self.total_wins = 0
        self.total_antes = 0
        self.recent_metrics: list = []
        self.window_size = 100

    def _on_step(self) -> bool:
        # 檢查是否有 episode 結束
        for idx, done in enumerate(self.locals.get("dones", [])):
            if done:
                self.episode_count += 1

                # 從 info 獲取詳細 metrics
                infos = self.locals.get("infos", [])
                if idx < len(infos):
                    info = infos[idx]
                    self._log_episode_metrics(info)

        return True

    def _log_episode_metrics(self, info: Dict[str, Any]):
        """記錄單個 episode 的 metrics"""

        # 提取關鍵指標
        won = info.get("episode/won", 0)
        final_ante = info.get("episode/final_ante", 1)
        total_reward = info.get("episode/total_reward", 0)
        blind_clear_rate = info.get("progress/blind_clear_rate", 0)

        self.total_wins += won
        self.total_antes += final_ante

        # 更新 recent metrics
        self.recent_metrics.append({
            "won": won,
            "ante": final_ante,
            "reward": total_reward,
            "blind_clear_rate": blind_clear_rate,
        })
        if len(self.recent_metrics) > self.window_size:
            self.recent_metrics.pop(0)

        # 記錄到 TensorBoard
        if self.episode_count % self.tb_log_freq == 0 and self.logger:
            # Episode 級別統計
            for key, value in info.items():
                if isinstance(value, (int, float)) and not np.isnan(value):
                    self.logger.record(f"joker/{key}", value)

            # 移動平均統計
            if len(self.recent_metrics) > 0:
                recent_wins = [m["won"] for m in self.recent_metrics]
                recent_antes = [m["ante"] for m in self.recent_metrics]
                recent_rewards = [m["reward"] for m in self.recent_metrics]

                self.logger.record("joker/recent_win_rate", np.mean(recent_wins))
                self.logger.record("joker/recent_avg_ante", np.mean(recent_antes))
                self.logger.record("joker/recent_avg_reward", np.mean(recent_rewards))

        # 控制台輸出
        if self.verbose > 0 and self.episode_count % self.log_freq == 0:
            self._print_summary(info)

    def _print_summary(self, info: Dict[str, Any]):
        """打印訓練摘要到控制台"""
        n = max(1, self.episode_count)
        recent_n = len(self.recent_metrics)

        # 計算移動平均
        if recent_n > 0:
            recent_win_rate = np.mean([m["won"] for m in self.recent_metrics])
            recent_avg_ante = np.mean([m["ante"] for m in self.recent_metrics])
            recent_avg_reward = np.mean([m["reward"] for m in self.recent_metrics])
        else:
            recent_win_rate = 0
            recent_avg_ante = 1
            recent_avg_reward = 0

        # 從 info 提取詳細統計
        blind_clear_rate = info.get("progress/blind_clear_rate", 0)
        skip_rate = info.get("progress/skip_rate", 0)
        max_money = info.get("economy/max_money", 0)
        jokers_bought = info.get("joker/bought", 0)
        plays_made = info.get("play/total_plays", 0)
        discards_made = info.get("play/total_discards", 0)

        print("\n" + "=" * 60)
        print(f"Episode {self.episode_count} Summary")
        print("=" * 60)
        print(f"Overall:      Win Rate: {self.total_wins / n:.1%} | Avg Ante: {self.total_antes / n:.1f}")
        print(f"Recent({recent_n:3d}): Win Rate: {recent_win_rate:.1%} | Avg Ante: {recent_avg_ante:.1f} | Avg Reward: {recent_avg_reward:.2f}")
        print("-" * 60)
        print(f"This Episode:")
        print(f"  Result: {'WIN' if info.get('episode/won') else 'LOSE'} | Ante: {info.get('episode/final_ante', 1)} | Steps: {info.get('episode/total_steps', 0)}")
        print(f"  Blind Clear Rate: {blind_clear_rate:.1%} | Skip Rate: {skip_rate:.1%}")
        print(f"  Economy: Max ${max_money} | Jokers Bought: {jokers_bought}")
        print(f"  Actions: Plays={plays_made} | Discards={discards_made}")
        print("=" * 60 + "\n")

    def _on_training_end(self):
        """訓練結束時打印最終統計"""
        if self.verbose > 0:
            n = max(1, self.episode_count)
            print("\n" + "=" * 60)
            print("Final Training Statistics")
            print("=" * 60)
            print(f"Total Episodes: {self.episode_count}")
            print(f"Overall Win Rate: {self.total_wins / n:.1%}")
            print(f"Overall Avg Ante: {self.total_antes / n:.2f}")
            print("=" * 60 + "\n")


class EvalMetricsCallback(BaseCallback):
    """
    用於評估的 Callback，收集詳細統計但不干擾訓練。
    """

    def __init__(self, n_eval_episodes: int = 100):
        super().__init__(verbose=0)
        self.n_eval_episodes = n_eval_episodes
        self.results = []

    def _on_step(self) -> bool:
        for idx, done in enumerate(self.locals.get("dones", [])):
            if done:
                infos = self.locals.get("infos", [])
                if idx < len(infos):
                    self.results.append(infos[idx])

                if len(self.results) >= self.n_eval_episodes:
                    return False  # 停止評估

        return True

    def get_summary(self) -> Dict[str, float]:
        """取得評估摘要"""
        if not self.results:
            return {}

        wins = [r.get("episode/won", 0) for r in self.results]
        antes = [r.get("episode/final_ante", 1) for r in self.results]
        rewards = [r.get("episode/total_reward", 0) for r in self.results]
        blind_clear_rates = [r.get("progress/blind_clear_rate", 0) for r in self.results]

        return {
            "win_rate": np.mean(wins),
            "avg_ante": np.mean(antes),
            "avg_reward": np.mean(rewards),
            "avg_blind_clear_rate": np.mean(blind_clear_rates),
            "std_ante": np.std(antes),
            "std_reward": np.std(rewards),
        }
