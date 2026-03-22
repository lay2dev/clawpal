#!/bin/bash
set -euo pipefail

echo "=== Recipe GUI E2E (Local Mode) ==="
echo "ClawPal and OpenClaw in the same container — no SSH"

mkdir -p "$SCREENSHOT_DIR" "$REPORT_DIR"

# Start Xvfb
Xvfb :99 -screen 0 1280x1024x24 &
sleep 2

# Start OpenClaw gateway
echo "Starting OpenClaw gateway..."
openclaw gateway start &
GATEWAY_PID=$!

# Wait for gateway to be ready
echo "Waiting for gateway..."
for i in $(seq 1 60); do
  if curl -sf http://127.0.0.1:18789/health >/dev/null 2>&1; then
    echo "Gateway ready after ${i}s"
    break
  fi
  sleep 1
done

# Start tauri-driver
tauri-driver --port 4444 &
sleep 2

# Run tests in local mode
echo "Running recipe E2E tests (local mode)..."
node recipe-e2e.mjs --mode=local || EXIT_CODE=$?

# Copy gateway logs for debugging
echo "--- gateway log ---"
cat /root/.openclaw/logs/*.log 2>/dev/null | tail -50 || true
echo "--- end gateway log ---"

exit ${EXIT_CODE:-0}
