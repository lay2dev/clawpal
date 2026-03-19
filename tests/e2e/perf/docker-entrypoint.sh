#!/bin/bash
# Start OpenClaw gateway in the background (don't block sshd)
nohup openclaw gateway start > /tmp/oc-gw.log 2>&1 &
echo "OpenClaw gateway starting (pid $!)"

# Start SSH daemon in foreground immediately
exec /usr/sbin/sshd -D
