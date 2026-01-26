#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -z "${PYTHON_BIN:-}" ]]; then
  if command -v python >/dev/null 2>&1; then
    PYTHON_BIN="python"
  elif command -v python3 >/dev/null 2>&1; then
    PYTHON_BIN="python3"
  else
    echo "Python not found. Set PYTHON_BIN or install python3." >&2
    exit 1
  fi
fi

"${PYTHON_BIN}" -m grpc_tools.protoc \
  -I "${ROOT_DIR}/proto" \
  --python_out "${ROOT_DIR}/python-env/src/joker_env/proto" \
  --grpc_python_out "${ROOT_DIR}/python-env/src/joker_env/proto" \
  "${ROOT_DIR}/proto/joker_guide.proto"

GRPC_FILE="${ROOT_DIR}/python-env/src/joker_env/proto/joker_guide_pb2_grpc.py"
if [[ -f "${GRPC_FILE}" ]]; then
  sed -i '' 's/^import joker_guide_pb2 as joker__guide__pb2$/from . import joker_guide_pb2 as joker__guide__pb2/' "${GRPC_FILE}"
fi
