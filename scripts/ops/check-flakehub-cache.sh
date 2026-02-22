#!/usr/bin/env bash

set -euo pipefail

EXTRA_NIX_FLAGS=(--extra-experimental-features nix-command --extra-experimental-features flakes)

ok() {
  printf "ok: %s\n" "$1"
}

warn() {
  printf "warn: %s\n" "$1"
}

fail() {
  printf "error: %s\n" "$1" >&2
  exit 1
}

if ! command -v nix >/dev/null 2>&1; then
  fail "nix is required"
fi

if ! command -v grep >/dev/null 2>&1; then
  fail "grep is required"
fi

substituters="$(nix "${EXTRA_NIX_FLAGS[@]}" config show substituters 2>/dev/null || true)"
trusted_substituters="$(nix "${EXTRA_NIX_FLAGS[@]}" config show trusted-substituters 2>/dev/null || true)"
trusted_keys="$(nix "${EXTRA_NIX_FLAGS[@]}" config show trusted-public-keys 2>/dev/null || true)"
netrc_file="$(nix "${EXTRA_NIX_FLAGS[@]}" config show netrc-file 2>/dev/null || true)"

if printf "%s\n%s\n" "$substituters" "$trusted_substituters" | grep -q "https://cache.flakehub.com"; then
  ok "cache.flakehub.com is configured as substituter/trusted-substituter"
else
  fail "cache.flakehub.com is not configured in Nix substituters"
fi

if printf "%s\n" "$trusted_keys" | grep -qi "cache.flakehub.com"; then
  ok "flakehub cache public key is configured"
else
  warn "flakehub cache public key string not detected; verify trusted-public-keys"
fi

if [ -n "$netrc_file" ] && [ -f "$netrc_file" ]; then
  if grep -q "cache.flakehub.com" "$netrc_file"; then
    ok "netrc file exists and has cache.flakehub.com credentials"
  else
    warn "netrc file exists but no cache.flakehub.com entry found ($netrc_file)"
  fi
else
  warn "netrc file is not configured or missing"
fi

if nix "${EXTRA_NIX_FLAGS[@]}" store ping --store https://cache.flakehub.com >/dev/null 2>&1; then
  ok "nix store ping succeeded for cache.flakehub.com"
else
  fail "nix store ping failed for cache.flakehub.com"
fi

echo "flakehub cache check complete"
