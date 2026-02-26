#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SESSION_NAME="choiros-cutover"
LOG_DIR="${HOME}/.local/share/choiros/cutover/logs"

FRONTEND_DIST="${ROOT_DIR}/dioxus-desktop/target/dx/dioxus-desktop/release/web/public"
SANDBOX_CMD="cd ${ROOT_DIR}/sandbox && FRONTEND_DIST='${FRONTEND_DIST}' DATABASE_URL='sqlite:../data/events.db' SQLX_OFFLINE=true CARGO_INCREMENTAL=0 cargo run"
HYPERVISOR_CMD="cd ${ROOT_DIR}/hypervisor && FRONTEND_DIST='${FRONTEND_DIST}' SQLX_OFFLINE=true HYPERVISOR_DATABASE_URL='sqlite:../data/hypervisor.db' cargo run"

mkdir -p "$LOG_DIR"

has_session() {
  tmux has-session -t "$SESSION_NAME" 2>/dev/null
}

has_window() {
  local window="$1"
  tmux list-windows -t "$SESSION_NAME" -F '#{window_name}' 2>/dev/null | rg -x "$window" >/dev/null 2>&1
}

ensure_session() {
  if ! has_session; then
    tmux new-session -d -s "$SESSION_NAME" -n shell -c "$ROOT_DIR"
    tmux send-keys -t "$SESSION_NAME:shell" "echo 'choiros cutover session ready'" C-m
  fi
}

ensure_ui_dist() {
  if [[ ! -d "$FRONTEND_DIST" ]]; then
    echo "UI dist missing at $FRONTEND_DIST"
    echo "Run: just local-build-ui"
    exit 1
  fi
}

start_window() {
  local window="$1"
  local cmd="$2"
  local log_file="$3"

  if has_window "$window"; then
    echo "window '$window' already running"
    return
  fi

  tmux new-window -t "$SESSION_NAME" -n "$window" -c "$ROOT_DIR"
  tmux send-keys -t "$SESSION_NAME:$window" "set -o pipefail; $cmd 2>&1 | tee '$LOG_DIR/$log_file'" C-m
}

start_runtime() {
  ensure_session
  ensure_ui_dist
  start_window "sandbox" "$SANDBOX_CMD" "sandbox.log"
}

start_control() {
  ensure_session
  ensure_ui_dist
  start_window "hypervisor" "$HYPERVISOR_CMD" "hypervisor.log"
}

start_all() {
  start_runtime
  start_control
}

start_all_foreground() {
  start_all
  touch "$LOG_DIR/sandbox.log" "$LOG_DIR/hypervisor.log"

  cleanup() {
    stop_all
  }
  trap cleanup INT TERM EXIT

  echo "streaming logs (Ctrl+C to stop all)"
  set +e
  tail -n +1 -f "$LOG_DIR/sandbox.log" "$LOG_DIR/hypervisor.log"
  local tail_status=$?
  set -e
  if [[ "$tail_status" -ne 0 && "$tail_status" -ne 130 ]]; then
    return "$tail_status"
  fi
}

stop_all() {
  if has_session; then
    tmux kill-session -t "$SESSION_NAME"
  fi

  pkill -f "/target/debug/sandbox" 2>/dev/null || true
  pkill -f "/target/debug/hypervisor" 2>/dev/null || true
  pkill -f "cargo run.*sandbox" 2>/dev/null || true
  pkill -f "cargo run.*hypervisor" 2>/dev/null || true
}

status() {
  if has_session; then
    echo "session: $SESSION_NAME (running)"
    tmux list-windows -t "$SESSION_NAME" -F '  - #W'
  else
    echo "session: $SESSION_NAME (stopped)"
  fi

  if curl -fsS http://127.0.0.1:8080/health >/dev/null 2>&1; then
    echo "sandbox: healthy (http://127.0.0.1:8080/health)"
  else
    echo "sandbox: down"
  fi

  if curl -fsS http://127.0.0.1:9090/login >/dev/null 2>&1; then
    echo "hypervisor: healthy (http://127.0.0.1:9090/login)"
  else
    echo "hypervisor: down"
  fi

  echo "logs: $LOG_DIR"
}

attach() {
  if ! has_session; then
    echo "session not running: $SESSION_NAME"
    exit 1
  fi
  tmux attach -t "$SESSION_NAME"
}

usage() {
  cat <<USAGE
Usage: $0 <command>

Commands:
  start-control   Start control-plane process(es) in tmux
  start-runtime   Start runtime-plane process(es) in tmux
  start-all       Start control + runtime planes in tmux
  start-all-fg    Start control + runtime planes in foreground
  stop            Stop tmux session and local sandbox/hypervisor processes
  status          Show tmux/process health state
  attach          Attach to tmux session
USAGE
}

case "${1:-}" in
  start-control)
    start_control
    ;;
  start-runtime)
    start_runtime
    ;;
  start-all)
    start_all
    ;;
  start-all-fg)
    start_all_foreground
    ;;
  stop)
    stop_all
    ;;
  status)
    status
    ;;
  attach)
    attach
    ;;
  *)
    usage
    exit 1
    ;;
esac
