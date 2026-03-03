#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: validate-local-provider-matrix.sh [options]

Runs a local provider validation matrix before host deploys.

Lanes:
  1) Model provider live matrix tests (sandbox test binary)
  2) Gateway search smokes (tavily/brave/exa via hypervisor provider gateway)

Options:
  --models <csv>            Model ids for live matrix (default: CHOIR_LIVE_MODEL_IDS or ZaiGLM47Flash,KimiK25,InceptionMercury2)
  --gateway-base <url>      Provider gateway base URL (default: CHOIR_PROVIDER_GATEWAY_BASE_URL or http://127.0.0.1:9090)
  --gateway-token <token>   Provider gateway token (default: CHOIR_PROVIDER_GATEWAY_TOKEN)
  --skip-model-tests        Skip model live matrix tests
  --skip-gateway-search     Skip gateway search smoke tests
  --codex-openai-bridge     If OPENAI_API_KEY is missing, load it from ${CODEX_HOME:-$HOME/.codex}/auth.json
  -h, --help                Show this help

Examples:
  ./scripts/ops/validate-local-provider-matrix.sh
  ./scripts/ops/validate-local-provider-matrix.sh --models "OpenAIGPT5CodexDev,KimiK25" --codex-openai-bridge
  ./scripts/ops/validate-local-provider-matrix.sh --skip-model-tests --gateway-token "$CHOIR_PROVIDER_GATEWAY_TOKEN"
EOF
}

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing dependency: $cmd" >&2
    exit 1
  fi
}

MODELS="${CHOIR_LIVE_MODEL_IDS:-ZaiGLM47Flash,KimiK25,InceptionMercury2}"
GATEWAY_BASE="${CHOIR_PROVIDER_GATEWAY_BASE_URL:-http://127.0.0.1:9090}"
GATEWAY_TOKEN="${CHOIR_PROVIDER_GATEWAY_TOKEN:-}"
SKIP_MODEL_TESTS="false"
SKIP_GATEWAY_SEARCH="false"
CODEX_OPENAI_BRIDGE="false"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --models)
      MODELS="$2"
      shift 2
      ;;
    --gateway-base)
      GATEWAY_BASE="$2"
      shift 2
      ;;
    --gateway-token)
      GATEWAY_TOKEN="$2"
      shift 2
      ;;
    --skip-model-tests)
      SKIP_MODEL_TESTS="true"
      shift
      ;;
    --skip-gateway-search)
      SKIP_GATEWAY_SEARCH="true"
      shift
      ;;
    --codex-openai-bridge)
      CODEX_OPENAI_BRIDGE="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

need_cmd cargo
need_cmd curl
need_cmd jq

if [ "$CODEX_OPENAI_BRIDGE" = "true" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
  AUTH_PATH="${CODEX_HOME:-$HOME/.codex}/auth.json"
  if [ ! -f "$AUTH_PATH" ]; then
    echo "Codex auth file not found: $AUTH_PATH" >&2
    exit 1
  fi
  OPENAI_API_KEY="$(jq -r '.OPENAI_API_KEY // empty' "$AUTH_PATH")"
  export OPENAI_API_KEY
  if [ -z "$OPENAI_API_KEY" ]; then
    echo "OPENAI_API_KEY missing in $AUTH_PATH" >&2
    exit 1
  fi
  echo "Loaded OPENAI_API_KEY from Codex auth bridge."
fi

PASS_COUNT=0
FAIL_COUNT=0
RESULTS=()

record_pass() {
  local label="$1"
  PASS_COUNT=$((PASS_COUNT + 1))
  RESULTS+=("PASS ${label}")
}

record_fail() {
  local label="$1"
  local detail="$2"
  FAIL_COUNT=$((FAIL_COUNT + 1))
  RESULTS+=("FAIL ${label}: ${detail}")
}

run_model_matrix() {
  local models_csv="$1"
  echo "== Model provider matrix =="
  echo "models=${models_csv}"
  if CHOIR_LIVE_MODEL_IDS="$models_csv" \
    cargo test -p sandbox --test model_provider_live_test live_provider_smoke_matrix -- --nocapture; then
    record_pass "model_provider_live_test/live_provider_smoke_matrix"
  else
    record_fail "model_provider_live_test/live_provider_smoke_matrix" "see cargo output above"
  fi

  if CHOIR_LIVE_MODEL_IDS="$models_csv" \
    cargo test -p sandbox --test model_provider_live_test live_decide_matrix -- --nocapture; then
    record_pass "model_provider_live_test/live_decide_matrix"
  else
    record_fail "model_provider_live_test/live_decide_matrix" "see cargo output above"
  fi
}

gateway_curl() {
  local method="$1"
  local url="$2"
  local upstream="$3"
  local model_label="$4"
  local body="${5:-}"
  local tmp_body
  tmp_body="$(mktemp)"
  local code

  if [ "$method" = "GET" ]; then
    code="$(
      curl -sS -o "$tmp_body" -w "%{http_code}" \
        -X GET "$url" \
        -H "Authorization: Bearer ${GATEWAY_TOKEN}" \
        -H "x-choiros-upstream-base-url: ${upstream}" \
        -H "x-choiros-sandbox-id: local:matrix" \
        -H "x-choiros-user-id: local" \
        -H "x-choiros-model: ${model_label}"
    )"
  else
    code="$(
      curl -sS -o "$tmp_body" -w "%{http_code}" \
        -X POST "$url" \
        -H "Authorization: Bearer ${GATEWAY_TOKEN}" \
        -H "Content-Type: application/json" \
        -H "x-choiros-upstream-base-url: ${upstream}" \
        -H "x-choiros-sandbox-id: local:matrix" \
        -H "x-choiros-user-id: local" \
        -H "x-choiros-model: ${model_label}" \
        -d "$body"
    )"
  fi

  echo "$code|$tmp_body"
}

