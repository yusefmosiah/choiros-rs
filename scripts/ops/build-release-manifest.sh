#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: build-release-manifest.sh [--manifest <path>] [--allow-dirty]

Builds sandbox, hypervisor, and desktop flake outputs and writes a release manifest.

Options:
  --manifest <path>  Output manifest path (default: artifacts/releases/<sha>.env)
  --allow-dirty      Allow manifest generation from a dirty git tree
EOF
}

EXTRA_NIX_FLAGS=(--extra-experimental-features nix-command --extra-experimental-features flakes)
ALLOW_DIRTY="false"
MANIFEST_PATH=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --manifest)
      if [ "$#" -lt 2 ]; then
        echo "Missing value for --manifest"
        exit 1
      fi
      MANIFEST_PATH="$2"
      shift 2
      ;;
    --allow-dirty)
      ALLOW_DIRTY="true"
      shift
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

if [ ! -f "flake.nix" ] || [ ! -d "sandbox" ] || [ ! -d "hypervisor" ] || [ ! -d "dioxus-desktop" ]; then
  echo "Run this script from repository root."
  exit 1
fi

if ! command -v nix >/dev/null 2>&1; then
  echo "Missing dependency: nix"
  exit 1
fi

if ! command -v git >/dev/null 2>&1; then
  echo "Missing dependency: git"
  exit 1
fi

RELEASE_SHA="$(git rev-parse HEAD)"
SHORT_SHA="$(git rev-parse --short HEAD)"
GIT_DIRTY="$( [ -n "$(git status --porcelain)" ] && echo "true" || echo "false" )"

if [ "$ALLOW_DIRTY" != "true" ] && [ "$GIT_DIRTY" = "true" ]; then
  echo "Refusing to build release manifest from dirty tree."
  echo "Commit/stash changes or re-run with --allow-dirty."
  exit 1
fi

if [ -z "$MANIFEST_PATH" ]; then
  MANIFEST_PATH="artifacts/releases/${SHORT_SHA}.env"
fi

mkdir -p "$(dirname "$MANIFEST_PATH")"

echo "Building flake outputs for ${SHORT_SHA}"
SANDBOX_PATH="$(nix "${EXTRA_NIX_FLAGS[@]}" build ./sandbox#sandbox --no-link --print-out-paths)"
HYPERVISOR_PATH="$(nix "${EXTRA_NIX_FLAGS[@]}" build ./hypervisor#hypervisor --no-link --print-out-paths)"
DESKTOP_PATH="$(nix "${EXTRA_NIX_FLAGS[@]}" build ./dioxus-desktop#desktop --no-link --print-out-paths)"

if [ ! -x "${SANDBOX_PATH}/bin/sandbox" ]; then
  echo "Missing sandbox binary at ${SANDBOX_PATH}/bin/sandbox"
  exit 1
fi

if [ ! -x "${HYPERVISOR_PATH}/bin/hypervisor" ]; then
  echo "Missing hypervisor binary at ${HYPERVISOR_PATH}/bin/hypervisor"
  exit 1
fi

if [ ! -x "${DESKTOP_PATH}/bin/sandbox-ui" ]; then
  echo "Missing desktop binary at ${DESKTOP_PATH}/bin/sandbox-ui"
  exit 1
fi

cat > "$MANIFEST_PATH" <<EOF
RELEASE_SHA=${RELEASE_SHA}
RELEASE_SHORT_SHA=${SHORT_SHA}
RELEASE_CREATED_AT=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
REPO_ROOT=$(pwd)
GIT_DIRTY=${GIT_DIRTY}
SANDBOX_PATH=${SANDBOX_PATH}
HYPERVISOR_PATH=${HYPERVISOR_PATH}
DESKTOP_PATH=${DESKTOP_PATH}
SANDBOX_BIN=${SANDBOX_PATH}/bin/sandbox
HYPERVISOR_BIN=${HYPERVISOR_PATH}/bin/hypervisor
DESKTOP_BIN=${DESKTOP_PATH}/bin/sandbox-ui
EOF

echo "Release manifest written to ${MANIFEST_PATH}"
echo "SHA: ${RELEASE_SHA}"
echo "Sandbox: ${SANDBOX_PATH}"
echo "Hypervisor: ${HYPERVISOR_PATH}"
echo "Desktop: ${DESKTOP_PATH}"
