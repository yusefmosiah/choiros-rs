#!/usr/bin/env bash
set -euo pipefail

SCRIPT_NAME="$(basename "$0")"

usage() {
  cat <<'USAGE'
Usage:
  bootstrap-local-linux-builder.sh [options]

Options:
  --utm-vm NAME           UTM VM name/UUID to start and query for guest IP.
  --ssh-host HOST         SSH host for the Linux builder (default: 127.0.0.1).
  --ssh-port PORT         SSH port (default: 2222, or 22 when --utm-vm is used and --ssh-port is omitted).
  --ssh-user USER         SSH username (default: root).
  --ssh-key PATH          SSH private key path (default: ~/.ssh/id_ed25519).
  --max-jobs N            Builder max-jobs in /etc/nix/machines (default: 8).
  --skip-remote-bootstrap Skip installing/checking Nix on the remote VM.
  --skip-local-config     Skip local /etc/nix config updates.
  --help                  Show this help.

Notes:
  - Requires sudo for local /etc/nix writes and daemon restart.
  - UTM automation uses /Applications/UTM.app/Contents/MacOS/utmctl.
  - utmctl cannot create new VMs; create/import one VM template first.
USAGE
}

log() {
  printf '[builder] %s\n' "$*"
}

die() {
  printf '[builder] ERROR: %s\n' "$*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

UTM_VM=""
SSH_HOST="127.0.0.1"
SSH_PORT="2222"
SSH_USER="root"
SSH_KEY="${HOME}/.ssh/id_ed25519"
MAX_JOBS="8"
SKIP_REMOTE_BOOTSTRAP="false"
SKIP_LOCAL_CONFIG="false"
PORT_EXPLICIT="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --utm-vm)
      UTM_VM="${2:-}"
      shift 2
      ;;
    --ssh-host)
      SSH_HOST="${2:-}"
      shift 2
      ;;
    --ssh-port)
      SSH_PORT="${2:-}"
      PORT_EXPLICIT="true"
      shift 2
      ;;
    --ssh-user)
      SSH_USER="${2:-}"
      shift 2
      ;;
    --ssh-key)
      SSH_KEY="${2:-}"
      shift 2
      ;;
    --max-jobs)
      MAX_JOBS="${2:-}"
      shift 2
      ;;
    --skip-remote-bootstrap)
      SKIP_REMOTE_BOOTSTRAP="true"
      shift
      ;;
    --skip-local-config)
      SKIP_LOCAL_CONFIG="true"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      die "unknown arg: $1"
      ;;
  esac
done

[[ -n "$SSH_HOST" ]] || die "--ssh-host cannot be empty"
[[ -n "$SSH_PORT" ]] || die "--ssh-port cannot be empty"
[[ -n "$SSH_USER" ]] || die "--ssh-user cannot be empty"
[[ -n "$SSH_KEY" ]] || die "--ssh-key cannot be empty"
[[ -f "$SSH_KEY" ]] || die "ssh key not found: $SSH_KEY"

require_cmd ssh
require_cmd nix
require_cmd rg
require_cmd awk

UTMCTL="/Applications/UTM.app/Contents/MacOS/utmctl"
if [[ -n "$UTM_VM" ]]; then
  [[ -x "$UTMCTL" ]] || die "utmctl not found at $UTMCTL"
  if [[ "$PORT_EXPLICIT" == "false" ]]; then
    SSH_PORT="22"
  fi
fi

resolve_utm_ipv4() {
  local vm="$1"
  local ip=""
  for _ in {1..180}; do
    local out
    out="$("$UTMCTL" ip-address "$vm" 2>/dev/null || true)"
    ip="$(printf '%s\n' "$out" | rg -o '([0-9]{1,3}\.){3}[0-9]{1,3}' | head -n 1 || true)"
    if [[ -n "$ip" ]]; then
      printf '%s' "$ip"
      return 0
    fi
    sleep 1
  done
  return 1
}

if [[ -n "$UTM_VM" ]]; then
  log "starting UTM VM '$UTM_VM' (or ensuring it is running)"
  "$UTMCTL" start --hide "$UTM_VM" >/dev/null 2>&1 || true
  log "resolving UTM guest IP address"
  SSH_HOST="$(resolve_utm_ipv4 "$UTM_VM")" || die "failed to resolve UTM guest IP for '$UTM_VM'"
  log "using UTM guest SSH target ${SSH_USER}@${SSH_HOST}:${SSH_PORT}"
fi

