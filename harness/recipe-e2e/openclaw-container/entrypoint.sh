#!/bin/bash
set -euo pipefail

export PATH="/root/.local/bin:/usr/local/bin:${PATH}"

mkdir -p /var/run/sshd
/usr/sbin/sshd

# Start the gateway in background (daemon mode)
openclaw gateway start >/tmp/openclaw-gateway.log 2>&1 || true

# Keep container alive — sshd runs in background, gateway runs as daemon
# The harness will stop this container when tests complete
exec sleep infinity
