#!/bin/bash
set -euo pipefail

export PATH="/root/.local/bin:/usr/local/bin:${PATH}"

mkdir -p /var/run/sshd
/usr/sbin/sshd

nohup openclaw gateway start >/tmp/openclaw-gateway.log 2>&1 &
GATEWAY_PID=$!

cleanup() {
  kill "${GATEWAY_PID}" 2>/dev/null || true
}

trap cleanup EXIT

while kill -0 "${GATEWAY_PID}" 2>/dev/null; do
  sleep 2
done

wait "${GATEWAY_PID}"
