#!/bin/bash
# 訓練啟動腳本 - 單一 Rust 引擎支援多遊戲並發
#
# 使用方式: ./train.sh [Python環境數] [其他訓練參數...]
#
# 例如:
#   ./train.sh 4 --timesteps 1000000     # 4 個並行環境
#   ./train.sh 8 --timesteps 1000000     # 8 個並行環境
#   ./train.sh 16 --timesteps 1000000    # 16 個並行環境

set -e

# 預設參數
N_ENVS=${1:-4}
shift 1 2>/dev/null || true

PORT=50051
RUST_ENGINE="./rust-engine/target/release/joker_env"

# 清理函數
cleanup() {
    echo ""
    echo "Stopping all processes..."
    if [ -n "$ENGINE_PID" ] && kill -0 "$ENGINE_PID" 2>/dev/null; then
        kill "$ENGINE_PID" 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    echo "All processes stopped."
    exit 0
}

# 註冊信號處理
trap cleanup SIGINT SIGTERM EXIT

# 檢查 Rust 引擎是否存在
if [ ! -f "$RUST_ENGINE" ]; then
    echo "Error: Rust engine not found at $RUST_ENGINE"
    echo "Please build it first with: cd rust-engine && cargo build --release"
    exit 1
fi

echo "============================================"
echo "  Joker Guide Training (Concurrent Mode)"
echo "============================================"
echo "  Rust engine:  1 (with multi-game support)"
echo "  Python envs:  $N_ENVS"
echo "============================================"
echo ""

# 啟動單一 Rust 引擎（支援多遊戲）
echo "Starting Rust engine on port $PORT..."
"$RUST_ENGINE" --port "$PORT" &
ENGINE_PID=$!

# 等待引擎啟動
sleep 1

# 驗證引擎是否正常運行
if ! kill -0 "$ENGINE_PID" 2>/dev/null; then
    echo "Error: Rust engine failed to start"
    exit 1
fi

if nc -z 127.0.0.1 "$PORT" 2>/dev/null; then
    echo "Rust engine started and ready!"
else
    echo "Warning: Engine may not be ready yet, waiting..."
    sleep 1
fi
echo ""

# 啟動 Python 訓練（所有環境連接同一個引擎）
echo "Starting training with $N_ENVS parallel environments..."
echo "Press Ctrl+C to stop"
echo ""

JOKER_BASE_PORT=$PORT \
JOKER_N_ENGINES=1 \
PYTHONPATH=python-env/src python -m joker_env.train_sb3 \
    --n-envs "$N_ENVS" \
    "$@"
