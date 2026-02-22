#!/usr/bin/env bash
set -euo pipefail

# Runs on the target host.
# Converges host to RELEASE_SHA and validates hypervisor + sandbox runtime.

RELEASE_SHA="${RELEASE_SHA:-}"
WORKDIR="${WORKDIR:-/opt/choiros/deploy-repo}"
REPO_URL="${REPO_URL:-https://github.com/yusefmosiah/choiros-rs.git}"

SANDBOX_STORE_PATH="${SANDBOX_STORE_PATH:-}"
HYPERVISOR_STORE_PATH="${HYPERVISOR_STORE_PATH:-}"
DESKTOP_STORE_PATH="${DESKTOP_STORE_PATH:-}"

if [[ -z "${RELEASE_SHA}" ]]; then
  echo "error: RELEASE_SHA is required"
  exit 2
fi

dump_runtime_diagnostics() {
  echo "===== deploy diagnostics: sandbox containers ====="
  for container_name in sandbox-live sandbox-dev; do
    echo "===== deploy diagnostics: ${container_name} ====="
    nixos-container run "${container_name}" -- systemctl show sandbox --property=ActiveState,SubState,ExecMainCode,ExecMainStatus || true
    nixos-container run "${container_name}" -- systemctl status sandbox --no-pager -l || true
    nixos-container run "${container_name}" -- journalctl -u sandbox --since '-15 min' -n 120 --no-pager || true
    nixos-container run "${container_name}" -- ss -ltnp '( sport = :8080 )' || true
  done

  echo "===== deploy diagnostics: host ====="
  ss -ltnp '( sport = :9090 or sport = :8080 or sport = :8081 )' || true
  systemctl status hypervisor --no-pager -l || true
  journalctl -u hypervisor --since '-15 min' -n 120 --no-pager || true
  systemctl status container@sandbox-live --no-pager -l || true
  systemctl status container@sandbox-dev --no-pager -l || true
}

wait_http() {
  local url="$1"
  local timeout_secs="$2"
  local started_at
  started_at="$(date +%s)"

  while true; do
    if curl -fsS "${url}" >/dev/null; then
      return 0
    fi

    if [[ $(( $(date +%s) - started_at )) -ge "${timeout_secs}" ]]; then
      echo "Timed out waiting for ${url}"
      dump_runtime_diagnostics
      return 1
    fi

    sleep 2
  done
}

wait_container_port() {
  local container_name="$1"
  local port="$2"
  local timeout_secs="$3"
  local started_at
  started_at="$(date +%s)"

  while true; do
    if nixos-container run "${container_name}" -- sh -lc "ss -ltn '( sport = :${port} )' | grep -q LISTEN"; then
      return 0
    fi

    if [[ $(( $(date +%s) - started_at )) -ge "${timeout_secs}" ]]; then
      echo "Timed out waiting for ${container_name} to listen on ${port}"
      dump_runtime_diagnostics
      return 1
    fi

    sleep 2
  done
}

wait_container_http_healthy() {
  local container_name="$1"
  local url="$2"
  local timeout_secs="$3"
  local started_at
  local probe
  started_at="$(date +%s)"

  probe="
    if command -v curl >/dev/null 2>&1; then
      curl -fsS '${url}'
    elif command -v wget >/dev/null 2>&1; then
      wget -qO- '${url}'
    elif command -v busybox >/dev/null 2>&1; then
      busybox wget -qO- '${url}'
    else
      exit 127
    fi
  "

  while true; do
    if nixos-container run "${container_name}" -- sh -lc "${probe}" | grep -q '"status"[[:space:]]*:[[:space:]]*"healthy"'; then
      return 0
    fi

    if [[ $(( $(date +%s) - started_at )) -ge "${timeout_secs}" ]]; then
      echo "Timed out waiting for healthy response from ${container_name} (${url})"
      echo "Last probe result:"
      nixos-container run "${container_name}" -- sh -lc "${probe}" || true
      dump_runtime_diagnostics
      return 1
    fi

    sleep 2
  done
}

if [[ ! -d "${WORKDIR}/.git" ]]; then
  rm -rf "${WORKDIR}"
  git clone "${REPO_URL}" "${WORKDIR}"
fi

cd "${WORKDIR}"
git fetch origin
git checkout -f "${RELEASE_SHA}"

if [[ -n "${SANDBOX_STORE_PATH}" || -n "${HYPERVISOR_STORE_PATH}" || -n "${DESKTOP_STORE_PATH}" ]]; then
  if [[ -z "${SANDBOX_STORE_PATH}" || -z "${HYPERVISOR_STORE_PATH}" || -z "${DESKTOP_STORE_PATH}" ]]; then
    echo "error: either set all store paths or none"
    exit 2
  fi

  export NIX_CONFIG="fallback = false"
  nix-store --realise "${SANDBOX_STORE_PATH}"
  nix-store --realise "${HYPERVISOR_STORE_PATH}"
  nix-store --realise "${DESKTOP_STORE_PATH}"

  install -m 0755 "${SANDBOX_STORE_PATH}/bin/sandbox" /opt/choiros/bin/sandbox
  install -m 0755 "${HYPERVISOR_STORE_PATH}/bin/hypervisor" /opt/choiros/bin/hypervisor
  install -m 0755 "${DESKTOP_STORE_PATH}/bin/sandbox-ui" /opt/choiros/bin/sandbox-ui
fi

export NIX_PATH="nixpkgs=$(nix --extra-experimental-features nix-command --extra-experimental-features flakes eval --raw nixpkgs#path)"
export NIXOS_CONFIG=/etc/nixos/configuration.nix
nixos-rebuild switch

wait_http http://127.0.0.1:9090/login 120
wait_container_port sandbox-live 8080 120
wait_container_port sandbox-dev 8080 120
wait_container_http_healthy sandbox-live http://127.0.0.1:8080/health 120
wait_container_http_healthy sandbox-dev http://127.0.0.1:8080/health 120

echo "Deploy OK for ${RELEASE_SHA}"
