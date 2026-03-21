#!/bin/bash
# Fast wrapper for openclaw that short-circuits slow commands

case "$*" in
  *"agents list"*"--json"*|*"agents"*"list"*"--json"*)
    cat <<'AGENTS_JSON'
[{"id":"main","model":"anthropic/claude-sonnet-4-20250514","workspace":"/root/.openclaw/agents/main/agent","identity":{"name":"Main Agent","emoji":"🤖"}}]
AGENTS_JSON
    exit 0
    ;;
  *"agents list"*|*"agents"*"list"*)
    echo "main"
    exit 0
    ;;
  *"config get"*)
    cat /root/.openclaw/openclaw.json
    exit 0
    ;;
  *"gateway restart"*|*"gateway stop"*)
    # Short-circuit gateway restart/stop — no real gateway restart needed in E2E
    echo "Gateway restart skipped (E2E mode)"
    exit 0
    ;;
  *"gateway status"*)
    echo "Gateway is running"
    exit 0
    ;;
  *)
    exec /usr/bin/openclaw-real "$@"
    ;;
esac
