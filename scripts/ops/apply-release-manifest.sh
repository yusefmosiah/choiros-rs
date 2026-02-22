#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: apply-release-manifest.sh <manifest-path>

Applies a previously-built release manifest on a host by switching
/opt/choiros/bin binaries to exact Nix store paths and restarting services.
EOF
}

if [ "$#" -eq 1 ] && { [ "$1" = "-h" ] || [ "$1" = "--help" ]; }; then
  usage
  exit 0
fi

if [ "$#" -ne 1 ]; then
  usage
  exit 1
fi

MANIFEST_PATH="$1"

if [ ! -f "$MANIFEST_PATH" ]; then
  echo "Manifest not found: $MANIFEST_PATH"
  exit 1
fi

if [ "${EUID}" -ne 0 ]; then
  echo "Run as root."
  exit 1
fi

# shellcheck disable=SC1090
source "$MANIFEST_PATH"

required_vars=(
  RELEASE_SHA
  SANDBOX_BIN
  HYPERVISOR_BIN
  DESKTOP_BIN
)

for var in "${required_vars[@]}"; do
  if [ -z "${!var:-}" ]; then
    echo "Manifest missing required key: ${var}"
    exit 1
  fi
done

for bin in "$SANDBOX_BIN" "$HYPERVISOR_BIN" "$DESKTOP_BIN"; do
  if [ ! -x "$bin" ]; then
    echo "Binary missing or not executable: $bin"
    exit 1
  fi
done

mkdir -p /opt/choiros/bin /opt/choiros/backups

BACKUP_DIR="/opt/choiros/backups/$(date -u +"%Y%m%dT%H%M%SZ")-${RELEASE_SHA:0:12}"
mkdir -p "$BACKUP_DIR"

capture_link() {
  local name="$1"
  local path="/opt/choiros/bin/$name"
  if [ -e "$path" ] || [ -L "$path" ]; then
    readlink -f "$path" > "${BACKUP_DIR}/${name}.previous-path"
  else
    printf "absent\n" > "${BACKUP_DIR}/${name}.previous-path"
  fi
}

capture_link sandbox
capture_link hypervisor
capture_link sandbox-ui

ln -sfn "$SANDBOX_BIN" /opt/choiros/bin/sandbox
ln -sfn "$HYPERVISOR_BIN" /opt/choiros/bin/hypervisor
ln -sfn "$DESKTOP_BIN" /opt/choiros/bin/sandbox-ui

cp "$MANIFEST_PATH" "${BACKUP_DIR}/applied-release.env"

systemctl restart container@sandbox-live container@sandbox-dev hypervisor
sleep 3

curl -fsS http://127.0.0.1:9090/health >/dev/null
curl -fsS http://127.0.0.1:8080/health >/dev/null
curl -fsS http://127.0.0.1:8081/health >/dev/null

echo "Applied release ${RELEASE_SHA}"
echo "Backup: ${BACKUP_DIR}"
echo "Health checks passed on :9090, :8080, :8081"
