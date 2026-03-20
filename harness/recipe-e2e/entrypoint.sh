#!/bin/bash
set -euo pipefail

echo "=== ClawPal Recipe GUI E2E Harness ==="

export DISPLAY="${DISPLAY:-:99}"
export SCREENSHOT_DIR="${SCREENSHOT_DIR:-/screenshots}"
export REPORT_DIR="${REPORT_DIR:-/report}"
export APP_BINARY="${APP_BINARY:-/usr/local/bin/clawpal}"
export OPENCLAW_IMAGE="${OPENCLAW_IMAGE:-clawpal-recipe-openclaw:latest}"
export OPENCLAW_CONTAINER_NAME="${OPENCLAW_CONTAINER_NAME:-clawpal-recipe-e2e}"
export OPENCLAW_SSH_HOST="${OPENCLAW_SSH_HOST:-127.0.0.1}"
export OPENCLAW_SSH_PORT="${OPENCLAW_SSH_PORT:-2222}"
export OPENCLAW_SSH_USER="${OPENCLAW_SSH_USER:-root}"
export OPENCLAW_SSH_PASSWORD="${OPENCLAW_SSH_PASSWORD:-clawpal-recipe-e2e}"

mkdir -p "${SCREENSHOT_DIR}" "${REPORT_DIR}" /tmp/runtime
eval "$(dbus-launch --sh-syntax)"
export DBUS_SESSION_BUS_ADDRESS

DRIVER_PID=""
XVFB_PID=""

cleanup() {
  local status=$?

  if docker ps -a --format '{{.Names}}' | grep -qx "${OPENCLAW_CONTAINER_NAME}"; then
    echo "--- inner OpenClaw container logs ---"
    docker logs "${OPENCLAW_CONTAINER_NAME}" 2>&1 || true
    echo "--- end inner logs ---"
    docker rm -f "${OPENCLAW_CONTAINER_NAME}" >/dev/null 2>&1 || true
  fi

  if [ -n "${DRIVER_PID}" ]; then
    kill "${DRIVER_PID}" 2>/dev/null || true
  fi
  if [ -n "${XVFB_PID}" ]; then
    kill "${XVFB_PID}" 2>/dev/null || true
  fi

  exit "${status}"
}

trap cleanup EXIT

Xvfb "${DISPLAY}" -screen 0 1440x960x24 -ac +extension GLX +render -noreset &
XVFB_PID=$!
sleep 1
echo "Xvfb started on ${DISPLAY}"

DISPLAY="${DISPLAY}" tauri-driver &
DRIVER_PID=$!
sleep 2

if ! kill -0 "${DRIVER_PID}" 2>/dev/null; then
  echo "ERROR: tauri-driver failed to start"
  exit 1
fi
echo "tauri-driver listening on :4444"

if ! docker image inspect "${OPENCLAW_IMAGE}" >/dev/null 2>&1; then
  echo "Building ${OPENCLAW_IMAGE} from /workspace"
  docker build \
    -t "${OPENCLAW_IMAGE}" \
    -f /workspace/harness/recipe-e2e/openclaw-container/Dockerfile \
    /workspace
fi

docker rm -f "${OPENCLAW_CONTAINER_NAME}" >/dev/null 2>&1 || true
docker run -d \
  --name "${OPENCLAW_CONTAINER_NAME}" \
  -p "${OPENCLAW_SSH_PORT}:22" \
  "${OPENCLAW_IMAGE}" >/dev/null

echo "Waiting for SSH on ${OPENCLAW_SSH_HOST}:${OPENCLAW_SSH_PORT}"
for attempt in $(seq 1 60); do
  if sshpass -p "${OPENCLAW_SSH_PASSWORD}" ssh \
    -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null \
    -o LogLevel=ERROR \
    -o ConnectTimeout=2 \
    -p "${OPENCLAW_SSH_PORT}" \
    "${OPENCLAW_SSH_USER}@${OPENCLAW_SSH_HOST}" \
    "true" >/dev/null 2>&1; then
    echo "SSH ready after ${attempt} attempt(s)"
    break
  fi
  if [ "${attempt}" -eq 60 ]; then
    echo "ERROR: timed out waiting for SSH"
    exit 1
  fi
  sleep 2
done

echo "Waiting for OpenClaw gateway readiness"
for attempt in $(seq 1 60); do
  if sshpass -p "${OPENCLAW_SSH_PASSWORD}" ssh \
    -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null \
    -o LogLevel=ERROR \
    -o ConnectTimeout=3 \
    -p "${OPENCLAW_SSH_PORT}" \
    "${OPENCLAW_SSH_USER}@${OPENCLAW_SSH_HOST}" \
    "export PATH=/root/.local/bin:/usr/local/bin:\$PATH; openclaw gateway status >/tmp/gateway-status.txt 2>&1 && cat /tmp/gateway-status.txt" >/dev/null 2>&1; then
    echo "Gateway ready after ${attempt} attempt(s)"
    break
  fi
  if [ "${attempt}" -eq 60 ]; then
    echo "ERROR: timed out waiting for gateway"
    exit 1
  fi
  sleep 2
done

echo "Docker containers:"
docker ps -a 2>/dev/null || true
echo "SSH port check:"
ss -tlnp | grep 2222 || true

cd /harness
node /harness/recipe-e2e.mjs "$@"
