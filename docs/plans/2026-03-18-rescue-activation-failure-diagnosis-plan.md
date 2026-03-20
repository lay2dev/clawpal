# Rescue Activation Failure Diagnosis Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a rescue activation failure diagnosis flow so remote Doctor logs actionable rescue startup checks instead of only returning a terminal configured-inactive error.

**Architecture:** Extend the existing rescue preflight path in `src-tauri/src/remote_doctor.rs`. When `manage_rescue_bot activate rescue` does not produce an active rescue gateway, collect a small local/remote diagnostic bundle using existing rescue status commands and targeted shell checks, append that bundle to the session log, and include a concise summary in the final error.

**Tech Stack:** Rust, Tauri commands, existing rescue/SSH helpers, cargo test.

---

### Task 1: Add failing tests for rescue activation diagnosis

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`
- Test: `src-tauri/src/remote_doctor.rs`

**Step 1: Write the failing test**

Add unit tests that expect:
- rescue activation failure errors to include a diagnosis summary
- rescue activation failure diagnostics to capture rescue status / gateway checks in log-friendly command results

**Step 2: Run test to verify it fails**

Run: `cargo test -p clawpal --lib rescue_activation_error_mentions_runtime_state -- --nocapture`

Expected: FAIL because the new diagnosis summary is not implemented yet.

**Step 3: Write minimal implementation**

Add the smallest helper/data needed to produce a structured rescue failure diagnosis bundle.

**Step 4: Run test to verify it passes**

Run: `cargo test -p clawpal --lib rescue_activation_error_mentions_runtime_state -- --nocapture`

Expected: PASS

### Task 2: Implement rescue activation failure diagnosis flow

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Extend rescue preflight failure handling**

When rescue activation remains inactive:
- gather rescue status details
- run a small set of check commands
- append `rescue_activation_diagnosis` to the remote doctor session log
- return an error that references the diagnosis summary

**Step 2: Keep behavior minimal**

Do not change the remote doctor protocol. Do not add new fallback protocols. Only improve local rescue failure diagnosis.

**Step 3: Verify targeted tests**

Run: `cargo test -p clawpal --lib remote_doctor -- --nocapture`

Expected: PASS

### Task 3: Verify regression behavior

**Files:**
- Modify: `src-tauri/src/remote_doctor.rs`

**Step 1: Run focused e2e regression**

Run: `cargo test -p clawpal --lib remote_doctor_docker_e2e_rescue_activation_fails_when_gateway_stays_inactive -- --nocapture`

Expected: PASS and still fail early on inactive rescue gateway.

**Step 2: Run broader remote doctor tests**

Run: `cargo test -p clawpal --lib remote_doctor`

Expected: PASS
