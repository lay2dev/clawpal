#!/bin/bash
set -euo pipefail

echo "=== ClawPal Screenshot Harness ==="

# D-Bus (GTK requirement)
mkdir -p /tmp/runtime
eval $(dbus-launch --sh-syntax)
export DBUS_SESSION_BUS_ADDRESS

# Xvfb
Xvfb :99 -screen 0 1200x820x24 -ac +extension GLX +render -noreset &
sleep 1
echo "Xvfb started on :99"

# tauri-driver (WebDriver on :4444)
DISPLAY=:99 tauri-driver &
DRIVER_PID=$!
sleep 2

if ! kill -0 $DRIVER_PID 2>/dev/null; then
    echo "ERROR: tauri-driver failed to start"
    exit 1
fi
echo "tauri-driver listening on :4444"

# Run capture
cd /harness
node capture.mjs "$@"
EXIT_CODE=$?

kill $DRIVER_PID 2>/dev/null || true
echo "=== Done ==="
exit $EXIT_CODE
