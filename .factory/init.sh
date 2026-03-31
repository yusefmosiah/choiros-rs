#!/usr/bin/env bash
set -euo pipefail

# Ensure required tooling is available
command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found"; exit 1; }
command -v rustc >/dev/null 2>&1 || { echo "ERROR: rustc not found"; exit 1; }
command -v nix >/dev/null 2>&1 || { echo "ERROR: nix not found"; exit 1; }

echo "Environment check: cargo=$(cargo --version | head -1)"
echo "Environment check: nix=$(nix --version | head -1)"

# Warm command metadata without doing heavy validation.
cargo metadata --no-deps --format-version 1 >/dev/null
