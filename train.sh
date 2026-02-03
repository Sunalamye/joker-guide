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

# 解析是否禁用 TensorBoard / profiling 設定
ENABLE_TB=true
PROFILE_EVERY=0
PY_PROFILE_EVERY=0
GRPC_PROFILE_EVERY=0
EXTRA_ARGS=()
while [ $# -gt 0 ]; do
    case "$1" in
        --no-tensorboard)
            ENABLE_TB=false
            ;;
        --profile-every)
            PROFILE_EVERY="${2:-0}"
            shift
            ;;
        --py-profile-every)
            PY_PROFILE_EVERY="${2:-0}"
            shift
            ;;
        --grpc-profile-every)
            GRPC_PROFILE_EVERY="${2:-0}"
            shift
            ;;
        *)
            EXTRA_ARGS+=("$1")
            ;;
    esac
    shift
done

# 清理函數
cleanup() {
    echo ""
    echo "Stopping all processes..."
    if [ -n "$ENGINE_PID" ] && kill -0 "$ENGINE_PID" 2>/dev/null; then
        kill "$ENGINE_PID" 2>/dev/null || true
    fi
    if [ -n "$TAIL_PID" ] && kill -0 "$TAIL_PID" 2>/dev/null; then
        kill "$TAIL_PID" 2>/dev/null || true
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
ENGINE_LOG="$LOG_DIR/engine.log"
ENGINE_CMD=("$RUST_ENGINE" --port "$PORT")
if command -v stdbuf >/dev/null 2>&1; then
    ENGINE_CMD=(stdbuf -oL -eL "${ENGINE_CMD[@]}")
fi
if [ "$PROFILE_EVERY" != "0" ]; then
    echo "Profiling enabled: every $PROFILE_EVERY steps (Rust)"
    JOKER_PROFILE_EVERY="$PROFILE_EVERY" "${ENGINE_CMD[@]}" >"$ENGINE_LOG" 2>&1 &
else
    "${ENGINE_CMD[@]}" >"$ENGINE_LOG" 2>&1 &
fi
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

if [ "$PROFILE_EVERY" != "0" ]; then
    echo "Tailing engine log: $ENGINE_LOG"
    tail -f "$ENGINE_LOG" &
    TAIL_PID=$!
    echo ""
fi

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
JOKER_PY_PROFILE_EVERY="$PY_PROFILE_EVERY" \
JOKER_GRPC_PROFILE_EVERY="$GRPC_PROFILE_EVERY" \
PYTHONPATH=python-env/src python -m joker_env.train_sb3 \
    --n-envs "$N_ENVS" \
    --tensorboard-log "$LOG_DIR" \
    --net-arch 512 512 256 \
    "${EXTRA_ARGS[@]}"
