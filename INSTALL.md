# Joker Guide - Installation Guide

[繁體中文版本](#繁體中文)

## Quick Install (One-Click)

```bash
chmod +x scripts/setup.sh
./scripts/setup.sh
```

This script automatically:
1. Installs system dependencies (protobuf compiler, build tools)
2. Installs Rust toolchain (if not present)
3. Builds the Rust game engine
4. Creates Python virtual environment
5. Installs all Python dependencies (including TensorBoard)
6. Generates gRPC proto files
7. Verifies the installation

## Manual Installation

### Prerequisites

| Requirement | Version | Check Command |
|-------------|---------|---------------|
| Python | >=3.10 | `python3 --version` |
| Rust | >=1.70 | `rustc --version` |
| protoc | any | `protoc --version` |

### Step 1: Install System Dependencies

**Linux (Ubuntu/Debian):**
```bash
sudo apt update
sudo apt install -y protobuf-compiler netcat-openbsd build-essential python3-venv python3-dev
```

**Linux (RHEL/CentOS/Fedora):**
```bash
sudo dnf install -y protobuf-compiler nmap-ncat gcc gcc-c++ python3-devel
```

**macOS:**
```bash
brew install protobuf
```

### Step 2: Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Step 3: Build Rust Engine

```bash
cd rust-engine
cargo build --release
cd ..
```

### Step 4: Setup Python Environment

```bash
cd python-env

# Create virtual environment
python3 -m venv .venv
source .venv/bin/activate

# Upgrade pip
pip install --upgrade pip

# Install package with all dependencies
pip install -e .

# Install additional tools
pip install tensorboard pytest

cd ..
```

### Step 5: Generate Proto Files

```bash
source python-env/.venv/bin/activate
PYTHON_BIN=python ./scripts/gen_proto.sh
```

### Step 6: Verify Installation

```bash
source python-env/.venv/bin/activate
python -c "
import gymnasium
import torch
import stable_baselines3
import sb3_contrib
import tensorboard
print('All dependencies installed successfully!')
print(f'PyTorch: {torch.__version__}')
print(f'CUDA available: {torch.cuda.is_available()}')
"
```

## Python Dependencies

### Core Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| grpcio | >=1.62 | gRPC communication with Rust engine |
| grpcio-tools | >=1.62 | Proto file compilation |
| protobuf | >=4.25 | Protocol buffer serialization |
| numpy | >=1.26 | Numerical computation |
| gymnasium | >=0.29 | RL environment interface |
| torch | >=2.1 | Deep learning framework |
| stable-baselines3 | >=2.3 | RL algorithms (PPO, etc.) |
| sb3-contrib | >=2.3 | MaskablePPO for action masking |

### Additional Tools

| Package | Purpose |
|---------|---------|
| tensorboard | Training visualization |
| pytest | Running tests |

## GPU Support

### NVIDIA CUDA

PyTorch with CUDA support is automatically installed if available. To verify:

```bash
python -c "import torch; print(f'CUDA: {torch.cuda.is_available()}')"
```

To install a specific CUDA version:

```bash
# CUDA 12.1
pip install torch --index-url https://download.pytorch.org/whl/cu121

# CUDA 11.8
pip install torch --index-url https://download.pytorch.org/whl/cu118
```

### Apple Silicon (M1/M2/M3)

MPS acceleration is automatically detected. Enable with `--mps` flag:

```bash
./train.sh 4 --timesteps 100000 --mps
```

### CPU Only

For smaller installations without GPU:

```bash
pip install torch --index-url https://download.pytorch.org/whl/cpu
```

## Troubleshooting

### Common Issues

| Error | Cause | Solution |
|-------|-------|----------|
| `protoc: command not found` | Missing protobuf compiler | Install protobuf-compiler |
| `ModuleNotFoundError: joker_env` | Package not installed | Run `pip install -e .` in python-env/ |
| `grpcio installation fails` | Missing build tools | Install build-essential (Linux) or Xcode CLT (macOS) |
| `sed: invalid option -- ''` | Linux sed vs macOS sed | Use updated gen_proto.sh |
| `nc: command not found` | Missing netcat | Install netcat-openbsd (Debian) or nmap-ncat (RHEL) |

### Regenerating Proto Files

If you modify `proto/joker_guide.proto`:

```bash
source python-env/.venv/bin/activate
PYTHON_BIN=python ./scripts/gen_proto.sh
```

### Rebuilding Rust Engine

After modifying Rust code:

```bash
cd rust-engine
cargo build --release
```

---

# 繁體中文

## 快速安裝（一鍵安裝）

```bash
chmod +x scripts/setup.sh
./scripts/setup.sh
```

此腳本自動執行：
1. 安裝系統依賴（protobuf 編譯器、建置工具）
2. 安裝 Rust 工具鏈（如果沒有）
3. 編譯 Rust 遊戲引擎
4. 建立 Python 虛擬環境
5. 安裝所有 Python 依賴（包括 TensorBoard）
6. 生成 gRPC proto 檔案
7. 驗證安裝

## 手動安裝

### 系統需求

| 需求 | 版本 | 檢查指令 |
|------|------|----------|
| Python | >=3.10 | `python3 --version` |
| Rust | >=1.70 | `rustc --version` |
| protoc | 任意 | `protoc --version` |

### 步驟 1：安裝系統依賴

**Linux (Ubuntu/Debian):**
```bash
sudo apt update
sudo apt install -y protobuf-compiler netcat-openbsd build-essential python3-venv python3-dev
```

**Linux (RHEL/CentOS/Fedora):**
```bash
sudo dnf install -y protobuf-compiler nmap-ncat gcc gcc-c++ python3-devel
```

**macOS:**
```bash
brew install protobuf
```

### 步驟 2：安裝 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### 步驟 3：編譯 Rust 引擎

```bash
cd rust-engine
cargo build --release
cd ..
```

### 步驟 4：設置 Python 環境

```bash
cd python-env

# 建立虛擬環境
python3 -m venv .venv
source .venv/bin/activate

# 升級 pip
pip install --upgrade pip

# 安裝套件及所有依賴
pip install -e .

# 安裝額外工具
pip install tensorboard pytest

cd ..
```

### 步驟 5：生成 Proto 檔案

```bash
source python-env/.venv/bin/activate
PYTHON_BIN=python ./scripts/gen_proto.sh
```

### 步驟 6：驗證安裝

```bash
source python-env/.venv/bin/activate
python -c "
import gymnasium
import torch
import stable_baselines3
import sb3_contrib
import tensorboard
print('所有依賴安裝成功！')
print(f'PyTorch: {torch.__version__}')
print(f'CUDA 可用: {torch.cuda.is_available()}')
"
```

## Python 依賴套件

### 核心依賴

| 套件 | 版本 | 用途 |
|------|------|------|
| grpcio | >=1.62 | 與 Rust 引擎的 gRPC 通訊 |
| grpcio-tools | >=1.62 | Proto 檔案編譯 |
| protobuf | >=4.25 | Protocol buffer 序列化 |
| numpy | >=1.26 | 數值計算 |
| gymnasium | >=0.29 | RL 環境介面 |
| torch | >=2.1 | 深度學習框架 |
| stable-baselines3 | >=2.3 | RL 演算法（PPO 等） |
| sb3-contrib | >=2.3 | MaskablePPO（動作遮罩） |

### 額外工具

| 套件 | 用途 |
|------|------|
| tensorboard | 訓練視覺化 |
| pytest | 執行測試 |

## GPU 支援

### NVIDIA CUDA

如果系統有 CUDA，PyTorch 會自動安裝 CUDA 版本。驗證：

```bash
python -c "import torch; print(f'CUDA: {torch.cuda.is_available()}')"
```

安裝特定 CUDA 版本：

```bash
# CUDA 12.1
pip install torch --index-url https://download.pytorch.org/whl/cu121

# CUDA 11.8
pip install torch --index-url https://download.pytorch.org/whl/cu118
```

### Apple Silicon (M1/M2/M3)

MPS 加速會自動偵測。使用 `--mps` 旗標啟用：

```bash
./train.sh 4 --timesteps 100000 --mps
```

### 純 CPU

如需較小的安裝（無 GPU）：

```bash
pip install torch --index-url https://download.pytorch.org/whl/cpu
```

## 疑難排解

### 常見問題

| 錯誤 | 原因 | 解決方案 |
|------|------|----------|
| `protoc: command not found` | 缺少 protobuf 編譯器 | 安裝 protobuf-compiler |
| `ModuleNotFoundError: joker_env` | 套件未安裝 | 在 python-env/ 執行 `pip install -e .` |
| `grpcio installation fails` | 缺少建置工具 | 安裝 build-essential (Linux) 或 Xcode CLT (macOS) |
| `sed: invalid option -- ''` | Linux sed vs macOS sed | 使用更新的 gen_proto.sh |
| `nc: command not found` | 缺少 netcat | 安裝 netcat-openbsd (Debian) 或 nmap-ncat (RHEL) |

### 重新生成 Proto 檔案

修改 `proto/joker_guide.proto` 後：

```bash
source python-env/.venv/bin/activate
PYTHON_BIN=python ./scripts/gen_proto.sh
```

### 重新編譯 Rust 引擎

修改 Rust 程式碼後：

```bash
cd rust-engine
cargo build --release
```
