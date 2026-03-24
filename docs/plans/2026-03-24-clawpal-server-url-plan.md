# ClawPal Server URL Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Change ClawPal's default clawpal-server endpoints from `127.0.0.1:3000` to `65.21.45.43:3040` while keeping frontend copy, fallback behavior, and tests consistent.

**Architecture:** Keep the current fixed-default design. Update the Rust websocket and HTTP constants, update the frontend HTTP fallback constant, and align UI copy plus focused tests with the new address. Do not change user-saved preference behavior or add new configuration layers.

**Tech Stack:** Rust, Tauri v2, React, TypeScript, Bun

---

### Task 1: Update frontend fallback expectations first

**Files:**
- Modify: `src/lib/__tests__/invite-code.test.ts`
- Test: `src/lib/__tests__/invite-code.test.ts`

**Step 1: Write the failing test**

Update the test expectations so blank gateway URLs and localhost websocket defaults now resolve to `http://65.21.45.43:3040`, and invite exchange posts to `http://65.21.45.43:3040/api-keys/exchange`.

**Step 2: Run test to verify it fails**

Run: `bun test src/lib/__tests__/invite-code.test.ts`
Expected: FAIL because the implementation still returns `http://127.0.0.1:3000`.

**Step 3: Write minimal implementation**

Update `src/lib/invite-code.ts` to use `http://65.21.45.43:3040` as the default base URL.

**Step 4: Run test to verify it passes**

Run: `bun test src/lib/__tests__/invite-code.test.ts`
Expected: PASS

### Task 2: Update Rust fixed gateway default

**Files:**
- Modify: `src-tauri/src/remote_doctor/config.rs`
- Test: `src-tauri/src/remote_doctor/config.rs`

**Step 1: Write the failing test**

Update the existing fixed-gateway test so it expects `ws://65.21.45.43:3040/ws`.

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::config::tests::load_gateway_config_uses_fixed_clawpal_server_url -- --nocapture`
Expected: FAIL because the implementation still returns `ws://127.0.0.1:3000/ws`.

**Step 3: Write minimal implementation**

Update the fixed websocket constant in `src-tauri/src/remote_doctor/config.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::config::tests::load_gateway_config_uses_fixed_clawpal_server_url -- --nocapture`
Expected: PASS

### Task 3: Update Rust invite exchange endpoint and frontend copy

**Files:**
- Modify: `src-tauri/src/commands/preferences.rs`
- Modify: `src/pages/Settings.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh.json`

**Step 1: Write the change**

Update the fixed invite exchange URL to `http://65.21.45.43:3040/api-keys/exchange`. Update settings placeholder/hint text and the invite-exchange error log payload to reference `ws://65.21.45.43:3040/ws`.

**Step 2: Run focused verification**

Run: `bun test src/lib/__tests__/invite-code.test.ts`
Expected: PASS

Run: `cargo test remote_doctor::config::tests::load_gateway_config_uses_fixed_clawpal_server_url -- --nocapture`
Expected: PASS

### Task 4: Run final verification

**Files:**
- Modify: `docs/plans/2026-03-24-clawpal-server-url-design.md`
- Modify: `docs/plans/2026-03-24-clawpal-server-url-plan.md`

**Step 1: Run frontend verification**

Run: `bun test src/lib/__tests__/invite-code.test.ts`
Expected: PASS

**Step 2: Run Rust verification**

Run: `cargo test remote_doctor::config::tests::load_gateway_config_uses_fixed_clawpal_server_url -- --nocapture`
Expected: PASS

**Step 3: Review diff**

Run: `git diff -- docs/plans/2026-03-24-clawpal-server-url-design.md docs/plans/2026-03-24-clawpal-server-url-plan.md src/lib/invite-code.ts src/lib/__tests__/invite-code.test.ts src-tauri/src/remote_doctor/config.rs src-tauri/src/commands/preferences.rs src/pages/Settings.tsx src/locales/en.json src/locales/zh.json`
Expected: Only the default ClawPal server URL references and aligned docs/copy should change.