SSH_DEST="${SSH_USER}@${SSH_HOST}"
SSH_OPTS=(
  -o BatchMode=yes
  -o StrictHostKeyChecking=accept-new
  -o ConnectTimeout=5
  -p "$SSH_PORT"
  -i "$SSH_KEY"
)

wait_for_ssh() {
  for _ in {1..120}; do
    if ssh "${SSH_OPTS[@]}" "$SSH_DEST" true >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

log "waiting for SSH on ${SSH_DEST}:${SSH_PORT}"
wait_for_ssh || die "SSH is not reachable at ${SSH_DEST}:${SSH_PORT}"

if [[ "$SKIP_REMOTE_BOOTSTRAP" != "true" ]]; then
  log "bootstrapping remote Linux VM (Nix install + daemon enable)"
  if [[ "$SSH_USER" == "root" ]]; then
    ssh "${SSH_OPTS[@]}" "$SSH_DEST" "bash -s" <<'REMOTE'
set -euo pipefail

if ! command -v curl >/dev/null 2>&1; then
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y curl xz-utils
  elif command -v dnf >/dev/null 2>&1; then
    dnf install -y curl xz
  fi
fi

if ! command -v nix >/dev/null 2>&1; then
  curl -fsSL https://install.determinate.systems/nix | sh -s -- install --determinate
fi

if command -v systemctl >/dev/null 2>&1; then
  systemctl enable --now nix-daemon.service || true
fi
REMOTE
  else
    ssh "${SSH_OPTS[@]}" "$SSH_DEST" "sudo bash -s" <<'REMOTE'
set -euo pipefail

if ! command -v curl >/dev/null 2>&1; then
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y curl xz-utils
  elif command -v dnf >/dev/null 2>&1; then
    dnf install -y curl xz
  fi
fi

if ! command -v nix >/dev/null 2>&1; then
  curl -fsSL https://install.determinate.systems/nix | sh -s -- install --determinate
fi

if command -v systemctl >/dev/null 2>&1; then
  systemctl enable --now nix-daemon.service || true
fi
REMOTE
  fi
fi

BUILDER_LINE="ssh://${SSH_USER}@${SSH_HOST}:${SSH_PORT} aarch64-linux ${SSH_KEY} ${MAX_JOBS} 1 big-parallel,benchmark"

if [[ "$SKIP_LOCAL_CONFIG" != "true" ]]; then
  log "updating local /etc/nix/machines and /etc/nix/nix.custom.conf (sudo required)"
  sudo touch /etc/nix/machines
  if ! sudo rg -Fqx -- "$BUILDER_LINE" /etc/nix/machines >/dev/null 2>&1; then
    printf '%s\n' "$BUILDER_LINE" | sudo tee -a /etc/nix/machines >/dev/null
  fi

  tmp="$(mktemp)"
  tmp_clean="${tmp}.clean"
  sudo cat /etc/nix/nix.custom.conf > "$tmp"
  awk '
    /^# BEGIN CHOIR LOCAL LINUX BUILDER$/ { skip=1; next }
    /^# END CHOIR LOCAL LINUX BUILDER$/   { skip=0; next }
    !skip { print }
  ' "$tmp" > "$tmp_clean"
  cat >> "$tmp_clean" <<EOF

# BEGIN CHOIR LOCAL LINUX BUILDER
trusted-users = root ${USER}
extra-platforms = aarch64-linux x86_64-linux x86_64-darwin
builders = @/etc/nix/machines
builders-use-substitutes = true
# END CHOIR LOCAL LINUX BUILDER
EOF
  sudo cp "$tmp_clean" /etc/nix/nix.custom.conf
  rm -f "$tmp" "$tmp_clean"

  if launchctl print system/systems.determinate.nix-daemon >/dev/null 2>&1; then
    sudo launchctl kickstart -k system/systems.determinate.nix-daemon
  elif launchctl print system/org.nixos.nix-daemon >/dev/null 2>&1; then
    sudo launchctl kickstart -k system/org.nixos.nix-daemon
  else
    die "could not find nix-daemon launchctl label"
  fi
fi

log "verifying local nix config"
nix config show | rg "^(trusted-users|extra-platforms|builders|builders-use-substitutes)"

log "running remote aarch64-linux builder probe"
nix build --impure --max-jobs 0 \
  --builders "$BUILDER_LINE" \
  --expr 'let pkgs = import (builtins.getFlake "flake:nixpkgs").outPath { system = "aarch64-linux"; }; in pkgs.runCommand "linux-builder-probe" {} "echo ok > $out"'

log "builder probe passed"
log "you can now retry: just test-e2e-vfkit-proof"
