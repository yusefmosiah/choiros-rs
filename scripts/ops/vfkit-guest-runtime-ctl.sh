#!/usr/bin/env bash
set -euo pipefail

# NixOS container evaluation/build can hit low default open-file limits in the guest.
# Raise process limit best-effort and lower Nix build fanout for local stability.
if [[ -n "${CHOIR_VFKIT_GUEST_NOFILE:-}" ]]; then
  ulimit -n "${CHOIR_VFKIT_GUEST_NOFILE}" 2>/dev/null || true
else
  ulimit -n 131072 2>/dev/null || true
fi

export NIX_BUILD_CORES="${NIX_BUILD_CORES:-2}"
export NIX_CONFIG="${NIX_CONFIG:-}
max-jobs = ${CHOIR_VFKIT_GUEST_MAX_JOBS:-2}
cores = ${NIX_BUILD_CORES}"

ACTION="${1:-}"
if [[ -z "$ACTION" ]]; then
  echo "usage: $0 <ensure|stop> --runtime <name> --port <port> [--role <live|dev>] [--branch <branch>]" >&2
  exit 2
fi
shift

RUNTIME=""
PORT=""
ROLE=""
BRANCH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
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

if [[ -z "$RUNTIME" || -z "$PORT" ]]; then
  echo "missing required args; need --runtime and --port" >&2
  exit 2
fi

slugify() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9._-' '_'
}

hash_short() {
  if command -v sha1sum >/dev/null 2>&1; then
    printf '%s' "$1" | sha1sum | awk '{print substr($1,1,7)}'
    return
  fi
  if command -v shasum >/dev/null 2>&1; then
    printf '%s' "$1" | shasum -a 1 | awk '{print substr($1,1,7)}'
    return
  fi
  printf '0000000'
}

config_fingerprint() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
    return
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
    return
  fi
  cksum "$path" | awk '{print $1 ":" $2}'
}

runtime_slug="$(slugify "$RUNTIME")"
container_name="sbx-$(hash_short "$runtime_slug")"

state_root="${CHOIR_VFKIT_GUEST_STATE_DIR:-/var/lib/choiros/vfkit}"
config_dir="$state_root/container-config"
runtime_dir="$state_root/runtimes/$runtime_slug"
config_file="$config_dir/${container_name}.nix"
config_fingerprint_file="$runtime_dir/config.sha256"

mkdir -p "$config_dir" "$runtime_dir"

sandbox_bin="${CHOIR_SANDBOX_BINARY_GUEST:-$state_root/bin/sandbox}"
sandbox_build_mode="${CHOIR_VFKIT_GUEST_BUILD_SANDBOX_MODE:-if-missing}"
sandbox_build_output="${CHOIR_SANDBOX_BUILD_OUTPUT:-/workspace/target/debug/sandbox}"
sandbox_bin_container="${CHOIR_SANDBOX_BINARY_CONTAINER:-/opt/choir/bin/sandbox}"
frontend_dist="${FRONTEND_DIST:-/workspace/dioxus-desktop/target/dx/dioxus-desktop/release/web/public}"
database_url="sqlite:$runtime_dir/events.db"
sandbox_role="${ROLE:-}"
sandbox_branch="${BRANCH:-}"

ensure_sandbox_binary() {
  if [[ "$sandbox_build_mode" != "always" && "$sandbox_build_mode" != "if-missing" ]]; then
    echo "invalid CHOIR_VFKIT_GUEST_BUILD_SANDBOX_MODE=$sandbox_build_mode (expected always|if-missing)" >&2
    exit 1
  fi

  if [[ "$sandbox_build_mode" == "if-missing" && -x "$sandbox_bin" ]]; then
    return 0
  fi

  if [[ "${CHOIR_VFKIT_GUEST_BUILD_SANDBOX:-true}" != "true" ]]; then
    echo "sandbox binary missing or rebuild required at $sandbox_bin and build disabled" >&2
    exit 1
  fi

  if [[ ! -f /workspace/Cargo.toml ]]; then
    echo "workspace not mounted at /workspace" >&2
    exit 1
  fi

  (
    cd /workspace
    mkdir -p /workspace/.cargo
    mkdir -p /workspace/target
    export SQLX_OFFLINE=true
    export CARGO_HOME=/workspace/.cargo
    export CARGO_TARGET_DIR=/workspace/target
    if [[ -z "${PKG_CONFIG_PATH:-}" ]]; then
      export PKG_CONFIG_PATH="/run/current-system/sw/lib/pkgconfig:/run/current-system/sw/share/pkgconfig"
    fi
    if [[ -d /run/current-system/sw/include/openssl ]]; then
      export OPENSSL_DIR="/run/current-system/sw"
    fi

    cargo fetch

    # baml 0.218 uses signed-char pointer casts that fail on aarch64-linux
    # (where c_char is unsigned). Patch cached sources for local vfkit dev.
    if [[ "$(uname -m)" == "aarch64" ]]; then
      baml_root="$(find "$CARGO_HOME/registry/src" -maxdepth 2 -type d -name 'baml-0.218.0' | head -n 1 || true)"
      if [[ -n "$baml_root" ]]; then
        sed -i 's/cast::<i8>()/cast::<std::ffi::c_char>()/g' "$baml_root/src/runtime.rs" "$baml_root/src/raw_objects/mod.rs"
      fi
    fi

    cargo build -p sandbox --bin sandbox
  )

  if [[ ! -x "$sandbox_build_output" ]]; then
    echo "built sandbox binary missing at $sandbox_build_output" >&2
    exit 1
  fi

  mkdir -p "$(dirname "$sandbox_bin")"
  cp "$sandbox_build_output" "$sandbox_bin"
  chmod +x "$sandbox_bin"

  if [[ ! -x "$sandbox_bin" ]]; then
    echo "sandbox binary still missing after build: $sandbox_bin" >&2
    exit 1
  fi
}

