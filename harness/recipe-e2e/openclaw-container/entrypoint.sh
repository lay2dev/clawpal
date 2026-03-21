#!/bin/bash
set -euo pipefail

export PATH="/root/.local/bin:/usr/local/bin:${PATH}"

mkdir -p /var/run/sshd
/usr/sbin/sshd

# Run gateway in foreground (no systemd in containers)
# Use 'openclaw gateway run' or direct node invocation
cd /root/.openclaw
nohup openclaw gateway run >/tmp/openclaw-gateway.log 2>&1 &

# Keep container alive
exec sleep infinity
