#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "Local DMG preview only supports macOS."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TARGET=""
MODE="auto"
while [ $# -gt 0 ]; do
  case "$1" in
    --target)
      TARGET="${2:-}"
      shift 2
      ;;
    --finder)
      MODE="finder"
      shift
      ;;
    --headless)
      MODE="headless"
      shift
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--target aarch64-apple-darwin|x86_64-apple-darwin] [--finder|--headless]"
      exit 1
      ;;
  esac
done

if [ -z "$TARGET" ]; then
  if [ "$(uname -m)" = "arm64" ]; then
    TARGET="aarch64-apple-darwin"
  else
    TARGET="x86_64-apple-darwin"
  fi
fi

cd "$REPO_ROOT"

bash scripts/generate-dmg-background.sh

build_with_finder() {
  echo "Building DMG with Finder aesthetics enabled..."
  env \
    TAURI_BUNDLER_DMG_IGNORE_CI=true \
    bunx @tauri-apps/cli build \
      --bundles dmg \
      --target "$TARGET" \
      --ci \
      --no-sign \
      --config '{"bundle":{"createUpdaterArtifacts":false,"macOS":{"signingIdentity":"-"}}}'
}

build_headless() {
  echo "Building DMG in headless mode (no Finder aesthetics)..."
  env \
    CI=true \
    bunx @tauri-apps/cli build \
      --bundles dmg \
      --target "$TARGET" \
      --ci \
      --no-sign \
      --config '{"bundle":{"createUpdaterArtifacts":false,"macOS":{"signingIdentity":"-"}}}'
}

if [ "$MODE" = "finder" ]; then
  build_with_finder
elif [ "$MODE" = "headless" ]; then
  build_headless
else
  if /usr/bin/osascript -e 'tell application "Finder" to get name of startup disk' >/dev/null 2>&1; then
    build_with_finder || {
      echo "Finder mode failed, falling back to headless mode..."
      build_headless
    }
  else
    echo "Finder automation unavailable, using headless mode..."
    build_headless
  fi
fi

DMG_PATH=""
for bundle_root in \
  "target/${TARGET}/release/bundle" \
  "src-tauri/target/${TARGET}/release/bundle"
do
  if [ -d "$bundle_root/dmg" ]; then
    DMG_PATH="$(find "$bundle_root/dmg" -maxdepth 1 -type f -name '*.dmg' | head -n 1 || true)"
    if [ -n "$DMG_PATH" ]; then
      break
    fi
  fi
done

if [ -z "$DMG_PATH" ]; then
  echo "Failed to locate DMG artifact for target: $TARGET"
  exit 1
fi

bash scripts/verify-dmg-layout.sh "$DMG_PATH" "ClawPal.app"
echo "Local DMG preview complete: $DMG_PATH"
