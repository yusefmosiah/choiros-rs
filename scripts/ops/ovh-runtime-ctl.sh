#!/run/current-system/sw/bin/bash
set -euo pipefail

# Cloud-hypervisor runtime controller for OVH bare metal.
# Called by the hypervisor's SandboxRegistry to manage sandbox microVMs.
#
# Each VM is managed via microvm.nix runner scripts:
#   1. tap-up: creates TAP device
#   2. virtiofsd-run: starts virtiofs daemons (supervisord)
#   3. microvm-run: starts cloud-hypervisor
#
# Usage: ovh-runtime-ctl.sh <ensure|stop> --user-id <id> --runtime <name> --port <port> [--role <live|dev>] [--branch <branch>]

WORKSPACE="${CHOIR_WORKSPACE_ROOT:-/opt/choiros/workspace}"
VM_STATE_DIR="${CHOIR_VM_STATE_DIR:-/opt/choiros/vms/state}"
BRIDGE="br-choiros"

# VM network assignments (static for Gate 2; dynamic per-user in Gate 3)
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

VM_NAME="${ROLE:-${RUNTIME}}"
VM_DIR="${VM_STATE_DIR}/${VM_NAME}"
VM_PID_FILE="${VM_DIR}/vm.pid"
VIRTIOFSD_PID_FILE="${VM_DIR}/virtiofsd.pid"
SOCAT_PID_FILE="${VM_DIR}/socat.pid"
VM_IP_ADDR="${VM_IP[${ROLE:-live}]:-10.0.0.10}"

# Runner directory (built by deploy script)
RUNNER_DIR="${WORKSPACE}/result-vm-${VM_NAME}"

log() { echo "[ovh-runtime-ctl] $*" >&2; }

pid_alive() { kill -0 "$1" 2>/dev/null; }

setup_tap() {
  local tap="tap-${VM_NAME}"
  if ip link show "$tap" &>/dev/null; then
    log "TAP $tap already exists"
    return 0
  fi
  log "Creating TAP $tap on $BRIDGE (multi_queue + vnet_hdr)"
  ip tuntap add dev "$tap" mode tap vnet_hdr multi_queue
  ip link set "$tap" master "$BRIDGE"
  ip link set "$tap" up
}

teardown_tap() {
  local tap="tap-${VM_NAME}"
  if ip link show "$tap" &>/dev/null; then
    log "Removing TAP $tap"
    ip link delete "$tap" 2>/dev/null || true
  fi
}

start_virtiofsd() {
  if [[ -f "$VIRTIOFSD_PID_FILE" ]]; then
    local pid; pid=$(cat "$VIRTIOFSD_PID_FILE")
    if pid_alive "$pid"; then
      log "virtiofsd already running (PID $pid)"
      return 0
    fi
    rm -f "$VIRTIOFSD_PID_FILE"
  fi

  local runner="${RUNNER_DIR}/bin/virtiofsd-run"
  if [[ ! -x "$runner" ]]; then
    log "ERROR: virtiofsd-run not found at $runner"
    exit 1
  fi

  log "Starting virtiofsd"
  cd "$VM_DIR"
  nohup "$runner" > "${VM_DIR}/virtiofsd.log" 2>&1 &
  local pid=$!
  echo "$pid" > "$VIRTIOFSD_PID_FILE"
  log "virtiofsd started (PID $pid)"

  # Wait for sockets to appear
  local max_wait=15 elapsed=0
  while (( elapsed < max_wait )); do
    local sock_count
    sock_count=$(find "$VM_DIR" -maxdepth 1 -name "*.sock" -not -name "*.api.sock" | wc -l)
    if (( sock_count >= 3 )); then
      log "virtiofsd sockets ready ($sock_count found)"
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done
  log "WARNING: virtiofsd sockets may not be fully ready after ${max_wait}s"
}

stop_virtiofsd() {
  if [[ -f "$VIRTIOFSD_PID_FILE" ]]; then
    local pid; pid=$(cat "$VIRTIOFSD_PID_FILE")
    if pid_alive "$pid"; then
      log "Stopping virtiofsd (PID $pid)"
      kill "$pid" 2>/dev/null || true
      sleep 1
      pid_alive "$pid" && kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$VIRTIOFSD_PID_FILE"
  fi
}

