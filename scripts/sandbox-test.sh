#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Fast sandbox test runner (avoids broad filtered cargo test runs).

Usage:
  scripts/sandbox-test.sh --lib [<lib_filter> ...]
  scripts/sandbox-test.sh --test <integration_test_binary> [<test_filter> ...]
  scripts/sandbox-test.sh --conductor

Examples:
  scripts/sandbox-test.sh --lib conductor
  scripts/sandbox-test.sh --test conductor_api_test
  scripts/sandbox-test.sh --test conductor_api_test test_conductor_execute_endpoint
  scripts/sandbox-test.sh --conductor
EOF
}

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

mode="$1"
shift

case "${mode}" in
  --lib)
    exec cargo test -p sandbox --lib "$@"
    ;;
  --test)
    if [[ $# -lt 1 ]]; then
      echo "error: --test requires an integration test binary name" >&2
      usage
      exit 2
    fi
    test_binary="$1"
    shift
    exec cargo test -p sandbox --test "${test_binary}" "$@"
    ;;
  --conductor)
    cargo test -p sandbox --lib conductor -- --nocapture
    cargo test -p sandbox --test conductor_api_test -- --nocapture
    ;;
  *)
    cat >&2 <<'EOF'
error: refusing ambiguous/broad test invocation.

Use one of:
  --lib ...
  --test <integration_test_binary> ...
  --conductor

Do not use broad filtered runs like:
  cargo test -p sandbox conductor
EOF
    exit 2
    ;;
esac
