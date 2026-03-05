#!/run/current-system/sw/bin/bash
set -euo pipefail

# Minimal runtime-ctl for OVH bare metal.
# Sandboxes run as systemd services; this script is a passthrough.

ACTION="${1:-}"
shift || true

while [[ $# -gt 0 ]]; do
  case "$1" in
    --user-id|--runtime|--port|--role|--branch)
      shift 2 ;;
    *) shift ;;
  esac
done

case "$ACTION" in
  ensure)
    # Sandboxes are managed by systemd; nothing to do.
    exit 0
    ;;
  stop)
    # Sandboxes are managed by systemd; nothing to do.
    exit 0
    ;;
  *)
    echo "invalid action '$ACTION' (expected ensure|stop)" >&2
    exit 2
    ;;
esac