run_gateway_search_matrix() {
  local base="$1"
  local token="$2"

  if [ -z "$token" ]; then
    record_fail "gateway-search-preflight" "missing CHOIR_PROVIDER_GATEWAY_TOKEN / --gateway-token"
    return
  fi

  echo "== Gateway search matrix =="
  echo "gateway_base=${base}"

  local tavily_out tavily_code tavily_body
  tavily_out="$(gateway_curl "POST" "${base%/}/provider/v1/search/search" "https://api.tavily.com" "search:tavily" '{"query":"weather in boston","max_results":2,"search_depth":"basic","include_answer":false,"include_raw_content":false}')"
  tavily_code="${tavily_out%%|*}"
  tavily_body="${tavily_out#*|}"
  if [ "$tavily_code" = "200" ] && jq -e '.results or .answer or .query' "$tavily_body" >/dev/null 2>&1; then
    record_pass "gateway-search:tavily"
  else
    record_fail "gateway-search:tavily" "status=${tavily_code} body=$(head -c 220 "$tavily_body")"
  fi
  rm -f "$tavily_body"

  local brave_out brave_code brave_body
  brave_out="$(gateway_curl "GET" "${base%/}/provider/v1/search/res/v1/web/search?q=weather+in+boston&count=2" "https://api.search.brave.com" "search:brave")"
  brave_code="${brave_out%%|*}"
  brave_body="${brave_out#*|}"
  if [ "$brave_code" = "200" ] && jq -e '.web or .results' "$brave_body" >/dev/null 2>&1; then
    record_pass "gateway-search:brave"
  else
    record_fail "gateway-search:brave" "status=${brave_code} body=$(head -c 220 "$brave_body")"
  fi
  rm -f "$brave_body"

  local exa_out exa_code exa_body
  exa_out="$(gateway_curl "POST" "${base%/}/provider/v1/search/search" "https://api.exa.ai" "search:exa" '{"query":"weather in boston","numResults":2,"type":"auto","contents":{"text":true}}')"
  exa_code="${exa_out%%|*}"
  exa_body="${exa_out#*|}"
  if [ "$exa_code" = "200" ] && jq -e '.results' "$exa_body" >/dev/null 2>&1; then
    record_pass "gateway-search:exa"
  else
    record_fail "gateway-search:exa" "status=${exa_code} body=$(head -c 220 "$exa_body")"
  fi
  rm -f "$exa_body"
}

if [ "$SKIP_MODEL_TESTS" != "true" ]; then
  run_model_matrix "$MODELS"
else
  echo "Skipping model provider matrix (--skip-model-tests)."
fi

if [ "$SKIP_GATEWAY_SEARCH" != "true" ]; then
  run_gateway_search_matrix "$GATEWAY_BASE" "$GATEWAY_TOKEN"
else
  echo "Skipping gateway search matrix (--skip-gateway-search)."
fi

echo
echo "== Provider Matrix Summary =="
for row in "${RESULTS[@]}"; do
  echo "$row"
done
echo "passes=${PASS_COUNT} failures=${FAIL_COUNT}"

if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 1
fi
