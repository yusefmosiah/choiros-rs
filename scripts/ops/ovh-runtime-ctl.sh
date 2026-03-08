#!/run/current-system/sw/bin/bash
set -euo pipefail

# Ensure system tools (ip, kill, etc.) are available when called from systemd
export PATH="/run/current-system/sw/bin:/run/current-system/sw/sbin:$PATH"

# Cloud-hypervisor runtime controller for OVH bare metal.
# Called by the hypervisor's SandboxRegistry to manage sandbox microVMs.
#
# Each VM is managed via microvm.nix runner scripts:
#   1. tap-up: creates TAP device
#   2. virtiofsd-run: starts virtiofs daemons (supervisord)
#   3. microvm-run: starts cloud-hypervisor
#
# Lifecycle verbs:
#   ensure    — resume from VM snapshot if available, otherwise cold boot
#   hibernate — pause + snapshot VM state to disk + stop process (fast resume next ensure)
#   stop      — hard stop, no VM snapshot (data snapshot still taken)
#
# Usage: ovh-runtime-ctl.sh <ensure|hibernate|stop> --user-id <id> --runtime <name> --port <port> [--role <live|dev>] [--branch <branch>]

WORKSPACE="${CHOIR_WORKSPACE_ROOT:-/opt/choiros/workspace}"
# Prefer injected store path for VM runner (set by NixOS systemd unit)
VM_RUNNER_OVERRIDE="${CHOIR_VM_RUNNER_DIR:-}"
VM_STATE_DIR="${CHOIR_VM_STATE_DIR:-/opt/choiros/vms/state}"
SNAPSHOT_DIR="${CHOIR_SNAPSHOT_DIR:-/data/snapshots}"
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
VM_SNAPSHOT_DIR="${VM_DIR}/vm-snapshot"
API_SOCK="${VM_DIR}/sandbox-${VM_NAME}.sock"

# Runner directory: prefer store path from env, fall back to workspace symlink
if [[ -n "$VM_RUNNER_OVERRIDE" ]]; then
  RUNNER_DIR="$VM_RUNNER_OVERRIDE"
else
  RUNNER_DIR="${WORKSPACE}/result-vm-${VM_NAME}"
fi

log() { echo "[ovh-runtime-ctl] $*" >&2; }

pid_alive() { kill -0 "$1" 2>/dev/null; }

