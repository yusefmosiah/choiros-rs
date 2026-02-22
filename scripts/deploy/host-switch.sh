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
ALLOW_HOST_BUILD_FALLBACK="${ALLOW_HOST_BUILD_FALLBACK:-false}"

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

resolve_or_build_store_path() {
  local store_path="$1"
  local flake_attr="$2"
  local label="$3"

  if [[ -n "${store_path}" ]] && nix-store --realise "${store_path}" >/dev/null 2>&1; then
    echo "${store_path}"
    return 0
  fi

  if [[ "${ALLOW_HOST_BUILD_FALLBACK}" != "true" ]]; then
    if [[ -z "${store_path}" ]]; then
      echo "error: ${label} store path missing and ALLOW_HOST_BUILD_FALLBACK=false"
    else
      echo "error: could not realize ${label} store path ${store_path} and ALLOW_HOST_BUILD_FALLBACK=false"
    fi
    echo "error: build release outputs on grind, copy closures, and pass *_STORE_PATH values"
    exit 1
  fi

  if [[ -n "${store_path}" ]]; then
    echo "warn: could not realize ${label} store path ${store_path}; building ${flake_attr} on host"
  else
    echo "warn: ${label} store path missing; building ${flake_attr} on host"
  fi

  nix --extra-experimental-features nix-command \
      --extra-experimental-features flakes \
      build "${flake_attr}" \
      --no-link \
      --print-out-paths | tail -n 1
}

sandbox_out="$(resolve_or_build_store_path "${SANDBOX_STORE_PATH}" './sandbox#sandbox' 'sandbox')"
hypervisor_out="$(resolve_or_build_store_path "${HYPERVISOR_STORE_PATH}" './hypervisor#hypervisor' 'hypervisor')"
desktop_out="$(resolve_or_build_store_path "${DESKTOP_STORE_PATH}" './dioxus-desktop#desktop' 'desktop')"

install -m 0755 "${sandbox_out}/bin/sandbox" /opt/choiros/bin/sandbox
install -m 0755 "${hypervisor_out}/bin/hypervisor" /opt/choiros/bin/hypervisor
install -m 0755 "${desktop_out}/bin/dioxus-desktop" /opt/choiros/bin/dioxus-desktop

export NIX_PATH="nixpkgs=$(nix --extra-experimental-features nix-command --extra-experimental-features flakes eval --raw nixpkgs#path)"
export NIXOS_CONFIG=/etc/nixos/configuration.nix
nixos-rebuild switch

wait_http http://127.0.0.1:9090/login 120
wait_container_port sandbox-live 8080 120
wait_container_port sandbox-dev 8080 120
wait_container_http_healthy sandbox-live http://127.0.0.1:8080/health 120
wait_container_http_healthy sandbox-dev http://127.0.0.1:8080/health 120

echo "Deploy OK for ${RELEASE_SHA}"
