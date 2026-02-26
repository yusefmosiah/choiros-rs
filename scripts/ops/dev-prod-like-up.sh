#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOG_DIR="${CHOIR_DEV_LOG_DIR:-/tmp/choiros-devprod}"
DIST_DIR="${CHOIR_FRONTEND_DIST:-$ROOT_DIR/dioxus-desktop/target/dx/dioxus-desktop/release/web/public}"
SANDBOX_LOG="$LOG_DIR/sandbox.log"
HYPERVISOR_LOG="$LOG_DIR/hypervisor.log"

mkdir -p "$LOG_DIR"

echo "[dev-prod-like] stopping existing local services"
cd "$ROOT_DIR"
just stop >/dev/null 2>&1 || true
pkill -f "/target/debug/sandbox" >/dev/null 2>&1 || true
pkill -f "/target/debug/hypervisor" >/dev/null 2>&1 || true

echo "[dev-prod-like] building static UI assets (release)"
cd "$ROOT_DIR/dioxus-desktop"
dx build --release

if [[ ! -f "$DIST_DIR/index.html" ]]; then
  echo "[dev-prod-like] ERROR: missing frontend index at $DIST_DIR/index.html" >&2
  exit 1
fi

echo "[dev-prod-like] building backend binaries"
cd "$ROOT_DIR"
cargo build -p sandbox -p hypervisor

echo "[dev-prod-like] starting sandbox"
nohup env \
  FRONTEND_DIST="$DIST_DIR" \
  DATABASE_URL="sqlite:$ROOT_DIR/data/events.db" \
  SQLX_OFFLINE=true \
  RUST_LOG="${RUST_LOG:-info}" \
  "$ROOT_DIR/target/debug/sandbox" >"$SANDBOX_LOG" 2>&1 &
SANDBOX_PID=$!

echo "[dev-prod-like] waiting for sandbox on :8080"
for _ in $(seq 1 60); do
  if curl -fsS http://127.0.0.1:8080/health >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
if ! curl -fsS http://127.0.0.1:8080/health >/dev/null 2>&1; then
  echo "[dev-prod-like] ERROR: sandbox failed to become healthy" >&2
  tail -n 120 "$SANDBOX_LOG" >&2 || true
  exit 1
fi

echo "[dev-prod-like] starting hypervisor"
nohup env \
  FRONTEND_DIST="$DIST_DIR" \
  HYPERVISOR_DATABASE_URL="sqlite:$ROOT_DIR/data/hypervisor.db" \
  SQLX_OFFLINE=true \
  RUST_LOG="${RUST_LOG:-info}" \
  "$ROOT_DIR/target/debug/hypervisor" >"$HYPERVISOR_LOG" 2>&1 &
HYPERVISOR_PID=$!

echo "[dev-prod-like] waiting for hypervisor on :9090"
for _ in $(seq 1 60); do
  if curl -fsSI http://127.0.0.1:9090/login >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
if ! curl -fsSI http://127.0.0.1:9090/login >/dev/null 2>&1; then
  echo "[dev-prod-like] ERROR: hypervisor failed to become ready" >&2
  tail -n 120 "$HYPERVISOR_LOG" >&2 || true
  exit 1
fi

echo "[dev-prod-like] validating frontend asset path contract"
INDEX_HTML="$(curl -fsS http://127.0.0.1:9090/register)"
# Accept both modern hashed assets path and legacy wasm loader path.
ASSET_PATH="$(printf '%s' "$INDEX_HTML" | grep -Eo '/(\./)?(assets|wasm)/[^"]+\.js' | head -n 1 || true)"
ASSET_PATH="$(printf '%s' "$ASSET_PATH" | sed 's#^/\\./#/#')"
if [[ -z "$ASSET_PATH" ]]; then
  echo "[dev-prod-like] ERROR: could not extract JS asset path from /register" >&2
  echo "$INDEX_HTML" | sed -n '1,80p' >&2
  exit 1
fi
if ! curl -fsSI "http://127.0.0.1:9090$ASSET_PATH" >/dev/null 2>&1; then
  echo "[dev-prod-like] ERROR: frontend asset not reachable at $ASSET_PATH" >&2
  exit 1
fi

echo "[dev-prod-like] ready"
echo "  sandbox pid:    $SANDBOX_PID"
echo "  hypervisor pid: $HYPERVISOR_PID"
echo "  frontend dist:  $DIST_DIR"
echo "  logs:           $LOG_DIR"
echo "  url:            http://localhost:9090"
