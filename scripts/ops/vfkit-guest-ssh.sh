#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  vfkit-guest-ssh.sh [--user-id <id>] [--host <host>] [--port <port>] [--user <user>] [--guest-name <name>] [-- <command...>]

Examples:
  vfkit-guest-ssh.sh
  vfkit-guest-ssh.sh -- btop
  vfkit-guest-ssh.sh --user-id public -- btop
USAGE
}

USER_ID="${CHOIR_VFKIT_USER_ID:-}"
HOST="${CHOIR_VFKIT_GUEST_HOST:-127.0.0.1}"
PORT_OVERRIDE="${CHOIR_VFKIT_GUEST_PORT:-}"
GUEST_USER="${CHOIR_VFKIT_GUEST_USER:-root}"
GUEST_NAME="${CHOIR_VFKIT_GUEST_NAME:-}"
SSH_BASE="${CHOIR_VFKIT_SSH_PORT_BASE:-22000}"
KEY_PATH="${CHOIR_VFKIT_SSH_KEY_PATH:-$HOME/.local/share/choiros/vfkit/keys/runtime_ed25519}"
ALLOW_DHCP_FALLBACK="${CHOIR_VFKIT_ALLOW_DHCP_LEASE_FALLBACK:-true}"

command_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --user-id)
      USER_ID="${2:-}"
      shift 2
      ;;
    --host)
      HOST="${2:-}"
      shift 2
      ;;
    --port)
      PORT_OVERRIDE="${2:-}"
      shift 2
      ;;
    --user)
      GUEST_USER="${2:-}"
      shift 2
      ;;
    --guest-name)
      GUEST_NAME="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --)
      shift
      command_args=("$@")
      break
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ ! -f "$KEY_PATH" ]]; then
  echo "missing SSH key: $KEY_PATH" >&2
  echo "start a vfkit runtime first (for key provisioning): just dev-control-plane" >&2
  exit 1
fi

hash_short() {
  if command -v shasum >/dev/null 2>&1; then
    printf '%s' "$1" | shasum -a 1 | awk '{print substr($1,1,6)}'
    return
  fi
  if command -v sha1sum >/dev/null 2>&1; then
    printf '%s' "$1" | sha1sum | awk '{print substr($1,1,6)}'
    return
  fi
  printf '000000'
}

hash_ten() {
  if command -v shasum >/dev/null 2>&1; then
    printf '%s' "$1" | shasum -a 1 | awk '{print substr($1,1,10)}'
    return
  fi
  if command -v sha1sum >/dev/null 2>&1; then
    printf '%s' "$1" | sha1sum | awk '{print substr($1,1,10)}'
    return
  fi
  printf '0000000000'
}

derive_guest_name() {
  local uid="${USER_ID:-public}"
  printf 'cvm-%s' "$(hash_ten "$uid")"
}

if [[ -z "$GUEST_NAME" ]]; then
  GUEST_NAME="$(derive_guest_name)"
fi

resolve_guest_ips_from_leases() {
  local leases_file="/var/db/dhcpd_leases"
  [[ -r "$leases_file" ]] || return 0

  awk -v wanted_name="$GUEST_NAME" '
    $0 == "{" { in_lease = 1; lease_name = ""; lease_ip = ""; lease_hex = ""; next }
    in_lease && $0 == "}" {
      if (lease_name == wanted_name && lease_ip != "" && lease_hex != "") {
        print lease_hex "\t" lease_ip
      }
      in_lease = 0
      next
    }
    in_lease && $0 ~ /^[[:space:]]*name=/ {
      line = $0
      sub(/^[[:space:]]*name=/, "", line)
      lease_name = line
    }
    in_lease && $0 ~ /^[[:space:]]*ip_address=/ {
      line = $0
      sub(/^[[:space:]]*ip_address=/, "", line)
      lease_ip = line
    }
    in_lease && $0 ~ /^[[:space:]]*lease=0x/ {
      line = $0
      sub(/^[[:space:]]*lease=0x/, "", line)
      lease_hex = line
    }
  ' "$leases_file"
}

resolve_endpoints() {
  if [[ "$HOST" != "127.0.0.1" ]]; then
    local port="${PORT_OVERRIDE:-22}"
    printf '%s:%s\n' "$HOST" "$port"
    return 0
  fi

  if [[ -n "$PORT_OVERRIDE" ]]; then
    printf '127.0.0.1:%s\n' "$PORT_OVERRIDE"
  else
    local endpoint_user_id="${USER_ID:-public}"
    local user_hash_hex user_hash_dec ssh_port
    user_hash_hex="$(hash_short "$endpoint_user_id")"
    user_hash_dec=$((16#${user_hash_hex:-0}))
    ssh_port=$((SSH_BASE + (user_hash_dec % 1000)))
    printf '127.0.0.1:%s\n' "$ssh_port"
  fi

  if [[ "$ALLOW_DHCP_FALLBACK" == "true" && "$(uname -s)" == "Darwin" ]]; then
    resolve_guest_ips_from_leases \
      | sort -r \
      | awk '{ print $2 ":22" }'
  fi
}

ssh_opts=(
  -o StrictHostKeyChecking=no
  -o UserKnownHostsFile=/dev/null
  -o LogLevel=ERROR
  -o BatchMode=yes
  -o IdentitiesOnly=yes
  -o ConnectTimeout=5
  -i "$KEY_PATH"
)

chosen_endpoint=""
tried_endpoints=()

while IFS= read -r endpoint; do
  [[ -z "$endpoint" ]] && continue
  tried_endpoints+=("$endpoint")

  host="${endpoint%:*}"
  port="${endpoint##*:}"
  if ssh -p "$port" "${ssh_opts[@]}" "${GUEST_USER}@${host}" true >/dev/null 2>&1; then
    chosen_endpoint="$endpoint"
    break
  fi
done < <(resolve_endpoints | awk '!seen[$0]++')

if [[ -z "$chosen_endpoint" ]]; then
  echo "no reachable vfkit guest endpoint found" >&2
  if [[ "${#tried_endpoints[@]}" -gt 0 ]]; then
    echo "tried: ${tried_endpoints[*]}" >&2
  fi
  echo "ensure a vfkit runtime is running first (for example: open http://localhost:9090)" >&2
  exit 1
fi

host="${chosen_endpoint%:*}"
port="${chosen_endpoint##*:}"
echo "connecting to ${GUEST_USER}@${host}:${port}" >&2

if [[ "${#command_args[@]}" -gt 0 ]]; then
  exec ssh -t -p "$port" "${ssh_opts[@]}" "${GUEST_USER}@${host}" "${command_args[@]}"
fi

exec ssh -p "$port" "${ssh_opts[@]}" "${GUEST_USER}@${host}"
