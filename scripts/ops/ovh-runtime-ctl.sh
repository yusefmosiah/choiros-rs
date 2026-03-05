#!/run/current-system/sw/bin/bash
set -euo pipefail

# Cloud-hypervisor runtime controller for OVH bare metal.
# Called by the hypervisor's SandboxRegistry to manage sandbox microVMs.
#
# Usage: ovh-runtime-ctl.sh <ensure|stop> --user-id <id> --runtime <name> --port <port> [--role <live|dev>] [--branch <branch>]
#
# Environment:
#   CHOIR_WORKSPACE_ROOT  — path to choiros workspace (default: /opt/choiros/workspace)
#   CHOIR_VM_STATE_DIR    — VM state directory (default: /opt/choiros/vms/state)

WORKSPACE="${CHOIR_WORKSPACE_ROOT:-/opt/choiros/workspace}"
VM_STATE_DIR="${CHOIR_VM_STATE_DIR:-/opt/choiros/vms/state}"
BRIDGE="br-choiros"

# VM network assignments (static for now, dynamic per-user in Gate 3)
declare -A VM_IP=( [live]="10.0.0.10" [dev]="10.0.0.11" )

ACTION="${1:-}"
shift || true

USER_ID="" RUNTIME="" PORT="" ROLE="" BRANCH=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --user-id) USER_ID="$2"; shift 2 ;;
    --runtime) RUNTIME="$2"; shift 2 ;;
    --port)    PORT="$2"; shift 2 ;;
    --role)    ROLE="$2"; shift 2 ;;
    --branch)  BRANCH="$2"; shift 2 ;;
    *) shift ;;
  esac
done

# Determine VM identity from role
VM_NAME="${ROLE:-${RUNTIME}}"
TAP_DEV="tap-${VM_NAME}"
VM_DIR="${VM_STATE_DIR}/${VM_NAME}"
PID_FILE="${VM_DIR}/vm.pid"
SOCAT_PID_FILE="${VM_DIR}/socat.pid"
VM_IP_ADDR="${VM_IP[${ROLE:-live}]:-10.0.0.10}"

# Runner paths (built by deploy script)
RUNNER_LIVE="${WORKSPACE}/result-vm-live/bin/microvm-run"
RUNNER_DEV="${WORKSPACE}/result-vm-dev/bin/microvm-run"

log() { echo "[ovh-runtime-ctl] $*" >&2; }

pid_alive() {
  kill -0 "$1" 2>/dev/null
}

create_tap() {
  if ip link show "$TAP_DEV" &>/dev/null; then
    return 0
  fi
  log "Creating TAP device $TAP_DEV on $BRIDGE"
  ip tuntap add dev "$TAP_DEV" mode tap
  ip link set "$TAP_DEV" master "$BRIDGE"
  ip link set "$TAP_DEV" up
}

destroy_tap() {
  if ip link show "$TAP_DEV" &>/dev/null; then
    log "Removing TAP device $TAP_DEV"
    ip link delete "$TAP_DEV" 2>/dev/null || true
  fi
}

start_vm() {
  local runner
  case "$ROLE" in
    live) runner="$RUNNER_LIVE" ;;
    dev)  runner="$RUNNER_DEV" ;;
    *)    log "ERROR: unsupported role '$ROLE'"; exit 1 ;;
  esac

  if [[ ! -x "$runner" ]]; then
    log "ERROR: VM runner not found at $runner (run nix build first)"
    exit 1
  fi

  mkdir -p "$VM_DIR"
  create_tap

  log "Starting VM $VM_NAME (runner: $runner)"
  cd "$VM_DIR"
  nohup "$runner" > "${VM_DIR}/vm.log" 2>&1 &
  local vm_pid=$!
  echo "$vm_pid" > "$PID_FILE"
  log "VM started with PID $vm_pid"
}

stop_vm() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid=$(cat "$PID_FILE")
    if pid_alive "$pid"; then
      log "Stopping VM $VM_NAME (PID $pid)"
      kill "$pid" 2>/dev/null || true
      # Wait up to 10s for graceful shutdown
      for _ in $(seq 1 10); do
        pid_alive "$pid" || break
        sleep 1
      done
      # Force kill if still alive
      if pid_alive "$pid"; then
        log "Force killing VM $VM_NAME (PID $pid)"
        kill -9 "$pid" 2>/dev/null || true
      fi
    fi
    rm -f "$PID_FILE"
  fi
  destroy_tap
}

start_socat() {
  # Forward 127.0.0.1:PORT → VM_IP:PORT so hypervisor can reach sandbox
  if [[ -f "$SOCAT_PID_FILE" ]]; then
    local old_pid
    old_pid=$(cat "$SOCAT_PID_FILE")
    if pid_alive "$old_pid"; then
      return 0 # Already running
    fi
    rm -f "$SOCAT_PID_FILE"
  fi

  log "Starting socat forwarder 127.0.0.1:${PORT} → ${VM_IP_ADDR}:${PORT}"
  socat TCP-LISTEN:"${PORT}",bind=127.0.0.1,reuseaddr,fork \
        TCP:"${VM_IP_ADDR}":"${PORT}" &
  local socat_pid=$!
  echo "$socat_pid" > "$SOCAT_PID_FILE"
  log "socat forwarder started with PID $socat_pid"
}

stop_socat() {
  if [[ -f "$SOCAT_PID_FILE" ]]; then
    local pid
    pid=$(cat "$SOCAT_PID_FILE")
    if pid_alive "$pid"; then
      log "Stopping socat forwarder (PID $pid)"
      kill "$pid" 2>/dev/null || true
    fi
    rm -f "$SOCAT_PID_FILE"
  fi
}

wait_for_vm_health() {
  local max_wait=60
  local elapsed=0
  log "Waiting for sandbox health at ${VM_IP_ADDR}:${PORT} (max ${max_wait}s)"
  while (( elapsed < max_wait )); do
    if curl -fsS --connect-timeout 1 "http://${VM_IP_ADDR}:${PORT}/health" &>/dev/null; then
      log "Sandbox healthy after ${elapsed}s"
      return 0
    fi
    sleep 2
    elapsed=$((elapsed + 2))
  done
  log "ERROR: Sandbox did not become healthy within ${max_wait}s"
  return 1
}

case "$ACTION" in
  ensure)
    # Check if VM is already running
    if [[ -f "$PID_FILE" ]]; then
      pid=$(cat "$PID_FILE")
      if pid_alive "$pid"; then
        # VM running, ensure socat is up
        start_socat
        log "VM $VM_NAME already running (PID $pid)"
        exit 0
      fi
      rm -f "$PID_FILE"
    fi

    start_vm
    wait_for_vm_health
    start_socat
    ;;
  stop)
    stop_socat
    stop_vm
    ;;
  *)
    echo "invalid action '$ACTION' (expected ensure|stop)" >&2
    exit 2
    ;;
esac
