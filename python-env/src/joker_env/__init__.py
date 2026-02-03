from .client import JokerEnvClient
from .env import JokerGymEnv, JokerGymDictEnv, EpisodeMetrics, AggregatedMetrics
from .batch_vec_env import JokerBatchVecEnv

__all__ = [
    "JokerEnvClient",
    "JokerGymEnv",
    "JokerGymDictEnv",
    "JokerBatchVecEnv",
    "EpisodeMetrics",
    "AggregatedMetrics",
]

# Optional: SB3 callbacks (requires stable_baselines3)
try:
    from .callbacks import JokerMetricsCallback, EvalMetricsCallback
    __all__.extend(["JokerMetricsCallback", "EvalMetricsCallback"])
except ImportError:
    pass  # stable_baselines3 not installed
