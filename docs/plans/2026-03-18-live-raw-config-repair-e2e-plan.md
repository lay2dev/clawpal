# Live Raw Config Repair E2E Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a live remote Doctor e2e that starts a Docker OpenClaw target, corrupts `openclaw.json`, runs real `clawpal-server` remote repair, and verifies the target becomes healthy again.

**Architecture:** Reuse the existing live gateway and Docker SSH test fixtures in `src-tauri/src/remote_doctor.rs`. Add one new live e2e guarded by the existing URL/token env vars plus Docker availability. The test will create a remote host config, deliberately corrupt the target config over SSH, invoke `start_remote_doctor_repair_impl(...)`, then verify the config is valid JSON again and rescue diagnosis is healthy.

**Tech Stack:** Rust, Tokio tests, Docker, SSH test fixture, real `clawpal-server` websocket gateway.

---

### Task 1: Add failing live e2e

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`
- Test: `src-tauri/src/remote_doctor.rs`

**Step 1: Write the failing test**

Add a live e2e that:
- starts the Docker SSH target
- corrupts `/root/.openclaw/openclaw.json`
- calls `start_remote_doctor_repair_impl(...)`
- expects a successful repair

**Step 2: Run test to verify it fails**

Run: `cargo test -p clawpal --lib remote_doctor_live_gateway_repairs_unreadable_remote_config -- --nocapture`

Expected: FAIL until the fixture/helpers are sufficient and the live path is wired for this scenario.

**Step 3: Write minimal implementation**

Add only the fixture/helper code needed for the new e2e.

**Step 4: Run test to verify it passes**

Run: `cargo test -p clawpal --lib remote_doctor_live_gateway_repairs_unreadable_remote_config -- --nocapture`

Expected: PASS in an environment where the real `clawpal-server` supports raw-config repair.

### Task 2: Preserve existing remote doctor tests

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Run the broader test group**

Run: `cargo test -p clawpal --lib remote_doctor -- --nocapture`

Expected: PASS