render_container_config() {
  cat >"$config_file" <<CONFIG
{ pkgs, ... }:
{
  system.stateVersion = "25.11";

  environment.systemPackages = with pkgs; [
    curl
    sqlite
  ];

  # Reuse the VM's shared workspace inside each branch container so
  # the sandbox binary and frontend dist are accessible.
  fileSystems."/workspace" = {
    device = "/workspace";
    fsType = "none";
    options = [ "bind" ];
  };

  systemd.services.choir-sandbox = {
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "${sandbox_bin_container}";
      Restart = "always";
      RestartSec = 1;
      WorkingDirectory = "/";
      Environment = [
        "PORT=${PORT}"
        "DATABASE_URL=${database_url}"
        "SQLX_OFFLINE=true"
        "FRONTEND_DIST=${frontend_dist}"
        "CHOIR_SANDBOX_RUNTIME=${RUNTIME}"
        "CHOIR_SANDBOX_ROLE=${sandbox_role}"
        "CHOIR_SANDBOX_BRANCH=${sandbox_branch}"
      ];
    };
  };
}
CONFIG
}

container_exists() {
  nixos-container status "$container_name" >/dev/null 2>&1
}

container_running() {
  local status_output
  status_output="$(nixos-container status "$container_name" 2>/dev/null || true)"
  [[ "$status_output" == *"running"* ]]
}

ensure_container() {
  local recreate desired_fingerprint existing_fingerprint
  recreate="${CHOIR_VFKIT_GUEST_RECREATE_CONTAINER:-false}"
  desired_fingerprint="$(config_fingerprint "$config_file")"
  existing_fingerprint="$(cat "$config_fingerprint_file" 2>/dev/null || true)"

  if container_exists && [[ "$recreate" != "true" ]] && [[ -n "$existing_fingerprint" ]] && [[ "$existing_fingerprint" == "$desired_fingerprint" ]]; then
    return 0
  fi

  if container_exists; then
    nixos-container stop "$container_name" || true
    nixos-container destroy "$container_name" || true
  fi

  nixos-container create "$container_name" --config-file "$config_file" --use-host-network
  printf '%s\n' "$desired_fingerprint" > "$config_fingerprint_file"
}

start_container() {
  if container_running; then
    return 0
  fi
  nixos-container start "$container_name"
}

sync_sandbox_binary_into_container() {
  local container_bin_dir
  container_bin_dir="$(dirname "$sandbox_bin_container")"

  nixos-container run "$container_name" -- mkdir -p "$container_bin_dir"
  cat "$sandbox_bin" \
    | nixos-container run "$container_name" -- sh -lc \
      "cat > '$sandbox_bin_container' && chmod +x '$sandbox_bin_container'"
}

restart_sandbox_service() {
  nixos-container run "$container_name" -- systemctl restart choir-sandbox.service
}

stop_container() {
  if ! container_exists; then
    return 0
  fi
  nixos-container stop "$container_name" || true
}

case "$ACTION" in
  ensure)
  ensure_sandbox_binary
  render_container_config
  ensure_container
  start_container
  sync_sandbox_binary_into_container
  restart_sandbox_service
    ;;
  stop)
    stop_container
    ;;
  *)
    echo "invalid action '$ACTION' (expected ensure|stop)" >&2
    exit 2
    ;;
esac
