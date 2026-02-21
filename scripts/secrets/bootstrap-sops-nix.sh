#!/usr/bin/env bash

set -euo pipefail

if ! command -v age-keygen >/dev/null 2>&1; then
  echo "Missing dependency: age-keygen"
  echo "Install via Nix: nix shell nixpkgs#age -c age-keygen -h"
  exit 1
fi

if ! command -v sops >/dev/null 2>&1; then
  echo "Missing dependency: sops"
  echo "Install via Nix: nix shell nixpkgs#sops -c sops --version"
  exit 1
fi

KEY_DIR="/var/lib/sops-nix"
KEY_FILE="${KEY_DIR}/key.txt"
SECRETS_DIR="infra/secrets"
EXAMPLE_FILE="${SECRETS_DIR}/choiros-platform.secrets.example.yaml"
TARGET_FILE="${SECRETS_DIR}/choiros-platform.secrets.sops.yaml"

if [ "${EUID}" -ne 0 ]; then
  echo "Run as root so key ownership and permissions are correct."
  exit 1
fi

mkdir -p "${KEY_DIR}"
chmod 700 "${KEY_DIR}"

if [ ! -f "${KEY_FILE}" ]; then
  age-keygen -o "${KEY_FILE}"
  chmod 600 "${KEY_FILE}"
  echo "Generated age key at ${KEY_FILE}"
else
  echo "Using existing key: ${KEY_FILE}"
fi

PUBLIC_KEY="$(age-keygen -y "${KEY_FILE}")"
echo "age public recipient: ${PUBLIC_KEY}"
echo
echo "Next steps:"
echo "1) Update .sops.yaml with this recipient and your teammates'."
echo "2) Copy ${EXAMPLE_FILE} to ${TARGET_FILE} and set real values."
echo "3) Encrypt: SOPS_AGE_RECIPIENTS='${PUBLIC_KEY}' sops --encrypt --in-place ${TARGET_FILE}"
echo "4) Set services.choiros.platformSecrets.sopsFile to ${TARGET_FILE} in host config."
