#!/bin/bash
# Start OpenClaw gateway in the background
nohup openclaw gateway start > /tmp/oc-gw.log 2>&1 &
echo "OpenClaw gateway starting (pid $!)"

# Forward 0.0.0.0:18789 → 127.0.0.1:18789 for Docker port mapping
nohup socat TCP-LISTEN:18790,fork,reuseaddr,bind=0.0.0.0 TCP:127.0.0.1:18789 > /tmp/socat.log 2>&1 &
echo "socat proxy on 18790 → 18789"

# Start SSH daemon in foreground immediately
exec /usr/sbin/sshd -D
