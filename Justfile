# Justfile - Task runner for ChoirOS
# This file contains development, build, testing, and deployment commands

# Default recipe - list all available tasks
default:
    @just --list

# Development commands
# Run backend API server with local database
dev-sandbox:
    export DATABASE_URL="./data/events.db" && cd sandbox && cargo run

# Run hypervisor component
dev-hypervisor:
    cd hypervisor && cargo run

# Run frontend development server (Dioxus:3000)
dev-ui:
    cd sandbox-ui && dx serve --port 3000

# Stop/kill running development processes
stop:
    @echo "Stopping ChoirOS development processes..."
    @pkill -9 -f "cargo run -p sandbox" 2>/dev/null || true
    @pkill -9 -f "dx serve" 2>/dev/null || true
    @pkill -9 -f "sandbox" 2>/dev/null || true
    @echo "✓ All processes stopped"

# Build commands
# Build all packages in release mode
build:
    cargo build --release

# Build frontend + backend for production
# Frontend builds to dist/, then copied to sandbox/static/
# Backend builds in sandbox/
build-sandbox:
    cd sandbox-ui && dx build --release
    cp -r sandbox-ui/dist/* sandbox/static/
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

# Create new migration file with given name
new-migration NAME:
    cd sandbox && cargo sqlx migrate add {{NAME}}

# Docker
# Build Docker image for choir-sandbox
docker-build:
    docker build -t choir-sandbox:latest ./sandbox

# Run Docker container with port mapping and volume
docker-run:
    docker run -p 8080:8080 -v ./data:/data choir-sandbox:latest

# Deployment
# Deploy to EC2 instance at 3.83.131.245 (push code + build)
deploy-ec2:
    rsync -avz --delete ./ ubuntu@3.83.131.245:~/choiros-rs/
    ssh ubuntu@3.83.131.245 'cd ~/choiros-rs && just build-sandbox'

# Actorcode
# Execute actorcode script with given arguments
actorcode *ARGS:
    node skills/actorcode/scripts/actorcode.js {{ARGS}}

# Research tasks (non-blocking)
# Launch research task from template(s)
research *TEMPLATES:
    node skills/actorcode/scripts/research-launch.js {{TEMPLATES}}

# Monitor research sessions (collects findings in background)
research-monitor *SESSIONS:
    node skills/actorcode/scripts/research-monitor.js {{SESSIONS}}

# Research status - show active/completed research tasks
research-status *ARGS:
    node skills/actorcode/scripts/research-status.js {{ARGS}}

# Findings database commands
# Query findings database (stats, export, etc.)
findings *ARGS:
    node skills/actorcode/scripts/findings.js {{ARGS}}

# Research dashboard (tmux) - live findings view
research-dashboard CMD="compact":
    python skills/actorcode/scripts/research-dashboard.py {{CMD}}

# Open web dashboard in browser
research-web:
    open skills/actorcode/dashboard.html

# Start findings API server for web dashboard
findings-server:
    node skills/actorcode/scripts/findings-server.js

# Cleanup old research sessions
research-cleanup *ARGS:
    node skills/actorcode/scripts/cleanup-sessions.js {{ARGS}}

# Run diagnostics on research system
research-diagnose:
    node skills/actorcode/scripts/diagnose.js

# Fix findings with worktree isolation
fix-findings *ARGS:
    node skills/actorcode/scripts/fix-findings.js {{ARGS}}

# Check test hygiene before merging
check-test-hygiene:
    node skills/actorcode/scripts/check-test-hygiene.js

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

# Dashboard
# Open new dashboard with network/timeline/hierarchy views
dashboard:
    open skills/actorcode/dashboard/index.html

# Serve dashboard via HTTP (for development)
dashboard-serve:
    python -m http.server 8766 --directory skills/actorcode/dashboard

# NixOS Research
# Spawn supervisor to research Nix/NixOS for Rust + EC2
nixos-research:
    node skills/actorcode/scripts/nixos-research-supervisor.cjs

# Docs Upgrade
# Execute the docs coherence runbook fixes
docs-upgrade:
    node skills/actorcode/scripts/docs-upgrade-supervisor.cjs
