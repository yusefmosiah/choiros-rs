#!/bin/bash
# dev-workflow.sh - Tmux-based development workflow manager
# Usage: ./dev-workflow.sh [start|stop|attach|status]

set -e

SESSION_NAME="choiros-dev"
LOG_DIR="${HOME}/.local/share/choiros/logs"
PID_DIR="${HOME}/.local/share/choiros/pids"

# Ensure directories exist
mkdir -p "$LOG_DIR" "$PID_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[$(date +%H:%M:%S)]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[$(date +%H:%M:%S)] WARNING:${NC} $1"
}

error() {
    echo -e "${RED}[$(date +%H:%M:%S)] ERROR:${NC} $1"
}

# Start all services
start_workflow() {
    log "Starting ChoirOS development workflow..."

    # Check if already running
    if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
        warn "Session $SESSION_NAME already exists. Use 'attach' to connect."
        return 1
    fi

    # Create new tmux session (detached)
    tmux new-session -d -s "$SESSION_NAME" -n "editor" -c "$HOME/choiros-rs"

    # Window 1: Sandbox (API server)
    tmux new-window -t "$SESSION_NAME:1" -n "sandbox" -c "$HOME/choiros-rs"
    tmux send-keys -t "$SESSION_NAME:1" "echo '[SANDBOX] Starting Actix server...' && just dev-sandbox 2>&1 | tee $LOG_DIR/sandbox.log" C-m

    # Window 2: Dioxus UI dev server
    tmux new-window -t "$SESSION_NAME:2" -n "ui" -c "$HOME/choiros-rs"
    tmux send-keys -t "$SESSION_NAME:2" "echo '[UI] Starting Dioxus dev server...' && just dev-ui 2>&1 | tee $LOG_DIR/ui.log" C-m

    # Window 3: File watcher (auto-test on change)
    tmux new-window -t "$SESSION_NAME:3" -n "watcher" -c "$HOME/choiros-rs"
    tmux send-keys -t "$SESSION_NAME:3" "echo '[WATCHER] Starting file watcher...' && cargo watch -x 'test --lib' -w shared-types -w sandbox/src 2>&1 | tee $LOG_DIR/watcher.log" C-m

    # Window 4: Agent Browser (E2E tests - initially idle)
    tmux new-window -t "$SESSION_NAME:4" -n "e2e" -c "$HOME/choiros-rs"
    tmux send-keys -t "$SESSION_NAME:4" "echo '[E2E] Ready for browser tests. Run: agent-browser test --url http://localhost:3000'" C-m

    # Window 5: Logs monitor
    tmux new-window -t "$SESSION_NAME:5" -n "logs" -c "$HOME"
    tmux send-keys -t "$SESSION_NAME:5" "echo '[LOGS] Monitoring all services...' && multitail -f $LOG_DIR/sandbox.log $LOG_DIR/ui.log $LOG_DIR/watcher.log" C-m

    # Save PIDs for status checking
    echo "$(tmux list-sessions -F '#{session_name}:#{session_attached}' | grep "$SESSION_NAME")" > "$PID_DIR/session.info"

    log "✅ Development workflow started!"
    log "   Sandbox API: http://localhost:8080"
    log "   Dioxus UI:   http://localhost:3000"
    log ""
    log "Commands:"
    log "   ./dev-workflow.sh attach  - Attach to tmux session"
    log "   ./dev-workflow.sh status  - Check service status"
    log "   ./dev-workflow.sh stop    - Stop all services"

    # Create checkpoint marker
    echo "$(date -Iseconds)" > "$PID_DIR/last_start.txt"
}

# Stop all services
stop_workflow() {
    log "Stopping ChoirOS development workflow..."

    if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
        warn "No active session found"
        return 1
    fi

    # Graceful shutdown
    tmux send-keys -t "$SESSION_NAME:1" C-c
    tmux send-keys -t "$SESSION_NAME:2" C-c
    tmux send-keys -t "$SESSION_NAME:3" C-c

    sleep 2

    # Kill session
    tmux kill-session -t "$SESSION_NAME"

    log "✅ Development workflow stopped"
}

