#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-}"
if [[ -z "$ACTION" ]]; then
  echo "usage: $0 <ensure|stop> --user-id <id> --runtime <name> --port <port> [--role <live|dev>] [--branch <branch>]" >&2
  exit 2
fi
shift

USER_ID=""
RUNTIME=""
PORT=""
ROLE=""
BRANCH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --user-id)
      USER_ID="${2:-}"
      shift 2
      ;;
    --runtime)
      RUNTIME="${2:-}"
      shift 2
      ;;
    --port)
      PORT="${2:-}"
      shift 2
      ;;
    --role)
      ROLE="${2:-}"
      shift 2
      ;;
    --branch)
      BRANCH="${2:-}"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$USER_ID" || -z "$RUNTIME" || -z "$PORT" ]]; then
  echo "missing required args; need --user-id, --runtime, --port" >&2
  exit 2
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEFAULT_CTL_BIN="$ROOT_DIR/target/debug/vfkit-runtime-ctl"
CTL_BIN="${CHOIR_VFKIT_CTL_BIN:-$DEFAULT_CTL_BIN}"

export CHOIR_RUNTIME_ACTION="$ACTION"
export CHOIR_RUNTIME_USER_ID="$USER_ID"
export CHOIR_RUNTIME_NAME="$RUNTIME"
export CHOIR_RUNTIME_PORT="$PORT"
export CHOIR_RUNTIME_ROLE="$ROLE"
export CHOIR_RUNTIME_BRANCH="$BRANCH"

run_external() {
  local cmd="$1"
  /bin/bash -lc "$cmd"
}

case "$ACTION" in
  ensure)
    if [[ -n "${CHOIR_VFKIT_ENSURE_CMD:-}" ]]; then
      run_external "$CHOIR_VFKIT_ENSURE_CMD"
      exit 0
    fi
    ctl_args=(ensure --user-id "$USER_ID" --runtime "$RUNTIME" --port "$PORT")
    if [[ -n "$ROLE" ]]; then
      ctl_args+=(--role "$ROLE")
    fi
    if [[ -n "$BRANCH" ]]; then
      ctl_args+=(--branch "$BRANCH")
    fi
    if [[ -x "$CTL_BIN" ]]; then
      exec "$CTL_BIN" "${ctl_args[@]}"
    fi
    echo "missing vfkit runtime control binary: $CTL_BIN" >&2
    echo "build it with: cargo build -p hypervisor --bin vfkit-runtime-ctl" >&2
    exit 1
    ;;
  stop)
    if [[ -n "${CHOIR_VFKIT_STOP_CMD:-}" ]]; then
      run_external "$CHOIR_VFKIT_STOP_CMD"
      exit 0
    fi
    ctl_args=(stop --user-id "$USER_ID" --runtime "$RUNTIME" --port "$PORT")
    if [[ -n "$ROLE" ]]; then
      ctl_args+=(--role "$ROLE")
    fi
    if [[ -n "$BRANCH" ]]; then
      ctl_args+=(--branch "$BRANCH")
    fi
    if [[ -x "$CTL_BIN" ]]; then
      exec "$CTL_BIN" "${ctl_args[@]}"
    fi
    echo "missing vfkit runtime control binary: $CTL_BIN" >&2
    echo "build it with: cargo build -p hypervisor --bin vfkit-runtime-ctl" >&2
    exit 1
    ;;
  *)
    echo "invalid action '$ACTION' (expected ensure|stop)" >&2
    exit 2
    ;;
esac
