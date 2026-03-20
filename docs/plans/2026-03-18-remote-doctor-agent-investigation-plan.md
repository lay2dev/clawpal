# Remote Doctor Agent Investigation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the default `clawpal_server` remote repair planner with an agent-driven investigate/repair loop that can handle unreadable JSON configs without hardcoded repair logic.

**Architecture:** Update `src-tauri/src/remote_doctor.rs` to make the standard gateway `agent` path the default remote repair flow. Extend the plan state machine with a new `investigate` phase for `primary.config.unreadable`, generate phase-specific agent prompts, and keep command execution/logging in ClawPal. Preserve the existing `clawpal_server` path only as non-default fallback during migration.

**Tech Stack:** Rust, Tauri, websocket gateway client, cargo test, live Docker e2e.

---

### Task 1: Add failing tests for investigate phase selection

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`
- Test: `src-tauri/src/remote_doctor.rs`

**Step 1: Write the failing test**

Add unit tests that expect:
- `primary.config.unreadable` selects `investigate` before `repair`
- investigate prompts contain read-only constraints
- repair prompts reference prior investigation results

**Step 2: Run test to verify it fails**

Run: `cargo test -p clawpal --lib investigate_ -- --nocapture`

Expected: FAIL because `PlanKind::Investigate` and the new prompt rules do not exist yet.

**Step 3: Write minimal implementation**

Add the smallest new enum variant and prompt branching needed to satisfy the tests.

**Step 4: Run test to verify it passes**

Run: `cargo test -p clawpal --lib investigate_ -- --nocapture`

Expected: PASS

### Task 2: Make agent planner the default remote repair path

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Change protocol selection**

Make `AgentPlanner` the default remote repair protocol. Keep `ClawpalServer` only as explicit fallback / compatibility path.

**Step 2: Extend the state machine**

Add `PlanKind::Investigate` and route unreadable-config diagnoses into investigate first.

**Step 3: Preserve execution plumbing**

Do not rewrite command execution. Reuse existing `PlanResponse` / `PlanCommand` execution and result logging.

### Task 3: Update agent prompt construction

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Add phase-specific prompt rules**

Implement:
- diagnose prompt
- investigate prompt
- repair prompt

**Step 2: Include raw config context**

Always include:
- `configExcerpt`
- `configExcerptRaw`
- `configParseError`

### Task 4: Update logging and stall detection

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Log investigate plans**

Ensure `plan_received` and `command_result` support `planKind: investigate`.

**Step 2: Extend stall detection**

Treat repeated empty or non-actionable investigate plans as stalled.

### Task 5: Add/adjust live e2e

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Reuse the unreadable config live e2e**

Update the existing live Docker test so it runs through the agent path instead of the `clawpal_server` planner.

**Step 2: Verify behavior**

Run: `cargo test -p clawpal --lib remote_doctor_live_gateway_repairs_unreadable_remote_config -- --nocapture`

Expected: PASS when the real gateway agent returns actionable diagnostic and repair steps.

### Task 6: Run regression tests

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Run focused tests**

Run: `cargo test -p clawpal --lib investigate_ -- --nocapture`

Expected: PASS

**Step 2: Run broader remote doctor tests**

Run: `cargo test -p clawpal --lib remote_doctor -- --nocapture`

Expected: PASS
