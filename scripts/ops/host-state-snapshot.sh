#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: host-state-snapshot.sh [--output <path>] [--repo <path>]

Writes a deterministic host snapshot for grind/prod drift comparison.
EOF
}

OUTPUT_PATH=""
REPO_PATH="/opt/choiros/workspace"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --output)
      OUTPUT_PATH="$2"
      shift 2
      ;;
    --repo)
      REPO_PATH="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1"
      usage
      exit 1
      ;;
  esac
done

emit() {
  printf "%s=%s\n" "$1" "$2"
}

HAS_SYSTEMCTL="false"
if command -v systemctl >/dev/null 2>&1; then
  HAS_SYSTEMCTL="true"
fi

HAS_CURL="false"
if command -v curl >/dev/null 2>&1; then
  HAS_CURL="true"
fi

HAS_SHA256SUM="false"
if command -v sha256sum >/dev/null 2>&1; then
  HAS_SHA256SUM="true"
fi

state_blob() {
  local service_name="$1"
  local service_key
  local service_status="unknown"
  local service_substatus="unknown"
  local service_result="unknown"

  service_key="$(printf '%s' "$service_name" | tr '@/.-' '____')"

  if [ "$HAS_SYSTEMCTL" = "true" ] && systemctl list-unit-files --type=service --no-legend 2>/dev/null | awk '{print $1}' | grep -qx "$service_name"; then
    service_status="$(systemctl is-active "$service_name" 2>/dev/null || true)"
    service_substatus="$(systemctl show "$service_name" -p SubState --value 2>/dev/null || true)"
    service_result="$(systemctl show "$service_name" -p Result --value 2>/dev/null || true)"
  fi

  emit "service_${service_key}_active" "$service_status"
  emit "service_${service_key}_substate" "$service_substatus"
  emit "service_${service_key}_result" "$service_result"
}

binary_blob() {
  local name="$1"
  local path="/opt/choiros/bin/${name}"
  local resolved="absent"
  local digest="absent"

  if [ -e "$path" ] || [ -L "$path" ]; then
    resolved="$(readlink -f "$path" 2>/dev/null || echo unresolved)"
    if [ -x "$resolved" ] && [ "$HAS_SHA256SUM" = "true" ]; then
      digest="$(sha256sum "$resolved" | awk '{print $1}')"
    elif [ -x "$resolved" ]; then
      digest="sha256sum-unavailable"
    else
      digest="not-executable"
    fi
  fi

  emit "binary_${name}_path" "$resolved"
  emit "binary_${name}_sha256" "$digest"
}

health_blob() {
  local name="$1"
  shift
  if [ "$HAS_CURL" != "true" ]; then
    emit "health_${name}" "curl-unavailable"
    emit "health_${name}_url" "none"
    return
  fi

  for url in "$@"; do
    if curl -fsS "$url" >/dev/null 2>&1; then
      emit "health_${name}" "ok"
      emit "health_${name}_url" "$url"
      return
    fi
  done

  emit "health_${name}" "fail"
  emit "health_${name}_url" "$1"
}

generate_snapshot() {
  emit "snapshot_version" "1"
  emit "snapshot_created_at" "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  emit "hostname" "$(hostname)"
  emit "nixos_version" "$(nixos-version 2>/dev/null || echo unavailable)"
  emit "current_system" "$(readlink -f /run/current-system 2>/dev/null || echo unavailable)"

  if [ -d "$REPO_PATH/.git" ]; then
    emit "repo_path" "$REPO_PATH"
    emit "repo_head" "$(git -C "$REPO_PATH" rev-parse HEAD 2>/dev/null || echo unavailable)"
    emit "repo_head_short" "$(git -C "$REPO_PATH" rev-parse --short HEAD 2>/dev/null || echo unavailable)"
    if [ -n "$(git -C "$REPO_PATH" status --porcelain 2>/dev/null || true)" ]; then
      emit "repo_dirty" "true"
    else
      emit "repo_dirty" "false"
    fi
  else
    emit "repo_path" "$REPO_PATH"
    emit "repo_head" "missing"
    emit "repo_head_short" "missing"
    emit "repo_dirty" "missing"
  fi

  binary_blob "sandbox"
  binary_blob "hypervisor"
  binary_blob "sandbox-ui"

  state_blob "hypervisor.service"
  state_blob "container@sandbox-live.service"
  state_blob "container@sandbox-dev.service"
  state_blob "caddy.service"

  health_blob "hypervisor" "http://127.0.0.1:9090/health"
  health_blob "sandbox_live" "http://127.0.0.1:8080/health" "http://10.233.1.2:8080/health"
  health_blob "sandbox_dev" "http://127.0.0.1:8081/health" "http://10.233.2.2:8080/health"

  if [ "$HAS_SYSTEMCTL" = "true" ]; then
    emit "hypervisor_env_files" "$(systemctl show hypervisor.service -p EnvironmentFiles --value 2>/dev/null || echo unavailable)"
  else
    emit "hypervisor_env_files" "systemctl-unavailable"
  fi
}

if [ -n "$OUTPUT_PATH" ]; then
  mkdir -p "$(dirname "$OUTPUT_PATH")"
  generate_snapshot > "$OUTPUT_PATH"
  echo "Snapshot written to $OUTPUT_PATH"
else
  generate_snapshot
fi
