# Remote Doctor Bootstrap Script Design

**Date:** 2026-03-24

**Goal:** Provide a one-off remote-host script that prepares the dedicated `clawpal-remote-doctor` agent, workspace, and bootstrap files needed by the Remote Doctor planner flow.

## Scope

- Run directly on the remote host with `bash`.
- Require `python3` for config mutation.
- Back up the existing `~/.openclaw/openclaw.json` before changing it.
- Ensure `agents.list` contains a `clawpal-remote-doctor` entry with an explicit workspace.
- Create the dedicated workspace and write the expected bootstrap files.
- Be safe to run more than once.

## Non-Goals

- No ClawPal/Tauri command dispatch.
- No SSH orchestration from the desktop app.
- No automatic execution from the Remote Doctor connection path.
- No full OpenClaw config repair beyond the dedicated agent/workspace setup.

## Approach Options

### Option 1: Pure bash/sed

Use shell string operations to patch `openclaw.json` in place.

**Pros:** Lowest dependency footprint.
**Cons:** Too easy to corrupt JSON, especially on repeat runs or partially configured hosts.

### Option 2: Bash plus `python3` JSON mutation

Use `bash` for file/dir orchestration and `python3` for config parsing, mutation, and pretty-printing.

**Pros:** Good balance of portability and safety. Keeps the script self-contained.
**Cons:** Depends on `python3`. Native `json` parsing needs a small normalization layer for common JSON5-style comments and trailing commas.

### Option 3: Depend on OpenClaw CLI write commands

Use remote `openclaw agents add` / `openclaw config set` commands instead of editing the config file directly.

**Pros:** Avoids manual file mutation if the CLI behavior is perfectly known.
**Cons:** Current repository evidence is not strong enough to rely on exact CLI write semantics for this one-off recovery script.

## Recommended Design

Use **Option 2**.

Add a standalone script at `scripts/remote-doctor-bootstrap.sh`. The script should default to:

- config path: `~/.openclaw/openclaw.json`
- agent id: `clawpal-remote-doctor`
- agent display name: `ClawPal Remote Doctor`
- workspace: `~/.openclaw/workspaces/clawpal-remote-doctor`

The script should:

1. Validate that `bash` and `python3` are available.
2. Create `~/.openclaw` and the config file if they do not exist.
3. Create a timestamped backup of the current config before mutation.
4. Use an embedded `python3` block to:
   - parse the config as JSON
   - retry with a lightweight normalization pass for common JSON5-style comments and trailing commas
   - ensure `/agents/list` exists as an array
   - add or update the dedicated `clawpal-remote-doctor` entry
   - preserve unrelated agent entries
   - write pretty JSON back to disk
5. Create the workspace directory.
6. Write:
   - `IDENTITY.md`
   - `AGENTS.md`
   - `BOOTSTRAP.md`
   - `USER.md`
   - `HEARTBEAT.md`
7. Print a short summary including the config path, backup path, workspace path, and whether the agent entry already existed.

The script should be idempotent:

- If the agent already exists, do not append a duplicate.
- If the workspace already exists, keep it and refresh the bootstrap files.
- If `agents` or `agents.list` are missing, create them.

## Testing

- Add an integration test that runs the script against a temporary `HOME`.
- Start from a minimal config that only contains the main agent.
- Verify the script:
  - exits successfully
  - creates a backup file
  - writes the dedicated `clawpal-remote-doctor` entry into `openclaw.json`
  - creates the workspace
  - writes all expected bootstrap files
- Add coverage for a config input that includes comments or trailing commas so the JSON normalization path is exercised.
