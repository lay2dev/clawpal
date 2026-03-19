#!/bin/bash
# Start OpenClaw gateway in the background
openclaw gateway start &

# Wait for gateway to be ready (up to 30s)
for i in $(seq 1 30); do
  if curl -sf http://127.0.0.1:18789/ > /dev/null 2>&1; then
    echo "OpenClaw gateway ready after ${i}s"
    break
  fi
  sleep 1
done

# Start SSH daemon in foreground
exec /usr/sbin/sshd -D
