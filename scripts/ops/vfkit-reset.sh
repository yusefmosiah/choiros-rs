#!/usr/bin/env bash
set -euo pipefail

STATE_DIR="${CHOIR_VFKIT_STATE_DIR:-$HOME/.local/share/choiros/vfkit}"
VM_DIR="$STATE_DIR/vms"
RUNTIME_DIR="$STATE_DIR/runtimes"

pkill -f '/bin/vfkit --cpus' >/dev/null 2>&1 || true
pkill -f 'nixosConfigurations.choiros-vfkit-user.config.microvm.runner.vfkit' >/dev/null 2>&1 || true
pkill -f 'ssh .*127.0.0.1:[0-9][0-9][0-9][0-9]:127.0.0.1:[0-9][0-9][0-9][0-9]' >/dev/null 2>&1 || true

if [[ -d "$VM_DIR" ]]; then
  find "$VM_DIR" -type f -name '*.pid' -delete
fi
if [[ -d "$RUNTIME_DIR" ]]; then
  find "$RUNTIME_DIR" -type f -name '*.pid' -delete
fi

num_files="$(sysctl -n kern.num_files 2>/dev/null || true)"
max_files="$(sysctl -n kern.maxfiles 2>/dev/null || true)"
if [[ -n "$num_files" && -n "$max_files" ]]; then
  echo "vfkit reset complete (kern.num_files=${num_files}/${max_files})"
else
  echo "vfkit reset complete"
fi
