# Remote Doctor Connect Diagnostics Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Expose actionable Remote Doctor websocket handshake failure details in backend errors, session logs, and Doctor UI messaging.

**Architecture:** Keep the existing Remote Doctor connection flow, but add one small state slot in `NodeClient` for the latest disconnect reason. Reuse that state to improve returned errors, log connect failures into the existing session JSONL stream, and format a more helpful Doctor page error string without changing the underlying protocol.

**Tech Stack:** Rust, Tauri v2, React, TypeScript, Bun

---

### Task 1: Lock backend disconnect wording with failing tests

**Files:**
- Modify: `src-tauri/src/node_client.rs`
- Test: `src-tauri/src/node_client.rs`

**Step 1: Write the failing test**

Add focused unit tests for:
- formatting a websocket close frame into a readable reason string
- building the `Connection lost while waiting for response: ...` message when a disconnect reason exists

**Step 2: Run test to verify it fails**

Run: `cargo test node_client::tests -- --nocapture`
Expected: FAIL because the helper functions and richer message formatting do not exist yet.

**Step 3: Write minimal implementation**

Add small helper functions and `last_disconnect_reason` storage to `NodeClient`, and use them when the reader task receives a close/error and when `send_request()` loses its response channel.

**Step 4: Run test to verify it passes**

Run: `cargo test node_client::tests -- --nocapture`
Expected: PASS

### Task 2: Lock Remote Doctor session logging with a failing test

**Files:**
- Modify: `src-tauri/src/remote_doctor/session.rs`
- Modify: `src-tauri/src/remote_doctor/repair_loops.rs`
- Test: `src-tauri/src/remote_doctor/session.rs`

**Step 1: Write the failing test**

Add a unit test that writes a gateway connect failure event and asserts the JSONL line contains:
- `event = "gateway_connect_failed"`
- the gateway URL
- whether a gateway auth token override was present
- the specific error string

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::session::tests -- --nocapture`
Expected: FAIL because the helper and event do not exist yet.

**Step 3: Write minimal implementation**

Add a small session logging helper and call it from `start_remote_doctor_repair_impl(...)` when `client.connect(...)` returns an error.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::session::tests -- --nocapture`
Expected: PASS

### Task 3: Lock Doctor UI wording with a failing test

**Files:**
- Create: `src/lib/remote-doctor-error.ts`
- Create: `src/lib/__tests__/remote-doctor-error.test.ts`
- Modify: `src/pages/Doctor.tsx`

**Step 1: Write the failing test**

Add a frontend unit test that verifies `Connection lost while waiting for response: ...` becomes a more actionable Doctor error message that mentions the websocket was accepted and that the invite-code-derived token or saved Remote Doctor token should be checked.

**Step 2: Run test to verify it fails**

Run: `bun test src/lib/__tests__/remote-doctor-error.test.ts`
Expected: FAIL because the formatter helper does not exist yet.

**Step 3: Write minimal implementation**

Implement the formatter helper and use it in the Remote Doctor repair catch block in `Doctor.tsx`.

**Step 4: Run test to verify it passes**

Run: `bun test src/lib/__tests__/remote-doctor-error.test.ts`
Expected: PASS

### Task 4: Final focused verification

**Files:**
- Modify: `docs/plans/2026-03-24-remote-doctor-connect-diagnostics-design.md`
- Modify: `docs/plans/2026-03-24-remote-doctor-connect-diagnostics-plan.md`

**Step 1: Run Rust verification**

Run: `cargo test node_client::tests remote_doctor::session::tests -- --nocapture`
Expected: PASS

**Step 2: Run frontend verification**

Run: `bun test src/lib/__tests__/remote-doctor-error.test.ts`
Expected: PASS

**Step 3: Review diff**

Run: `git diff -- docs/plans/2026-03-24-remote-doctor-connect-diagnostics-design.md docs/plans/2026-03-24-remote-doctor-connect-diagnostics-plan.md src-tauri/src/node_client.rs src-tauri/src/remote_doctor/session.rs src-tauri/src/remote_doctor/repair_loops.rs src/lib/remote-doctor-error.ts src/lib/__tests__/remote-doctor-error.test.ts src/pages/Doctor.tsx`
Expected: Only Remote Doctor diagnostics and UI wording should change.