start_vm() {
  if [[ -f "$VM_PID_FILE" ]]; then
    local pid; pid=$(cat "$VM_PID_FILE")
    if pid_alive "$pid"; then
      log "VM already running (PID $pid)"
      return 0
    fi
    rm -f "$VM_PID_FILE"
  fi

  local runner="${RUNNER_DIR}/bin/microvm-run"
  if [[ ! -x "$runner" ]]; then
    log "ERROR: microvm-run not found at $runner"
    exit 1
  fi

  log "Starting VM $VM_NAME"
  cd "$VM_DIR"
  nohup "$runner" > "${VM_DIR}/vm.log" 2>&1 &
  local pid=$!
  echo "$pid" > "$VM_PID_FILE"
  log "VM started (PID $pid)"
}

stop_vm() {
  # Try graceful shutdown via API socket first
  local api_sock="${VM_DIR}/${VM_NAME}.sock"
  if [[ -S "$api_sock" ]] 2>/dev/null; then
    log "Requesting VM shutdown via API socket"
    curl -s --unix-socket "$api_sock" -X PUT \
      "http://localhost/api/v1/vm.shutdown" 2>/dev/null || true
    sleep 2
  fi

  if [[ -f "$VM_PID_FILE" ]]; then
    local pid; pid=$(cat "$VM_PID_FILE")
    if pid_alive "$pid"; then
      log "Stopping VM (PID $pid)"
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 1 10); do
        pid_alive "$pid" || break
        sleep 1
      done
      pid_alive "$pid" && kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$VM_PID_FILE"
  fi
}

start_socat() {
  if [[ -f "$SOCAT_PID_FILE" ]]; then
    local pid; pid=$(cat "$SOCAT_PID_FILE")
    if pid_alive "$pid"; then
      log "socat forwarder already running (PID $pid)"
      return 0
    fi
    rm -f "$SOCAT_PID_FILE"
  fi

  log "Starting socat forwarder 127.0.0.1:${PORT} → ${VM_IP_ADDR}:${PORT}"
  socat TCP-LISTEN:"${PORT}",bind=127.0.0.1,reuseaddr,fork \
        TCP:"${VM_IP_ADDR}":"${PORT}" &
  local pid=$!
  echo "$pid" > "$SOCAT_PID_FILE"
  log "socat started (PID $pid)"
}

stop_socat() {
  if [[ -f "$SOCAT_PID_FILE" ]]; then
    local pid; pid=$(cat "$SOCAT_PID_FILE")
    if pid_alive "$pid"; then
      log "Stopping socat forwarder (PID $pid)"
      kill "$pid" 2>/dev/null || true
    fi
    rm -f "$SOCAT_PID_FILE"
  fi
}

wait_for_vm_health() {
  local max_wait=90 elapsed=0
  log "Waiting for sandbox health at ${VM_IP_ADDR}:${PORT} (max ${max_wait}s)"
  while (( elapsed < max_wait )); do
    if curl -fsS --connect-timeout 2 "http://${VM_IP_ADDR}:${PORT}/health" &>/dev/null; then
      log "Sandbox healthy after ${elapsed}s"
      return 0
    fi
    sleep 3
    elapsed=$((elapsed + 3))
  done
  log "ERROR: Sandbox not healthy after ${max_wait}s"
  log "VM log tail:"
  tail -20 "${VM_DIR}/vm.log" >&2 || true
  return 1
}

case "$ACTION" in
  ensure)
    if [[ ! -d "$RUNNER_DIR/bin" ]]; then
      log "ERROR: VM runner not found at $RUNNER_DIR (build with: nix build .#nixosConfigurations.choiros-ch-sandbox-${VM_NAME}.config.microvm.runner.cloud-hypervisor -o result-vm-${VM_NAME})"
      exit 1
    fi

    mkdir -p "$VM_DIR"

    # Check if VM is already running and healthy
    if [[ -f "$VM_PID_FILE" ]]; then
      pid=$(cat "$VM_PID_FILE")
      if pid_alive "$pid"; then
        start_socat
        log "VM $VM_NAME already running (PID $pid)"
        exit 0
      fi
      rm -f "$VM_PID_FILE"
    fi

    setup_tap
    start_virtiofsd
    start_vm
    wait_for_vm_health
    start_socat
    ;;
  stop)
    stop_socat
    stop_vm
    stop_virtiofsd
    teardown_tap
    ;;
  *)
    echo "invalid action '$ACTION' (expected ensure|stop)" >&2
    exit 2
    ;;
esac
