# Justfile - Task runner for ChoirOS
# This file contains development, build, testing, and deployment commands

# Default recipe - list all available tasks
default:
    @just --list

# Development commands (vfkit cutover flow)
# 1) Build static release frontend assets
local-build-ui:
    cd dioxus-desktop && dx build --release

# Build Rust vfkit host control binary.
build-vfkit-ctl:
    cargo build -p hypervisor --bin vfkit-runtime-ctl

# 2) Run hypervisor against release frontend dist with vfkit runtime control.
local-hypervisor: build-vfkit-ctl
    cd hypervisor && FRONTEND_DIST="$(pwd)/../dioxus-desktop/target/dx/dioxus-desktop/release/web/public" SQLX_OFFLINE=true HYPERVISOR_DATABASE_URL="sqlite:../data/hypervisor.db" SANDBOX_VFKIT_CTL="$(pwd)/../target/debug/vfkit-runtime-ctl" cargo run --bin hypervisor

# Cutover topology commands (tmux-backed, vfkit-first)
dev-control-plane:
    ./scripts/dev-vfkit.sh start-control

dev-runtime-plane:
    ./scripts/dev-vfkit.sh start-runtime

dev-all:
    ./scripts/dev-vfkit.sh start-all

dev-all-foreground:
    ./scripts/dev-vfkit.sh start-all-fg

# Canonical local startup helper (build UI release assets, then start cutover stack)
dev:
    @echo "Building UI (release) then starting vfkit cutover stack..."
    @just local-build-ui
    @just dev-all

dev-status:
    ./scripts/dev-vfkit.sh status

# Build the vfkit guest NixOS system derivation with streaming logs.
vfkit-nixos-build:
    CHOIR_WORKSPACE_ROOT="$(pwd)" CHOIR_VFKIT_GUEST_STATE_ROOT="$HOME/.local/share/choiros/vfkit/guest" nix build -L --impure "path:$(pwd)#nixosConfigurations.choiros-vfkit-user.config.system.build.toplevel"

# Run vfkit guest in foreground (streams nix build + runner output).
vfkit-vm-runner USER_ID="public":
    CHOIR_VFKIT_USER_ID="{{USER_ID}}" CHOIR_WORKSPACE_ROOT="$(pwd)" CHOIR_VFKIT_GUEST_STATE_ROOT="$HOME/.local/share/choiros/vfkit/guest" nix run -L --impure "path:$(pwd)#nixosConfigurations.choiros-vfkit-user.config.microvm.runner.vfkit"

# Rebuild and run vfkit guest with streaming Nix output (explicit alias).
vfkit-vm-rebuild USER_ID="public":
    @just vfkit-vm-runner USER_ID="{{USER_ID}}"

# Ensure the vfkit live runtime is running for a given user id (default: public).
vfkit-runtime-live USER_ID="public":
    if [ ! -x ./target/debug/vfkit-runtime-ctl ]; then cargo build -p hypervisor --bin vfkit-runtime-ctl; fi
    ./target/debug/vfkit-runtime-ctl ensure --user-id "{{USER_ID}}" --runtime live --role live --port 8080

# Reset stale vfkit VMs/tunnels/pid files.
vfkit-reset:
    ./scripts/ops/vfkit-reset.sh

# Attach to the vfkit guest VM over SSH (auto-discovers guest endpoint).
vfkit-guest-shell:
    ./scripts/ops/vfkit-guest-ssh.sh

# Open btop in the vfkit guest VM.
vfkit-guest-btop:
    ./scripts/ops/vfkit-guest-ssh.sh -- btop

# Shortcut: open btop in the vfkit guest VM.
btop:
    ./scripts/ops/vfkit-guest-ssh.sh -- btop

dev-attach:
    ./scripts/dev-vfkit.sh attach

stop-all:
    ./scripts/dev-vfkit.sh stop

# Local cutover readiness check
# Run `just cutover-status --probe-builder` for a live aarch64-linux builder build probe.
cutover-status *ARGS:
    ./scripts/ops/check-local-cutover-status.sh {{ARGS}}

# Local Linux builder bootstrap (required for local aarch64-linux derivation builds)
# UTM path: starts named VM, resolves guest IP, bootstraps Nix, wires /etc/nix builder config.
builder-bootstrap-utm VM:
    ./scripts/ops/bootstrap-local-linux-builder.sh --utm-vm "{{VM}}"

