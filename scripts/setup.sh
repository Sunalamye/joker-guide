#!/bin/bash
# Joker Guide - One-Click Setup Script
# Supports: Linux (Ubuntu/Debian, RHEL/CentOS), macOS

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo -e "\n${BLUE}============================================${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}============================================${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Detect OS
detect_os() {
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command -v apt-get &> /dev/null; then
            OS="debian"
        elif command -v yum &> /dev/null; then
            OS="rhel"
        elif command -v dnf &> /dev/null; then
            OS="fedora"
        else
            OS="linux"
        fi
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        OS="macos"
    else
        OS="unknown"
    fi
    echo "$OS"
}

# Check Python version
check_python() {
    print_header "Checking Python"

    if command -v python3 &> /dev/null; then
        PYTHON_VERSION=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')
        PYTHON_MAJOR=$(echo "$PYTHON_VERSION" | cut -d. -f1)
        PYTHON_MINOR=$(echo "$PYTHON_VERSION" | cut -d. -f2)

        if [[ "$PYTHON_MAJOR" -ge 3 ]] && [[ "$PYTHON_MINOR" -ge 10 ]]; then
            print_success "Python $PYTHON_VERSION found"
            return 0
        else
            print_error "Python $PYTHON_VERSION found, but >=3.10 required"
            return 1
        fi
    else
        print_error "Python 3 not found"
        return 1
    fi
}

# Check Rust
check_rust() {
    print_header "Checking Rust"

    if command -v cargo &> /dev/null; then
        RUST_VERSION=$(rustc --version | awk '{print $2}')
        print_success "Rust $RUST_VERSION found"
        return 0
    else
        print_warning "Rust not found"
        return 1
    fi
}

# Install system dependencies
install_system_deps() {
    print_header "Installing System Dependencies"

    OS=$(detect_os)

    case "$OS" in
        debian)
            echo "Detected: Debian/Ubuntu"
            sudo apt-get update
            sudo apt-get install -y \
                protobuf-compiler \
                netcat-openbsd \
                build-essential \
                python3-venv \
                python3-dev
            print_success "System dependencies installed"
            ;;
        rhel)
            echo "Detected: RHEL/CentOS"
            sudo yum install -y \
                protobuf-compiler \
                nmap-ncat \
                gcc \
                gcc-c++ \
                python3-devel
            print_success "System dependencies installed"
            ;;
        fedora)
            echo "Detected: Fedora"
            sudo dnf install -y \
                protobuf-compiler \
                nmap-ncat \
                gcc \
                gcc-c++ \
                python3-devel
            print_success "System dependencies installed"
            ;;
        macos)
            echo "Detected: macOS"
            if command -v brew &> /dev/null; then
                brew install protobuf
                print_success "System dependencies installed via Homebrew"
            else
                print_error "Homebrew not found. Please install: https://brew.sh"
                exit 1
            fi
            ;;
        *)
            print_warning "Unknown OS. Please install protobuf-compiler manually."
            ;;
    esac
}

# Install Rust
install_rust() {
    print_header "Installing Rust"

    if command -v cargo &> /dev/null; then
        print_success "Rust already installed"
    else
        echo "Installing Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
        print_success "Rust installed"
    fi
}

# Build Rust engine
build_rust_engine() {
    print_header "Building Rust Engine"

    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

    cd "$PROJECT_ROOT/rust-engine"
    cargo build --release
    print_success "Rust engine built: rust-engine/target/release/joker_env"
}

# Setup Python environment
setup_python_env() {
    print_header "Setting Up Python Environment"

    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

    cd "$PROJECT_ROOT/python-env"

    # Create virtual environment if not exists
    if [[ ! -d ".venv" ]]; then
        echo "Creating virtual environment..."
        python3 -m venv .venv
        print_success "Virtual environment created"
    else
        print_success "Virtual environment already exists"
    fi

    # Activate and install
    source .venv/bin/activate

    echo "Upgrading pip..."
    pip install --upgrade pip

    echo "Installing joker-env package..."
    pip install -e .

    echo "Installing additional dependencies..."
    pip install tensorboard pytest

    print_success "Python environment configured"

    echo ""
    echo -e "${GREEN}Virtual environment location: python-env/.venv${NC}"
    echo -e "${GREEN}Activate with: source python-env/.venv/bin/activate${NC}"
}

