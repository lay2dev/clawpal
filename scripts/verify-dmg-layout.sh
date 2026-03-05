#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "::error::DMG verification only supports macOS runners"
  exit 1
fi

if [ $# -lt 1 ]; then
  echo "Usage: $0 <path-to-dmg> [app-bundle-name]"
  exit 1
fi

DMG_PATH="$1"
APP_NAME="${2:-ClawPal.app}"

if [ ! -f "$DMG_PATH" ]; then
  echo "::error::DMG not found: $DMG_PATH"
  exit 1
fi

MOUNT_POINT="$(mktemp -d /tmp/clawpal-dmg-verify.XXXXXX)"
cleanup() {
  local resolved_mount=""
  resolved_mount="$(cd "$MOUNT_POINT" 2>/dev/null && pwd -P || true)"

  for mp in "$MOUNT_POINT" "$resolved_mount"; do
    [ -z "$mp" ] && continue
    if mount | awk '{print $3}' | grep -Fxq "$mp"; then
      hdiutil detach "$mp" -force >/dev/null 2>&1 || true
    fi
  done

  rm -rf "$MOUNT_POINT" >/dev/null 2>&1 || true
}
trap cleanup EXIT

hdiutil attach "$DMG_PATH" -nobrowse -readonly -mountpoint "$MOUNT_POINT" >/dev/null

if [ ! -d "$MOUNT_POINT/$APP_NAME" ]; then
  echo "::error::Missing app bundle in DMG: $APP_NAME"
  exit 1
fi

if [ ! -L "$MOUNT_POINT/Applications" ]; then
  echo "::error::Missing Applications symlink in DMG"
  exit 1
fi

APP_LINK_TARGET="$(readlink "$MOUNT_POINT/Applications" || true)"
if [ "$APP_LINK_TARGET" != "/Applications" ]; then
  echo "::error::Applications symlink target mismatch: $APP_LINK_TARGET"
  exit 1
fi

if [ ! -f "$MOUNT_POINT/.background/dmg-background.png" ]; then
  echo "::error::Missing DMG background asset: .background/dmg-background.png"
  exit 1
fi

echo "Verified DMG layout: $DMG_PATH"