# Generic SSH path for any Linux VM/host builder.
builder-bootstrap-ssh HOST PORT USER:
    ./scripts/ops/bootstrap-local-linux-builder.sh --ssh-host "{{HOST}}" --ssh-port "{{PORT}}" --ssh-user "{{USER}}"

# Build the Dioxus WASM frontend (debug) into target/dx/dioxus-desktop/debug/web/public
build-ui:
    cd dioxus-desktop && dx build

# Build the Dioxus WASM frontend (release)
build-ui-release:
    cd dioxus-desktop && dx build --release

# Build release UI then run hypervisor (vfkit runtime topology) on port 9090.
dev-full: local-build-ui build-vfkit-ctl
    cd hypervisor && FRONTEND_DIST="$(pwd)/../dioxus-desktop/target/dx/dioxus-desktop/release/web/public" SQLX_OFFLINE=true HYPERVISOR_DATABASE_URL="sqlite:../data/hypervisor.db" SANDBOX_VFKIT_CTL="$(pwd)/../target/debug/vfkit-runtime-ctl" cargo run --bin hypervisor

# Stop/kill running development processes
stop:
    @just stop-all

# Build commands
# Build all packages in release mode
build:
    cargo build --release

# Build frontend + backend for production
# Frontend builds to dist/, then copied to sandbox/static/
# Backend builds in sandbox/
build-sandbox:
    cd dioxus-desktop && dx build --release
    mkdir -p sandbox/static
    cp -r dioxus-desktop/target/dx/dioxus-desktop/release/web/public/* sandbox/static/
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

# Canonical vfkit proof run:
# 1) Ensure `just dev` is running.
# 2) Runs single-user cutover proof (live + branch + terminal NixOS evidence) with video artifacts.
test-e2e-vfkit-proof:
    if [ "${CHOIR_E2E_VFKIT_RESET:-true}" = "true" ]; then ./scripts/ops/vfkit-reset.sh; fi
    cd tests/playwright && CHOIR_E2E_EXPECT_NIXOS=1 npx playwright test --config=playwright.config.ts --project=hypervisor vfkit-cutover-proof.spec.ts --workers=1

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

# Deployment (current AWS path)
# Uses AWS SSM + host-side nixos-rebuild switch flow.
# Required env: DEPLOY_INSTANCE_ID (or EC2_INSTANCE_ID)
deploy-aws-ssm:
    ./scripts/deploy/aws-ssm-deploy.sh

# Verify FlakeHub cache configuration/auth on current host.
cache-check:
    ./scripts/ops/check-flakehub-cache.sh

# Build canonical release manifest (flake outputs only).
release-manifest:
    ./scripts/ops/build-release-manifest.sh

# Legacy alias retained to avoid silent stale usage.
deploy-ec2:
    @echo "ERROR: 'deploy-ec2' is deprecated and removed."
    @echo "Use: just deploy-aws-ssm"
    @exit 1

# Grind host
# Run canonical pre-push checks directly on grind.
grind-check:
    ssh -i "$HOME/.ssh/choiros-grind.pem" -o StrictHostKeyChecking=accept-new root@18.212.170.200 'set -e; cd /opt/choiros/workspace; git status --short --branch; nix --extra-experimental-features nix-command --extra-experimental-features flakes develop ./hypervisor --command cargo check -p hypervisor; nix --extra-experimental-features nix-command --extra-experimental-features flakes develop ./sandbox --command cargo check -p sandbox; git status --short --branch'

# Build a deterministic release manifest from current commit
release-build-manifest:
    ./scripts/ops/build-release-manifest.sh

# Promote exact grind closures to prod
release-promote GRIND PROD:
    ./scripts/ops/promote-grind-to-prod.sh --grind {{GRIND}} --prod {{PROD}}

# Capture host state for drift debugging
ops-host-snapshot OUT:
    ./scripts/ops/host-state-snapshot.sh --output {{OUT}}

# System Monitor
# View actor network as ASCII diagram
monitor:
    node skills/system-monitor/scripts/system-monitor.js

# Save actor network report to file
monitor-save:
    node skills/system-monitor/scripts/system-monitor.js --save

# Compact actor network view
monitor-compact:
    node skills/system-monitor/scripts/system-monitor.js --compact

# Dashboard (Native ChoirOS - Coming Soon)
# Note: Native dashboard will be integrated into ChoirOS UI
# For now, use the UI at http://localhost:3000
