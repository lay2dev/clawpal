#!/usr/bin/env bash

set -euo pipefail

AGENT_ID="${AGENT_ID:-clawpal-remote-doctor}"
AGENT_NAME="${AGENT_NAME:-ClawPal Remote Doctor}"
OPENCLAW_HOME="${OPENCLAW_HOME:-$HOME/.openclaw}"
CONFIG_PATH="${CONFIG_PATH:-$OPENCLAW_HOME/openclaw.json}"
WORKSPACE_CONFIG_PATH="${WORKSPACE_CONFIG_PATH:-~/.openclaw/workspaces/$AGENT_ID}"
WORKSPACE_DIR="${WORKSPACE_DIR:-$OPENCLAW_HOME/workspaces/$AGENT_ID}"

require_command() {
  local missing=0
  local cmd
  for cmd in "$@"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      printf 'Missing required command: %s\n' "$cmd" >&2
      missing=1
    fi
  done

  if [ "$missing" -ne 0 ]; then
    exit 127
  fi
}

write_file() {
  local path="$1"
  local content="$2"
  mkdir -p "$(dirname "$path")"
  printf '%s' "$content" >"$path"
}

require_command python3

mkdir -p "$(dirname "$CONFIG_PATH")"
if [ ! -f "$CONFIG_PATH" ]; then
  printf '{}\n' >"$CONFIG_PATH"
fi

BACKUP_PATH="${CONFIG_PATH}.bak-$(date +%Y%m%d-%H%M%S)-$$"
cp "$CONFIG_PATH" "$BACKUP_PATH"

PYTHON_SUMMARY="$(
  python3 - "$CONFIG_PATH" "$AGENT_ID" "$WORKSPACE_CONFIG_PATH" <<'PY'
import json
import pathlib
import sys


def strip_comments(text: str) -> str:
    result = []
    in_string = False
    string_quote = ""
    escaped = False
    i = 0
    while i < len(text):
        ch = text[i]
        if in_string:
            result.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == string_quote:
                in_string = False
            i += 1
            continue
        if ch in ('"', "'"):
            in_string = True
            string_quote = ch
            result.append(ch)
            i += 1
            continue
        if ch == "/" and i + 1 < len(text):
            nxt = text[i + 1]
            if nxt == "/":
                i += 2
                while i < len(text) and text[i] not in "\r\n":
                    i += 1
                continue
            if nxt == "*":
                i += 2
                while i + 1 < len(text) and not (text[i] == "*" and text[i + 1] == "/"):
                    i += 1
                i = min(i + 2, len(text))
                continue
        result.append(ch)
        i += 1
    return "".join(result)


def strip_trailing_commas(text: str) -> str:
    result = []
    in_string = False
    string_quote = ""
    escaped = False
    i = 0
    while i < len(text):
        ch = text[i]
        if in_string:
            result.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == string_quote:
                in_string = False
            i += 1
            continue
        if ch in ('"', "'"):
            in_string = True
            string_quote = ch
            result.append(ch)
            i += 1
            continue
        if ch == ",":
            j = i + 1
            while j < len(text) and text[j] in " \t\r\n":
                j += 1
            if j < len(text) and text[j] in "}]":
                i += 1
                continue
        result.append(ch)
        i += 1
    return "".join(result)


def load_config(raw: str):
    text = raw.strip()
    if not text:
        return {}
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        normalized = strip_trailing_commas(strip_comments(text))
        return json.loads(normalized)


config_path = pathlib.Path(sys.argv[1])
agent_id = sys.argv[2]
workspace = sys.argv[3]

raw = config_path.read_text(encoding="utf-8")
config = load_config(raw)
if not isinstance(config, dict):
    raise SystemExit("openclaw.json must contain a top-level object")

agents = config.get("agents")
if agents is None:
    agents = {}
    config["agents"] = agents
if not isinstance(agents, dict):
    raise SystemExit("config field 'agents' must be an object")

agents_list = agents.get("list")
if agents_list is None:
    agents_list = []
    agents["list"] = agents_list
if not isinstance(agents_list, list):
    raise SystemExit("config field 'agents.list' must be an array")

existing = None
for item in agents_list:
    if isinstance(item, dict) and str(item.get("id", "")).strip() == agent_id:
        existing = item
        break

agent_existed = existing is not None
if existing is None:
    existing = {"id": agent_id}
    agents_list.append(existing)

existing["id"] = agent_id
existing["workspace"] = workspace

config_path.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
print(json.dumps({
    "agentExisted": agent_existed,
    "agentCount": len(agents_list),
}))
PY
)"

mkdir -p "$WORKSPACE_DIR"

write_file "$WORKSPACE_DIR/IDENTITY.md" "- Name: $AGENT_NAME
"
write_file "$WORKSPACE_DIR/AGENTS.md" "# Remote Doctor
Use this workspace only for ClawPal remote doctor planning sessions.
Return structured, operational answers.
"
write_file "$WORKSPACE_DIR/BOOTSTRAP.md" "Bootstrap is already complete for this workspace.
Do not ask who you are or who the user is.
Use IDENTITY.md and USER.md as the canonical identity context.
"
write_file "$WORKSPACE_DIR/USER.md" "- Name: ClawPal Desktop
- Role: desktop repair orchestrator
- Preferences: concise, operational, no bootstrap chatter
"
write_file "$WORKSPACE_DIR/HEARTBEAT.md" "Status: active remote-doctor planning workspace.
"

printf 'Remote Doctor bootstrap complete.\n'
printf 'config=%s\n' "$CONFIG_PATH"
printf 'backup=%s\n' "$BACKUP_PATH"
printf 'workspace=%s\n' "$WORKSPACE_DIR"
printf 'summary=%s\n' "$PYTHON_SUMMARY"
