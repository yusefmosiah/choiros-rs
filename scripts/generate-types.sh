#!/bin/bash
set -e
echo "Generating TypeScript types..."
mkdir -p /Users/wiz/choiros-rs/dioxus-desktop/src/types
cd /Users/wiz/choiros-rs/shared-types
cargo test export_types 2>&1 || true
echo "Done"