# Attach to tmux session
attach_workflow() {
    if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
        error "No active session. Run: ./dev-workflow.sh start"
        return 1
    fi

    log "Attaching to tmux session (Ctrl+B D to detach)..."
    tmux attach-session -t "$SESSION_NAME"
}

# Check status
status_workflow() {
    if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
        log "✅ ChoirOS dev workflow is RUNNING"
        log ""
        tmux list-windows -t "$SESSION_NAME" -F '  #I: #W (#{window_active}active)'
        log ""
        log "Logs: $LOG_DIR"

        # Check if services are responding
        if curl -s http://localhost:8080/health > /dev/null 2>&1; then
            log "  ✅ Sandbox API: http://localhost:8080 (healthy)"
        else
            warn "  ⚠️  Sandbox API: http://localhost:8080 (not responding)"
        fi

        if curl -s http://localhost:3000 > /dev/null 2>&1; then
            log "  ✅ Dioxus UI:   http://localhost:3000 (running)"
        else
            warn "  ⚠️  Dioxus UI:   http://localhost:3000 (not responding)"
        fi
    else
        error "❌ ChoirOS dev workflow is NOT RUNNING"
        log "   Start with: ./dev-workflow.sh start"
    fi
}

# Run E2E tests
run_e2e() {
    if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
        error "Dev workflow not running. Start it first."
        return 1
    fi

    log "Running E2E tests..."
    tmux send-keys -t "$SESSION_NAME:4" C-u
    tmux send-keys -t "$SESSION_NAME:4" "agent-browser test --url http://localhost:3000 --test-dir ./tests/e2e 2>&1 | tee $LOG_DIR/e2e-$(date +%Y%m%d-%H%M%S).log" C-m
    log "E2E tests started in window 4 (e2e)"
}

# Create checkpoint (git commit + tag)
create_checkpoint() {
    local checkpoint_name="checkpoint-$(date +%Y%m%d-%H%M%S)"

    log "Creating checkpoint: $checkpoint_name"

    # Git operations
    if [ -d ".git" ]; then
        git add -A
        git commit -m "Checkpoint: $checkpoint_name - working state" || true
        git tag "$checkpoint_name"
        log "✅ Checkpoint created: $checkpoint_name"
        log "   Rollback: git reset --hard $checkpoint_name"
    else
        warn "Not a git repo. Initialize with: git init"
    fi

    # Also save to file
    echo "$checkpoint_name" > "$PID_DIR/latest_checkpoint.txt"
}

# Rollback to last checkpoint
rollback() {
    local checkpoint=${1:-$(cat "$PID_DIR/latest_checkpoint.txt" 2>/dev/null || echo "")}

    if [ -z "$checkpoint" ]; then
        error "No checkpoint specified and no latest checkpoint found"
        log "Usage: ./dev-workflow.sh rollback [checkpoint-name]"
        return 1
    fi

    log "🔄 Rolling back to: $checkpoint"

    # Stop workflow
    stop_workflow

    # Git rollback
    git reset --hard "$checkpoint"

    # Restart
    start_workflow

    log "✅ Rolled back and restarted"
}

# Main command handler
case "${1:-}" in
    start)
        start_workflow
        ;;
    stop)
        stop_workflow
        ;;
    restart)
        stop_workflow
        sleep 2
        start_workflow
        ;;
    attach)
        attach_workflow
        ;;
    status)
        status_workflow
        ;;
    e2e)
        run_e2e
        ;;
    checkpoint)
        create_checkpoint
        ;;
    rollback)
        rollback "$2"
        ;;
    *)
        echo "ChoirOS Development Workflow Manager"
        echo ""
        echo "Usage: $0 [command]"
        echo ""
        echo "Commands:"
        echo "  start       - Start all services (tmux session)"
        echo "  stop        - Stop all services"
        echo "  restart     - Restart all services"
        echo "  attach      - Attach to tmux session"
        echo "  status      - Check service status"
        echo "  e2e         - Run E2E browser tests"
        echo "  checkpoint  - Create git checkpoint (rollback point)"
        echo "  rollback    - Rollback to last checkpoint"
        echo ""
        echo "Services:"
        echo "  - Sandbox API: http://localhost:8080"
        echo "  - Dioxus UI:   http://localhost:3000"
        exit 1
        ;;
esac
