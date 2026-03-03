#!/usr/bin/env bash
set -euo pipefail

# Runs from local/CI.
# Sends scripts/deploy/host-switch.sh to target OVH/NixOS host over SSH.

DEPLOY_HOST="${DEPLOY_HOST:-}"
DEPLOY_USER="${DEPLOY_USER:-root}"
DEPLOY_PORT="${DEPLOY_PORT:-22}"
SSH_KEY_PATH="${SSH_KEY_PATH:-}"

DEPLOY_SHA="${DEPLOY_SHA:-$(git rev-parse HEAD)}"
WORKDIR="${WORKDIR:-/opt/choiros/workspace}"
REPO_URL="${REPO_URL:-https://github.com/yusefmosiah/choiros-rs.git}"

SANDBOX_STORE_PATH="${SANDBOX_STORE_PATH:-}"
HYPERVISOR_STORE_PATH="${HYPERVISOR_STORE_PATH:-}"
DESKTOP_STORE_PATH="${DESKTOP_STORE_PATH:-}"
ALLOW_HOST_BUILD_FALLBACK="${ALLOW_HOST_BUILD_FALLBACK:-false}"

usage() {
  cat <<USAGE
Usage: DEPLOY_HOST=<host> [env ...] ./scripts/deploy/ovh-ssh-deploy.sh

Required env:
  DEPLOY_HOST                 Target host (IP or DNS)

Optional env:
  DEPLOY_USER                 SSH user (default: root)
  DEPLOY_PORT                 SSH port (default: 22)
  SSH_KEY_PATH                SSH private key path (optional)
  DEPLOY_SHA                  Git SHA to deploy (default: local HEAD)
  WORKDIR                     Repo path on host (default: /opt/choiros/workspace)
  REPO_URL                    Git repo URL (default: https://github.com/yusefmosiah/choiros-rs.git)
  SANDBOX_STORE_PATH          Prebuilt flake output path
  HYPERVISOR_STORE_PATH       Prebuilt flake output path
  DESKTOP_STORE_PATH          Prebuilt flake output path
  ALLOW_HOST_BUILD_FALLBACK   true/false (default: false)
USAGE
}

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "error: $cmd is required" >&2
    exit 2
  fi
}

if [[ -z "$DEPLOY_HOST" ]]; then
  usage
  echo "error: DEPLOY_HOST is required" >&2
  exit 2
fi

need_cmd ssh
need_cmd scp
need_cmd git

SCRIPT_PATH="$(dirname "$0")/host-switch.sh"
if [[ ! -f "$SCRIPT_PATH" ]]; then
  echo "error: missing ${SCRIPT_PATH}" >&2
  exit 2
fi

SSH_TARGET="${DEPLOY_USER}@${DEPLOY_HOST}"
SSH_ARGS=(-o StrictHostKeyChecking=accept-new -p "$DEPLOY_PORT")
if [[ -n "$SSH_KEY_PATH" ]]; then
  SSH_ARGS+=(-i "$SSH_KEY_PATH")
fi

REMOTE_SCRIPT="/tmp/choiros-host-switch.sh"

scp "${SSH_ARGS[@]}" "$SCRIPT_PATH" "$SSH_TARGET:$REMOTE_SCRIPT"

ssh "${SSH_ARGS[@]}" "$SSH_TARGET" \
  "chmod +x '$REMOTE_SCRIPT' && \
   RELEASE_SHA='$DEPLOY_SHA' \
   WORKDIR='$WORKDIR' \
   REPO_URL='$REPO_URL' \
   SANDBOX_STORE_PATH='$SANDBOX_STORE_PATH' \
   HYPERVISOR_STORE_PATH='$HYPERVISOR_STORE_PATH' \
   DESKTOP_STORE_PATH='$DESKTOP_STORE_PATH' \
   ALLOW_HOST_BUILD_FALLBACK='$ALLOW_HOST_BUILD_FALLBACK' \
   bash '$REMOTE_SCRIPT'"
