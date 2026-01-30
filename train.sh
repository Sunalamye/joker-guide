#!/bin/bash
# 訓練啟動腳本 - 單一 Rust 引擎支援多遊戲並發
#
# 使用方式: ./train.sh [Python環境數] [其他訓練參數...]
#
# 例如:
#   ./train.sh 4 --timesteps 1000000     # 4 個並行環境
#   ./train.sh 8 --timesteps 1000000     # 8 個並行環境
#   ./train.sh 4 --no-tensorboard        # 關閉 TensorBoard
#
# TensorBoard 預設啟用，訪問 http://localhost:6006

set -e

# 預設參數
N_ENVS=${1:-4}
shift 1 2>/dev/null || true

PORT=50051
RUST_ENGINE="./rust-engine/target/release/joker_env"
TB_PORT=6006

# 生成時間戳記的 log 目錄
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
LOG_DIR="python-env/logs/run_${TIMESTAMP}"

# 解析是否禁用 TensorBoard
ENABLE_TB=true
EXTRA_ARGS=()
for arg in "$@"; do
    if [ "$arg" = "--no-tensorboard" ]; then
        ENABLE_TB=false
    else
        EXTRA_ARGS+=("$arg")
    fi
done

# 清理函數
cleanup() {
    echo ""
    echo "Stopping all processes..."
    if [ -n "$ENGINE_PID" ] && kill -0 "$ENGINE_PID" 2>/dev/null; then
        kill "$ENGINE_PID" 2>/dev/null || true
    fi
    if [ -n "$TB_PID" ] && kill -0 "$TB_PID" 2>/dev/null; then
        kill "$TB_PID" 2>/dev/null || true
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

# 創建 log 目錄
mkdir -p "$LOG_DIR"

echo "============================================"
echo "  Joker Guide Training (Concurrent Mode)"
echo "============================================"
echo "  Rust engine:  1 (with multi-game support)"
echo "  Python envs:  $N_ENVS"
echo "  Log dir:      $LOG_DIR"
if [ "$ENABLE_TB" = true ]; then
echo "  TensorBoard:  http://localhost:$TB_PORT"
fi
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

# 啟動 TensorBoard（如果啟用）
if [ "$ENABLE_TB" = true ]; then
    echo "Starting TensorBoard on port $TB_PORT..."
    python3 -m tensorboard.main --logdir="$LOG_DIR" --port="$TB_PORT" --bind_all 2>/dev/null &
    TB_PID=$!
    sleep 1
    if kill -0 "$TB_PID" 2>/dev/null; then
        echo "TensorBoard started: http://localhost:$TB_PORT"
    else
        echo "Warning: TensorBoard failed to start (may already be running)"
        TB_PID=""
    fi
    echo ""
fi

# 啟動 Python 訓練（所有環境連接同一個引擎）
echo "Starting training with $N_ENVS parallel environments..."
echo "Press Ctrl+C to stop"
echo ""

JOKER_BASE_PORT=$PORT \
JOKER_N_ENGINES=1 \
PYTHONPATH=python-env/src python -m joker_env.train_sb3 \
    --n-envs "$N_ENVS" \
    --tensorboard-log "$LOG_DIR" \
    --net-arch 512 512 256 \
    "${EXTRA_ARGS[@]}"