# Generate proto files
generate_proto() {
    print_header "Generating Proto Files"

    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

    cd "$PROJECT_ROOT"

    # Activate Python env
    source python-env/.venv/bin/activate

    # Run proto generation with cross-platform fix
    PYTHON_BIN=python ./scripts/gen_proto.sh

    print_success "Proto files generated"
}

# Verify installation
verify_installation() {
    print_header "Verifying Installation"

    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

    cd "$PROJECT_ROOT"
    source python-env/.venv/bin/activate

    # Check imports
    echo "Checking Python imports..."
    python3 -c "
import gymnasium
import torch
import stable_baselines3
import sb3_contrib
import grpc
import tensorboard
import numpy
print('All imports successful!')
print(f'  PyTorch: {torch.__version__}')
print(f'  Gymnasium: {gymnasium.__version__}')
print(f'  SB3: {stable_baselines3.__version__}')
print(f'  NumPy: {numpy.__version__}')
"

    # Check Rust binary
    if [[ -f "rust-engine/target/release/joker_env" ]]; then
        print_success "Rust engine binary found"
    else
        print_error "Rust engine binary not found"
    fi

    print_success "Installation verified"
}

# Print usage
print_usage() {
    print_header "Setup Complete!"

    echo -e "To start training:"
    echo -e ""
    echo -e "  ${GREEN}# Activate Python environment${NC}"
    echo -e "  source python-env/.venv/bin/activate"
    echo -e ""
    echo -e "  ${GREEN}# Quick training (recommended)${NC}"
    echo -e "  ./train.sh 4 --timesteps 100000 --checkpoint python-env/models/my_model"
    echo -e ""
    echo -e "  ${GREEN}# With TensorBoard logging${NC}"
    echo -e "  ./train.sh 4 --timesteps 100000 --checkpoint python-env/models/my_model \\"
    echo -e "    --tensorboard-log python-env/logs/my_run"
    echo -e ""
    echo -e "  ${GREEN}# View TensorBoard${NC}"
    echo -e "  tensorboard --logdir python-env/logs/"
    echo -e ""
}

# Main
main() {
    print_header "Joker Guide - One-Click Setup"

    echo "This script will:"
    echo "  1. Install system dependencies (protobuf, build tools)"
    echo "  2. Install Rust (if not present)"
    echo "  3. Build the Rust game engine"
    echo "  4. Create Python virtual environment"
    echo "  5. Install all Python dependencies"
    echo "  6. Generate gRPC proto files"
    echo ""

    # Parse arguments
    SKIP_CONFIRM=false
    SKIP_SYSTEM_DEPS=false

    while [[ "$#" -gt 0 ]]; do
        case $1 in
            -y|--yes) SKIP_CONFIRM=true ;;
            --skip-system-deps) SKIP_SYSTEM_DEPS=true ;;
            -h|--help)
                echo "Usage: $0 [options]"
                echo ""
                echo "Options:"
                echo "  -y, --yes            Skip confirmation prompts"
                echo "  --skip-system-deps   Skip system dependency installation"
                echo "  -h, --help           Show this help"
                exit 0
                ;;
            *) echo "Unknown option: $1"; exit 1 ;;
        esac
        shift
    done

    if [[ "$SKIP_CONFIRM" == false ]]; then
        read -p "Continue? [Y/n] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]] && [[ ! -z $REPLY ]]; then
            echo "Aborted."
            exit 0
        fi
    fi

    # Run setup steps
    check_python || { print_error "Please install Python 3.10+"; exit 1; }

    if [[ "$SKIP_SYSTEM_DEPS" == false ]]; then
        install_system_deps
    fi

    install_rust

    # Ensure cargo is in PATH
    if [[ -f "$HOME/.cargo/env" ]]; then
        source "$HOME/.cargo/env"
    fi

    build_rust_engine
    setup_python_env
    generate_proto
    verify_installation
    print_usage
}

main "$@"