# Snapshot data disk image (simple file copy with reflink if on btrfs)
snapshot_data() {
  local data_img="${VM_DIR}/data.img"
  if [[ ! -f "$data_img" ]]; then
    log "No data.img found, skipping data snapshot"
    return 0
  fi

  local snap_name snap_path
  snap_name="$(date -u +%Y%m%dT%H%M%SZ)-${VM_NAME}"
  snap_path="${SNAPSHOT_DIR}/${snap_name}.img"
  mkdir -p "$SNAPSHOT_DIR"

  log "Snapshotting data disk: $snap_path"
  cp --reflink=auto "$data_img" "$snap_path"

  # Prune old snapshots (keep last 3)
  local snaps
  snaps=$(find "$SNAPSHOT_DIR" -maxdepth 1 -name "*-${VM_NAME}.img" -type f | sort)
  local count
  count=$(echo "$snaps" | wc -l)
  if (( count > 3 )); then
    echo "$snaps" | head -n $((count - 3)) | while read -r old; do
      log "Pruning old data snapshot: $old"
      rm -f "$old"
    done
  fi
}

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

  # Clean stale sockets before starting
  rm -f "${VM_DIR}"/*-virtiofs-*.sock

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

  # Wait for sockets to appear (2 shares: nix-store, choiros-creds)
  # Note: sandbox data is on virtio-blk, sandbox binary is in /nix/store
  local max_wait=30 elapsed=0
  while (( elapsed < max_wait )); do
    local sock_count
    sock_count=$(find "$VM_DIR" -maxdepth 1 -name "*-virtiofs-*.sock" 2>/dev/null | wc -l)
    if (( sock_count >= 2 )); then
      log "virtiofsd sockets ready ($sock_count found in ${elapsed}s)"
      return 0
    fi
    sleep 0.5
    elapsed=$((elapsed + 1))
  done
  log "WARNING: virtiofsd sockets may not be fully ready after ${max_wait} checks (found $(find "$VM_DIR" -maxdepth 1 -name "*-virtiofs-*.sock" 2>/dev/null | wc -l))"
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
  # Also kill any orphaned virtiofsd children
  pkill -f "virtiofsd.*${VM_NAME}" 2>/dev/null || true
}

# Cold boot: start cloud-hypervisor from microvm-run script
cold_boot_vm() {
  local runner="${RUNNER_DIR}/bin/microvm-run"
  if [[ ! -x "$runner" ]]; then
    log "ERROR: microvm-run not found at $runner"
    exit 1
  fi

  log "Cold booting VM $VM_NAME"
  cd "$VM_DIR"
  nohup "$runner" > "${VM_DIR}/vm.log" 2>&1 &
  local pid=$!
  echo "$pid" > "$VM_PID_FILE"
  log "VM cold boot started (PID $pid)"
}

# Check if a VM snapshot exists
has_vm_snapshot() {
  [[ -d "$VM_SNAPSHOT_DIR" ]] && [[ -f "$VM_SNAPSHOT_DIR/state.json" ]]
}

# Restore VM from cloud-hypervisor snapshot (fast resume)
restore_vm() {
  if [[ ! -d "$VM_SNAPSHOT_DIR" ]] || [[ ! -f "$VM_SNAPSHOT_DIR/state.json" ]]; then
    return 1  # No snapshot available
  fi

  log "Restoring VM $VM_NAME from snapshot"

  # Remove stale API socket (cloud-hypervisor creates a new one)
  rm -f "$API_SOCK"

  cd "$VM_DIR"
  nohup cloud-hypervisor \
    --restore "source_url=file://${VM_SNAPSHOT_DIR}" \
    --api-socket "$API_SOCK" \
    > "${VM_DIR}/vm.log" 2>&1 &
  local pid=$!
  echo "$pid" > "$VM_PID_FILE"

  # Wait for the process to start and the API socket to appear
  local max_wait=30 elapsed=0
  while (( elapsed < max_wait )); do
    if ! pid_alive "$pid"; then
      log "VM restore failed (process exited), falling back to cold boot"
      rm -f "$VM_PID_FILE"
      rm -rf "$VM_SNAPSHOT_DIR"
      return 1
    fi
    if [[ -S "$API_SOCK" ]]; then
      break
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  if [[ ! -S "$API_SOCK" ]]; then
    log "VM restore timed out waiting for API socket"
    kill "$pid" 2>/dev/null || true
    rm -f "$VM_PID_FILE"
    rm -rf "$VM_SNAPSHOT_DIR"
    return 1
  fi

  # The VM resumes in paused state after restore — resume it
  sleep 1
  curl -s --max-time 5 --unix-socket "$API_SOCK" -X PUT \
    "http://localhost/api/v1/vm.resume" 2>/dev/null || true

  log "VM restored from snapshot (PID $pid)"
  return 0
}

# Hibernate: pause + snapshot VM state + stop process
hibernate_vm() {
  if [[ ! -S "$API_SOCK" ]]; then
    log "No API socket — VM not running, skipping hibernate"
    return 1
  fi

  log "Hibernating VM $VM_NAME"

  # Pause vCPUs
  curl -s --max-time 5 --unix-socket "$API_SOCK" -X PUT \
    "http://localhost/api/v1/vm.pause" 2>/dev/null || true
  sleep 1

  # Snapshot VM state to disk
  mkdir -p "$VM_SNAPSHOT_DIR"
  local snapshot_result
  snapshot_result=$(curl -s --max-time 30 --unix-socket "$API_SOCK" -X PUT \
    "http://localhost/api/v1/vm.snapshot" \
    -H "Content-Type: application/json" \
    -d "{\"destination_url\": \"file://${VM_SNAPSHOT_DIR}\"}" 2>&1)

  if echo "$snapshot_result" | grep -qi "error"; then
    log "VM snapshot failed: $snapshot_result"
    # Resume the VM since hibernate failed
    curl -s --max-time 5 --unix-socket "$API_SOCK" -X PUT \
      "http://localhost/api/v1/vm.resume" 2>/dev/null || true
    return 1
  fi

  log "VM state saved to $VM_SNAPSHOT_DIR"

  # Stop the VM process (state is on disk now)
  stop_vm_process
  return 0
}

# Stop VM process without graceful shutdown (used after hibernate)
stop_vm_process() {
  if [[ -f "$VM_PID_FILE" ]]; then
    local pid; pid=$(cat "$VM_PID_FILE")
    if pid_alive "$pid"; then
      log "Stopping VM process (PID $pid)"
      kill "$pid" 2>/dev/null || true
      local i
      for i in $(seq 1 5); do
        pid_alive "$pid" || break
        sleep 1
      done
      pid_alive "$pid" && kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$VM_PID_FILE"
  fi
  rm -f "$API_SOCK"
}

# Graceful VM shutdown (for hard stop)
stop_vm() {
  if [[ -S "$API_SOCK" ]] 2>/dev/null; then
    log "Requesting VM shutdown via API socket"
    curl -s --max-time 5 --unix-socket "$API_SOCK" -X PUT \
      "http://localhost/api/v1/vm.shutdown" 2>/dev/null || true
    sleep 2
  fi

  stop_vm_process
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

  log "Starting socat forwarder 127.0.0.1:${PORT} -> ${VM_IP_ADDR}:${PORT}"
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

    # Try fast restore from VM snapshot, fall back to cold boot
    if has_vm_snapshot; then
      # Restart virtiofsd to get fresh sockets (old ones go stale after VM stop)
      stop_virtiofsd
      start_virtiofsd
      if restore_vm; then
        log "VM resumed from snapshot"
      else
        log "Snapshot restore failed, cold booting"
        cold_boot_vm
      fi
    else
      start_virtiofsd
      cold_boot_vm
    fi

    wait_for_vm_health
    start_socat
    ;;

  hibernate)
    stop_socat
    snapshot_data
    if hibernate_vm; then
      log "VM $VM_NAME hibernated (virtiofsd + TAP kept alive for fast restore)"
    else
      log "Hibernate failed, falling back to hard stop"
      stop_vm
      stop_virtiofsd
      teardown_tap
    fi
    ;;

  stop)
    stop_socat
    snapshot_data
    stop_vm
    stop_virtiofsd
    teardown_tap
    # Clean up VM snapshot since we did a full stop
    rm -rf "$VM_SNAPSHOT_DIR"
    ;;

  *)
    echo "invalid action '$ACTION' (expected ensure|hibernate|stop)" >&2
    exit 2
    ;;
esac
