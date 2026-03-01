#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UI_DIST="$ROOT_DIR/dioxus-desktop/target/dx/dioxus-desktop/release/web/public"
PROBE_BUILDER="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --probe-builder)
      PROBE_BUILDER="true"
      shift
      ;;
    --help|-h)
      cat <<'USAGE'
Usage:
  check-local-cutover-status.sh [--probe-builder]

Checks:
  - UI release asset availability
  - Nix config gate (trusted-users/extra-platforms/builders)
  - Linux builder registration (/etc/nix/machines)
  - Optional live aarch64-linux builder probe
  - Hypervisor health endpoint
  - Runtime control binary presence
USAGE
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

PASS_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0
NEEDS_UI_BUILD="false"
NEEDS_BUILDER_REG="false"
NEEDS_EXTRA_PLATFORMS_FIX="false"

pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf 'PASS %s\n' "$*"
}

warn() {
  WARN_COUNT=$((WARN_COUNT + 1))
  printf 'WARN %s\n' "$*"
}

fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf 'FAIL %s\n' "$*"
}

if [[ -f "$UI_DIST/index.html" ]]; then
  pass "UI dist present at $UI_DIST"
else
  fail "UI dist missing at $UI_DIST (run: just local-build-ui)"
  NEEDS_UI_BUILD="true"
fi

if [[ -x "$ROOT_DIR/target/debug/vfkit-runtime-ctl" ]]; then
  pass "vfkit runtime control binary present (target/debug/vfkit-runtime-ctl)"
elif [[ -f "$ROOT_DIR/hypervisor/src/bin/vfkit-runtime-ctl.rs" ]]; then
  warn "vfkit runtime control source present but binary not built yet (run: just build-vfkit-ctl)"
elif [[ -x "$ROOT_DIR/scripts/ops/vfkit-runtime-ctl.sh" ]]; then
  warn "vfkit runtime control dispatcher present but binary is missing (run: just build-vfkit-ctl)"
else
  fail "missing vfkit runtime control entrypoints (binary/source/dispatcher)"
fi

if [[ -x "$ROOT_DIR/scripts/ops/bootstrap-local-linux-builder.sh" ]]; then
  pass "local linux builder bootstrap script present"
else
  fail "missing scripts/ops/bootstrap-local-linux-builder.sh"
fi

if command -v nix >/dev/null 2>&1; then
  pass "nix command available"
else
  fail "nix command missing"
fi

NIX_CONFIG="$(nix config show 2>/dev/null || true)"
HOST_OS="$(uname -s)"
if printf '%s\n' "$NIX_CONFIG" | rg -q '^trusted-users = .*\<'"$USER"'\>'; then
  pass "nix trusted-users includes current user ($USER)"
else
  warn "nix trusted-users may not include current user ($USER)"
fi

if [[ "$HOST_OS" == "Darwin" ]]; then
  if printf '%s\n' "$NIX_CONFIG" | rg -q '^extra-platforms = .*(aarch64-linux|x86_64-linux)'; then
    fail "nix extra-platforms advertises Linux on macOS (this breaks remote Linux offload scheduling)"
    NEEDS_EXTRA_PLATFORMS_FIX="true"
  else
    pass "nix extra-platforms does not advertise Linux on macOS"
  fi

  if printf '%s\n' "$NIX_CONFIG" | rg -q '^extra-platforms = .*x86_64-darwin'; then
    pass "nix extra-platforms includes x86_64-darwin"
  else
    warn "nix extra-platforms missing x86_64-darwin (Rosetta builds may be unavailable)"
  fi
else
  warn "non-macOS host detected; skipping macOS-specific extra-platforms checks"
fi

if printf '%s\n' "$NIX_CONFIG" | rg -q '^builders = @/etc/nix/machines'; then
  pass "nix configured to read /etc/nix/machines"
else
  warn "nix builders is not @/etc/nix/machines"
fi

BUILDER_LINE=""
if [[ -f /etc/nix/machines ]]; then
  BUILDER_LINE="$(awk 'NF && $1 !~ /^#/ { print; exit }' /etc/nix/machines || true)"
  if [[ -n "$BUILDER_LINE" ]] && printf '%s\n' "$BUILDER_LINE" | rg -q 'aarch64-linux'; then
    pass "aarch64-linux builder entry found in /etc/nix/machines"
  elif [[ -n "$BUILDER_LINE" ]]; then
    warn "builder entry found but first entry is not aarch64-linux: $BUILDER_LINE"
  else
    fail "/etc/nix/machines exists but has no active builder entries"
  fi
  else
    fail "/etc/nix/machines missing"
    NEEDS_BUILDER_REG="true"
  fi

if curl -fsS http://127.0.0.1:9090/login >/dev/null 2>&1; then
  pass "hypervisor health endpoint reachable (http://127.0.0.1:9090/login)"
else
  warn "hypervisor not reachable at http://127.0.0.1:9090/login"
fi

if [[ "$PROBE_BUILDER" == "true" ]]; then
  if [[ -z "$BUILDER_LINE" ]]; then
    fail "cannot run builder probe without an active /etc/nix/machines entry"
  else
    echo "INFO running live builder probe against: $BUILDER_LINE"
    if nix build --impure --max-jobs 0 \
      --builders "$BUILDER_LINE" \
      --expr 'let pkgs = import (builtins.getFlake "flake:nixpkgs").outPath { system = "aarch64-linux"; }; in pkgs.runCommand ("linux-builder-probe-" + toString builtins.currentTime) {} "echo ok > $out"' \
      >/dev/null 2>&1; then
      pass "live aarch64-linux builder probe passed"
    else
      fail "live aarch64-linux builder probe failed"
    fi
  fi
else
  echo "INFO builder probe skipped (pass --probe-builder to enable)"
fi

echo
printf 'Summary: %d pass, %d warn, %d fail\n' "$PASS_COUNT" "$WARN_COUNT" "$FAIL_COUNT"

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo
  echo "Suggested next commands:"
  if [[ "$NEEDS_EXTRA_PLATFORMS_FIX" == "true" ]]; then
    echo "  sudo sh -c 'test -f /etc/nix/nix.custom.conf || touch /etc/nix/nix.custom.conf; tmp=\"\$(mktemp)\"; grep -v \"^extra-platforms = \" /etc/nix/nix.custom.conf > \"\$tmp\"; echo \"extra-platforms = x86_64-darwin\" >> \"\$tmp\"; mv \"\$tmp\" /etc/nix/nix.custom.conf'"
    echo "  sudo launchctl kickstart -k system/systems.determinate.nix-daemon || sudo launchctl kickstart -k system/org.nixos.nix-daemon || sudo launchctl kickstart -k system/systems.nix-daemon"
  fi
  if [[ "$NEEDS_UI_BUILD" == "true" ]]; then
    echo "  just local-build-ui"
  fi
  if [[ "$NEEDS_BUILDER_REG" == "true" ]]; then
    echo "  just builder-bootstrap-utm <utm-vm-name>"
  fi
  echo "  just cutover-status --probe-builder"
  exit 1
fi

exit 0
