#!/bin/bash
# setup-ec2-env.sh - Setup EC2 environment for ChoirOS development
# Run this on EC2 after cloning the repo

set -e

echo "=== ChoirOS EC2 Environment Setup ==="

# Source cargo (add to .bashrc if not there)
if ! grep -q "cargo/env" ~/.bashrc; then
    echo 'source "$HOME/.cargo/env"' >> ~/.bashrc
    echo "✓ Added cargo to PATH"
fi
source "$HOME/.cargo/env"

# Verify tools
echo ""
echo "Verifying installed tools..."
cargo --version
just --version
sqlx --version
trunk --version
dx --version
cargo-nextest --version

# Create data directory
mkdir -p ~/choiros-rs/data

# Build the project (first time - will take a while)
echo ""
echo "Building ChoirOS (this will take 5-10 minutes on first run)..."
cd ~/choiros-rs
cargo build --release

# Run tests
echo ""
echo "Running tests..."
cargo test -p sandbox

echo ""
echo "=== Setup Complete ==="
echo "Next steps:"
echo "  1. Setup tmux workflow: ./scripts/dev-workflow.sh start"
echo "  2. Or run manually: just dev-sandbox"
echo ""
echo "Repository: https://github.com/yusefmosiah/choiros-rs"
echo "Docs: ~/choiros-rs/docs/"