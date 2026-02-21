# Justfile - Task runner for ChoirOS
# This file contains development, build, testing, and deployment commands

# Default recipe - list all available tasks
default:
    @just --list

# Development commands
# Run backend API server with local database
dev-sandbox:
    cd sandbox && DATABASE_URL="sqlite:../data/events.db" SQLX_OFFLINE=true CARGO_INCREMENTAL=0 cargo run

# Run hypervisor component
dev-hypervisor:
    cd hypervisor && SQLX_OFFLINE=true DATABASE_URL="sqlite:../data/hypervisor.db" cargo run

# Build the Dioxus WASM frontend (debug) into target/dx/sandbox-ui/debug/web/public
build-ui:
    cd dioxus-desktop && dx build

# Build the Dioxus WASM frontend (release)
build-ui-release:
    cd dioxus-desktop && dx build --release

# Build UI then run hypervisor — full stack on port 9090
# Builds the sandbox binary, the Dioxus WASM frontend, then starts the hypervisor.
# The hypervisor serves the WASM app and proxies authenticated traffic to the sandbox.
dev-full: build-ui
    cargo build -p sandbox
    cd hypervisor && SQLX_OFFLINE=true DATABASE_URL="sqlite:../data/hypervisor.db" cargo run

# Run Dioxus frontend development server (port 3000)
dev-ui:
    cd dioxus-desktop && dx serve --port 3000 --addr 0.0.0.0

# Stop/kill running development processes
stop:
    @echo "Stopping ChoirOS development processes..."
    @pkill -9 -f "target/debug/sandbox" 2>/dev/null || true
    @pkill -9 -f "target/debug/hypervisor" 2>/dev/null || true
    @pkill -9 -f "dx serve --port 3000" 2>/dev/null || true
    @pkill -9 -f "vite --port 3000" 2>/dev/null || true
    @echo "✓ All processes stopped"

# Build commands
# Build all packages in release mode
build:
    cargo build --release

# Build frontend + backend for production
# Frontend builds to dist/, then copied to sandbox/static/
# Backend builds in sandbox/
build-sandbox:
    cd dioxus-desktop && dx build --release
    cp -r dioxus-desktop/dist/* sandbox/static/
    cd sandbox && cargo build --release

# Testing
# Run all tests across workspace (unit + integration)
test:
    cargo test --workspace

# Run only unit tests (--lib)
test-unit:
    cargo test --lib --workspace

# Run only integration tests (--test '*')
test-integration:
    cargo test --test '*' --workspace

# Fast, scoped sandbox test runner (avoids broad filtered test sweeps)
test-sandbox-lib +ARGS:
    ./scripts/sandbox-test.sh --lib {{ARGS}}

test-sandbox-itest TEST +ARGS:
    ./scripts/sandbox-test.sh --test {{TEST}} {{ARGS}}

test-conductor-fast:
    ./scripts/sandbox-test.sh --conductor

# Code quality
# Check formatting and linting without making changes
check:
    cargo fmt --check
    cargo clippy --workspace -- -D warnings

# Auto-fix formatting and linting issues
fix:
    cargo fmt
    cargo clippy --fix --allow-staged

# Database
# Run SQLx migrations (creates tables if needed)
migrate:
    cd sandbox && cargo sqlx migrate run

# Run hypervisor migrations
migrate-hypervisor:
    cd hypervisor && DATABASE_URL="sqlite:../data/hypervisor.db" sqlx migrate run

# Create new migration file with given name
new-migration NAME:
    cd sandbox && cargo sqlx migrate add {{NAME}}

# Create new hypervisor migration
new-hypervisor-migration NAME:
    cd hypervisor && sqlx migrate add {{NAME}}

# Docker
# Build Docker image for choir-sandbox
docker-build:
    docker build -t choir-sandbox:latest -f sandbox/Dockerfile .

# Run Docker container with port mapping and volume
docker-run:
    docker run -p 8080:8080 -v ./data:/data choir-sandbox:latest

# Build sandbox image with Podman
podman-build:
    podman build -t choir-sandbox:latest -f sandbox/Dockerfile .

# Run sandbox image with Podman
podman-run:
    podman run --rm -it --name choir-sandbox -p 8080:8080 -v ./data:/data:Z choir-sandbox:latest

# Deployment
# Deploy to EC2 instance at 3.83.131.245 (push code + build)
deploy-ec2:
    rsync -avz --delete ./ ubuntu@3.83.131.245:~/choiros-rs/
    ssh ubuntu@3.83.131.245 'cd ~/choiros-rs && just build-sandbox'

# Grind host
# Run canonical pre-push checks directly on grind.
grind-check:
    ssh -i "$HOME/.ssh/choiros-grind.pem" -o StrictHostKeyChecking=accept-new root@18.212.170.200 'set -e; cd /opt/choiros/workspace; git status --short --branch; nix --extra-experimental-features nix-command --extra-experimental-features flakes develop ./hypervisor --command cargo check -p hypervisor; nix --extra-experimental-features nix-command --extra-experimental-features flakes develop ./sandbox --command cargo check -p sandbox; git status --short --branch'

# System Monitor
# View actor network as ASCII diagram
monitor:
    node skills/system-monitor/scripts/system-monitor.js

# Save actor network report to file
monitor-save:
    node skills/system-monitor/scripts/system-monitor.js --save

# Compact actor network view (for chat)
monitor-compact:
    node skills/system-monitor/scripts/system-monitor.js --compact

# Dashboard (Native ChoirOS - Coming Soon)
# Note: Native dashboard will be integrated into ChoirOS UI
# For now, use the UI at http://localhost:3000
