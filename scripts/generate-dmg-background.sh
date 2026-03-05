#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This script only supports macOS."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

OUTPUT_PATH="${1:-src-tauri/.generated/dmg-background.png}"
if [[ "$OUTPUT_PATH" = /* ]]; then
  ABS_OUTPUT_PATH="$OUTPUT_PATH"
else
  ABS_OUTPUT_PATH="$REPO_ROOT/$OUTPUT_PATH"
fi

mkdir -p "$(dirname "$ABS_OUTPUT_PATH")"
swift "$SCRIPT_DIR/generate_dmg_background.swift" "$ABS_OUTPUT_PATH"
echo "Generated DMG background: $ABS_OUTPUT_PATH"
