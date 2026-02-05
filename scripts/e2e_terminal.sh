#!/usr/bin/env bash
set -euo pipefail

URL=${1:-http://localhost:3000}
OUT=${2:-tests/screenshots/terminal-e2e.png}
DESKTOP_ID=${3:-default-desktop}

# Ensure terminal app is registered in backend
curl -s -X POST http://localhost:8080/desktop/${DESKTOP_ID}/apps \
  -H 'Content-Type: application/json' \
  -d '{"id":"terminal","name":"Terminal","icon":"🖥️","component_code":"TerminalApp","default_width":700,"default_height":450}' \
  >/dev/null || true

# Open terminal window via backend
curl -s -X POST http://localhost:8080/desktop/${DESKTOP_ID}/windows \
  -H 'Content-Type: application/json' \
  -d '{"app_id":"terminal","title":"Terminal","props":null}' \
  >/dev/null || true

agent-browser connect 9222 >/dev/null 2>&1 || true

agent-browser open "$URL" >/dev/null
agent-browser wait 1000 >/dev/null

for i in {1..20}; do
  count=$(agent-browser eval "document.querySelectorAll('.xterm').length")
  if [ "${count}" != "0" ]; then
    break
  fi
  agent-browser wait 250 >/dev/null
  if [ "$i" = "20" ]; then
    echo "xterm not found"
    exit 1
  fi
done

container_id=$(agent-browser eval "document.querySelector('.terminal-container')?.id || ''" | tr -d '"')
if [ -z "${container_id}" ]; then
  echo "terminal container not found"
  exit 1
fi

agent-browser type "#${container_id} textarea" "echo hi\r" >/dev/null
agent-browser wait 500 >/dev/null

agent-browser screenshot "${OUT}" >/dev/null
agent-browser close >/dev/null

echo "ok: ${OUT}"
