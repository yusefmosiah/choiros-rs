#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: promote-grind-to-prod.sh --grind <ssh-host> --prod <ssh-host> [options]

Builds release outputs on grind, copies exact store paths to prod,
then applies the release manifest on prod.

Options:
  --grind <ssh-host>      SSH target for grind (required)
  --prod <ssh-host>       SSH target for prod (required)
  --repo <path>           Repo path on both hosts (default: /opt/choiros/workspace)
  --grind-repo <path>     Repo path on grind only (overrides --repo for grind)
  --prod-repo <path>      Repo path on prod only (overrides --repo for prod)
  --manifest <path>       Manifest path on grind/prod (default: /tmp/choiros-release.env)
EOF
}

GRIND_HOST=""
PROD_HOST=""
GRIND_REPO_PATH="/opt/choiros/workspace"
PROD_REPO_PATH="/opt/choiros/workspace"
MANIFEST_PATH="/tmp/choiros-release.env"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --grind)
      GRIND_HOST="$2"
      shift 2
      ;;
    --prod)
      PROD_HOST="$2"
      shift 2
      ;;
    --repo)
      GRIND_REPO_PATH="$2"
      PROD_REPO_PATH="$2"
      shift 2
      ;;
    --grind-repo)
      GRIND_REPO_PATH="$2"
      shift 2
      ;;
    --prod-repo)
      PROD_REPO_PATH="$2"
      shift 2
      ;;
    --manifest)
      MANIFEST_PATH="$2"
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

if [ -z "$GRIND_HOST" ] || [ -z "$PROD_HOST" ]; then
  usage
  exit 1
fi

if ! command -v nix >/dev/null 2>&1; then
  echo "Missing dependency: nix"
  exit 1
fi

if ! command -v ssh >/dev/null 2>&1; then
  echo "Missing dependency: ssh"
  exit 1
fi

TMP_MANIFEST="$(mktemp)"
cleanup() {
  rm -f "$TMP_MANIFEST"
}
trap cleanup EXIT

echo "Syncing grind repo to clean origin/main (${GRIND_HOST})"
ssh "$GRIND_HOST" "set -euo pipefail; cd '$GRIND_REPO_PATH'; git fetch origin; git checkout main; git pull --ff-only origin main; if [ -n \"\$(git status --porcelain)\" ]; then echo 'Grind repo is dirty after sync; commit/stash there first.'; exit 1; fi"

echo "Syncing prod repo to clean origin/main (${PROD_HOST})"
ssh "$PROD_HOST" "set -euo pipefail; cd '$PROD_REPO_PATH'; git fetch origin; git checkout main; git pull --ff-only origin main; if [ -n \"\$(git status --porcelain)\" ]; then echo 'Prod repo is dirty after sync; commit/stash there first.'; exit 1; fi"

GRIND_SHA="$(ssh "$GRIND_HOST" "set -euo pipefail; cd '$GRIND_REPO_PATH'; git rev-parse HEAD")"
PROD_SHA="$(ssh "$PROD_HOST" "set -euo pipefail; cd '$PROD_REPO_PATH'; git rev-parse HEAD")"

if [ "$GRIND_SHA" != "$PROD_SHA" ]; then
  echo "Refusing promote: grind and prod repo SHAs differ after sync."
  echo "grind: $GRIND_SHA"
  echo "prod:  $PROD_SHA"
  exit 1
fi

echo "Building release manifest on grind (${GRIND_HOST})"
ssh "$GRIND_HOST" "set -euo pipefail; cd '$GRIND_REPO_PATH'; ./scripts/ops/build-release-manifest.sh --manifest '$MANIFEST_PATH'"

echo "Fetching manifest from grind"
ssh "$GRIND_HOST" "cat '$MANIFEST_PATH'" > "$TMP_MANIFEST"

# shellcheck disable=SC1090
source "$TMP_MANIFEST"

if [ "${RELEASE_SHA:-}" != "$GRIND_SHA" ]; then
  echo "Refusing promote: manifest RELEASE_SHA does not match grind HEAD."
  echo "manifest: ${RELEASE_SHA:-missing}"
  echo "grind:    $GRIND_SHA"
  exit 1
fi

echo "Copying closures from grind to prod"
nix --extra-experimental-features nix-command copy \
  --from "ssh://${GRIND_HOST}" \
  --to "ssh://${PROD_HOST}" \
  "$SANDBOX_PATH" "$HYPERVISOR_PATH" "$DESKTOP_PATH"

echo "Uploading manifest to prod"
ssh "$PROD_HOST" "cat > '$MANIFEST_PATH'" < "$TMP_MANIFEST"

echo "Applying release on prod (${PROD_HOST})"
ssh "$PROD_HOST" "set -euo pipefail; cd '$PROD_REPO_PATH'; ./scripts/ops/apply-release-manifest.sh '$MANIFEST_PATH'"

echo "Promotion complete: ${RELEASE_SHA}"
