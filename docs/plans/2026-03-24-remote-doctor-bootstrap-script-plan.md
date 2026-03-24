# Remote Doctor Bootstrap Script Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a one-off remote-host bootstrap script that safely prepares the dedicated `clawpal-remote-doctor` agent entry, workspace, and bootstrap files required by the Remote Doctor planner flow.

**Architecture:** Keep the implementation fully self-contained in a repository script. Use `bash` for orchestration and file setup, and use a small embedded `python3` block for config parsing and mutation so the script stays idempotent and avoids brittle shell JSON edits. Verify behavior through an integration test that runs the real script in a temporary `HOME`.

**Tech Stack:** Bash, Python 3, Rust integration tests, Cargo

---

### Task 1: Write the approved design artifact

**Files:**
- Create: `docs/plans/2026-03-24-remote-doctor-bootstrap-script-design.md`
- Create: `docs/plans/2026-03-24-remote-doctor-bootstrap-script-plan.md`

**Step 1: Save the design**

Write the approved script design into `docs/plans/2026-03-24-remote-doctor-bootstrap-script-design.md`.

**Step 2: Save the plan**

Write this implementation plan into `docs/plans/2026-03-24-remote-doctor-bootstrap-script-plan.md`.

### Task 2: Add a failing integration test for the bootstrap script

**Files:**
- Create: `src-tauri/tests/remote_doctor_bootstrap_script.rs`
- Test: `src-tauri/tests/remote_doctor_bootstrap_script.rs`

**Step 1: Write the failing test**

Add an integration test that:
- creates a temporary `HOME`
- seeds `~/.openclaw/openclaw.json` with a minimal config containing only the main agent
- runs `bash ../scripts/remote-doctor-bootstrap.sh`
- asserts:
  - exit code is `0`
  - a config backup file exists
  - `clawpal-remote-doctor` is present in `agents.list`
  - the agent workspace equals `~/.openclaw/workspaces/clawpal-remote-doctor`
  - `IDENTITY.md`, `AGENTS.md`, `BOOTSTRAP.md`, `USER.md`, and `HEARTBEAT.md` exist

Add a second test that seeds a config with comments and trailing commas so the JSON normalization path is covered.

**Step 2: Run test to verify it fails**

Run: `cargo test -p clawpal --test remote_doctor_bootstrap_script -- --nocapture`
Expected: FAIL because the script does not exist yet.

**Step 3: Commit**

```bash
git add docs/plans/2026-03-24-remote-doctor-bootstrap-script-design.md docs/plans/2026-03-24-remote-doctor-bootstrap-script-plan.md src-tauri/tests/remote_doctor_bootstrap_script.rs
git commit -m "test: add remote doctor bootstrap script coverage"
```

### Task 3: Implement the standalone bootstrap script

**Files:**
- Create: `scripts/remote-doctor-bootstrap.sh`
- Modify: `scripts/README.md`

**Step 1: Write minimal implementation**

Create `scripts/remote-doctor-bootstrap.sh` that:
- uses `#!/usr/bin/env bash`
- enables `set -euo pipefail`
- validates `python3`
- resolves:
  - `OPENCLAW_HOME` defaulting to `$HOME/.openclaw`
  - `CONFIG_PATH="$OPENCLAW_HOME/openclaw.json"`
  - `AGENT_ID="clawpal-remote-doctor"`
  - `WORKSPACE_PATH="$OPENCLAW_HOME/workspaces/$AGENT_ID"`
- creates the config dir and config file if missing
- creates a timestamped backup before mutation
- uses embedded `python3` to parse/update/write the config
- writes the five bootstrap files with the expected Remote Doctor content
- prints a readable summary of the applied paths

Document the new script briefly in `scripts/README.md`.

**Step 2: Run test to verify it passes**

Run: `cargo test -p clawpal --test remote_doctor_bootstrap_script -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add scripts/remote-doctor-bootstrap.sh scripts/README.md src-tauri/tests/remote_doctor_bootstrap_script.rs
git commit -m "feat: add remote doctor bootstrap script"
```

### Task 4: Final focused verification

**Files:**
- Modify: `docs/plans/2026-03-24-remote-doctor-bootstrap-script-design.md`
- Modify: `docs/plans/2026-03-24-remote-doctor-bootstrap-script-plan.md`
- Create: `scripts/remote-doctor-bootstrap.sh`
- Create: `src-tauri/tests/remote_doctor_bootstrap_script.rs`
- Modify: `scripts/README.md`

**Step 1: Run focused Rust verification**

Run: `cargo test -p clawpal --test remote_doctor_bootstrap_script -- --nocapture`
Expected: PASS

**Step 2: Run focused script lint check**

Run: `bash -n scripts/remote-doctor-bootstrap.sh`
Expected: PASS

**Step 3: Review diff**

Run: `git diff -- docs/plans/2026-03-24-remote-doctor-bootstrap-script-design.md docs/plans/2026-03-24-remote-doctor-bootstrap-script-plan.md scripts/README.md scripts/remote-doctor-bootstrap.sh src-tauri/tests/remote_doctor_bootstrap_script.rs`
Expected: Only the new bootstrap docs, script, and focused test changes should appear.
