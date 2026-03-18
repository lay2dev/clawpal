# Raw Config Recovery Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow remote Doctor to continue when the target `openclaw.json` is not parseable JSON by switching to a raw-config recovery path instead of failing before a repair plan can be requested.

**Architecture:** Extend `src-tauri/src/remote_doctor.rs` so `clawpal_server` repair requests can carry a fallback raw config excerpt and structured unreadable-config metadata when `read_target_config(...)` fails with `primary.config.unreadable`. Keep normal JSON-based repair behavior unchanged for valid configs. Add logging that makes the fallback visible in the session log.

**Tech Stack:** Rust, Tauri, existing config read commands, cargo test.

---

### Task 1: Add failing tests for unreadable config fallback

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`
- Test: `src-tauri/src/remote_doctor.rs`

**Step 1: Write the failing test**

Add tests that expect:
- config parse failures to be converted into a raw config context payload
- final errors to avoid the old immediate `Failed to parse target config` failure in the clawpal-server planning path

**Step 2: Run test to verify it fails**

Run: `cargo test -p clawpal --lib unreadable_config_ -- --nocapture`

Expected: FAIL because the fallback context helpers do not exist yet.

**Step 3: Write minimal implementation**

Add the smallest helpers needed to construct raw config fallback context.

**Step 4: Run test to verify it passes**

Run: `cargo test -p clawpal --lib unreadable_config_ -- --nocapture`

Expected: PASS

### Task 2: Implement raw config recovery in clawpal_server path

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Add raw config read helper**

Read the target config as raw text and try to parse JSON. If parsing fails:
- keep the raw text
- record parse error
- build a fallback request context instead of returning early

**Step 2: Adjust clawpal_server plan request payload**

When JSON config is unavailable:
- send `configExcerpt: null`
- send `configExcerptRaw`
- send `configParseError`
- log a `config_recovery_context` event

**Step 3: Preserve normal valid-config behavior**

Do not change the request payload for valid configs.

### Task 3: Verify regression behavior

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Run focused tests**

Run: `cargo test -p clawpal --lib unreadable_config_ -- --nocapture`

Expected: PASS

**Step 2: Run broader remote doctor tests**

Run: `cargo test -p clawpal --lib remote_doctor -- --nocapture`

Expected: PASS
