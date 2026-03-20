# Remote Doctor Service Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a new remote doctor repair flow that requests detect/repair plans from a gateway websocket agent, executes returned commands locally against the selected OpenClaw target, reports results back, and loops until health is clean or 50 rounds are exhausted.

**Architecture:** Keep existing local repair behavior intact and add a second repair mode. The frontend exposes two repair actions, while Tauri owns a dedicated remote doctor websocket client, session orchestrator, command executor, and progress logging. The orchestrator alternates between detect and repair plans and treats the latest detection result as the only success source.

**Tech Stack:** React 18, TypeScript, Vitest, Tauri 2, Rust, Tokio, openclaw-gateway-client, Cargo test

---

### Task 1: Add frontend remote doctor types

**Files:**
- Modify: `src/lib/types.ts`
- Test: `src/lib/__tests__/doctor-page-features.test.ts`

**Step 1: Write the failing test**

Add a TypeScript-facing test case that constructs a remote doctor repair session object with:

- `mode: "remoteDoctor"`
- `status`
- `round`
- `phase`
- `lastPlanKind`
- `lastCommand`

and uses it in existing doctor page feature helpers without type errors.

**Step 2: Run test to verify it fails**

Run: `npm test -- doctor-page-features`
Expected: FAIL because the remote doctor types do not exist.

**Step 3: Write minimal implementation**

Add exported types for:

- `DoctorRepairMode`
- `RemoteDoctorPlanKind`
- `RemoteDoctorSessionStatus`
- `RemoteDoctorCommandPlan`
- `RemoteDoctorCommandResult`
- `RemoteDoctorRepairResult`
- `RemoteDoctorProgressEvent`

**Step 4: Run test to verify it passes**

Run: `npm test -- doctor-page-features`
Expected: PASS

**Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/__tests__/doctor-page-features.test.ts
git commit -m "feat: add remote doctor frontend types"
```

### Task 2: Add frontend API bindings

**Files:**
- Modify: `src/lib/api.ts`
- Modify: `src/lib/use-api.ts`
- Test: `src/lib/__tests__/use-api-extra.test.ts`

**Step 1: Write the failing test**

Add a test that asserts the API exposes a `startRemoteDoctorRepair` method and that `useApi()` returns it for both local and remote instances.

**Step 2: Run test to verify it fails**

Run: `npm test -- use-api-extra`
Expected: FAIL because the API method does not exist.

**Step 3: Write minimal implementation**

Add a Tauri wrapper for:

- `start_remote_doctor_repair`

The method must accept the current target context:

- `instanceId`
- `targetLocation`
- `hostId` when available

**Step 4: Run test to verify it passes**

Run: `npm test -- use-api-extra`
Expected: PASS

**Step 5: Commit**

```bash
git add src/lib/api.ts src/lib/use-api.ts src/lib/__tests__/use-api-extra.test.ts
git commit -m "feat: add remote doctor repair api binding"
```

### Task 3: Expose two repair actions in Doctor UI

**Files:**
- Modify: `src/pages/Doctor.tsx`
- Modify: `src/components/DoctorRecoveryOverview.tsx`
- Test: `src/pages/__tests__/Doctor.test.tsx`

**Step 1: Write the failing test**

Add a UI test that verifies:

- the Doctor page shows both `本地修复` and `远程 Doctor 修复`
- clicking the local button still calls the existing repair method
- clicking the remote doctor button calls the new API method

**Step 2: Run test to verify it fails**

Run: `npm test -- Doctor.test`
Expected: FAIL because only one repair action exists.

**Step 3: Write minimal implementation**

Update the Doctor page to:

- keep the existing diagnose button behavior
- show two repair actions when diagnosis indicates problems
- track which repair mode is running
- listen for remote doctor progress events

**Step 4: Run test to verify it passes**

Run: `npm test -- Doctor.test`
Expected: PASS

**Step 5: Commit**

```bash
git add src/pages/Doctor.tsx src/components/DoctorRecoveryOverview.tsx src/pages/__tests__/Doctor.test.tsx
git commit -m "feat: add dual repair actions to doctor page"
```

### Task 4: Define Rust remote doctor contract

**Files:**
- Create: `src-tauri/src/remote_doctor/types.rs`
- Create: `src-tauri/src/remote_doctor/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/remote_doctor/types.rs`

**Step 1: Write the failing test**

Add unit tests for:

- `RemoteDoctorPlanKind` deserialization from `detect` and `repair`
- default `targetLocation`
- command result serialization

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::types`
Expected: FAIL because the module does not exist.

**Step 3: Write minimal implementation**

Create serde types for:

- repair request payload
- plan response payload
- command item
- command result
- session result
- progress payload

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::types`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/remote_doctor/mod.rs src-tauri/src/remote_doctor/types.rs
git commit -m "feat: define remote doctor rust types"
```

### Task 5: Implement gateway doctor client

**Files:**
- Create: `src-tauri/src/remote_doctor/client.rs`
- Test: `src-tauri/src/remote_doctor/client.rs`

**Step 1: Write the failing test**

Add tests with fake request/response transport verifying:

- detect request payload includes target location
- repair request payload includes previous command results
- planner errors are surfaced as Rust errors

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::client`
Expected: FAIL because the client does not exist.

**Step 3: Write minimal implementation**

Wrap `openclaw-gateway-client` and expose methods:

- `request_detect_plan`
- `request_repair_plan`
- `report_plan_results`

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::client`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/client.rs
git commit -m "feat: add remote doctor gateway client"
```

### Task 6: Implement command executor and log writer

**Files:**
- Create: `src-tauri/src/remote_doctor/executor.rs`
- Create: `src-tauri/src/remote_doctor/log.rs`
- Test: `src-tauri/src/remote_doctor/executor.rs`

**Step 1: Write the failing test**

Add tests verifying:

- a command plan is executed in order
- stdout/stderr/exit code are captured
- command timeout is reported
- local and remote target contexts select the proper runner

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::executor`
Expected: FAIL because the executor does not exist.

**Step 3: Write minimal implementation**

Implement:

- local target execution through existing local command helpers
- remote target execution through existing SSH helpers
- JSONL or line-oriented session logging

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::executor`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/executor.rs src-tauri/src/remote_doctor/log.rs
git commit -m "feat: add remote doctor executor and logs"
```

### Task 7: Implement orchestrator loop and Tauri command

**Files:**
- Create: `src-tauri/src/remote_doctor/orchestrator.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/remote_doctor/orchestrator.rs`

**Step 1: Write the failing test**

Add orchestration tests covering:

- detect clean on first round => success
- detect then repair then detect clean => success
- more than 50 rounds => exhausted error
- planner failure => error

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::orchestrator`
Expected: FAIL because the orchestrator does not exist.

**Step 3: Write minimal implementation**

Implement:

- alternating detect/repair plan loop
- max round guard at 50
- progress event emission
- final result payload
- `start_remote_doctor_repair` Tauri command

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::orchestrator`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/orchestrator.rs src-tauri/src/commands/mod.rs src-tauri/src/main.rs
git commit -m "feat: add remote doctor repair orchestration"
```

### Task 8: Run verification and update docs

**Files:**
- Modify: `docs/mvp-checklist.md`
- Modify: `docs/plans/2026-03-17-remote-doctor-service-design.md`

**Step 1: Run targeted frontend tests**

Run: `npm test -- doctor-page-features`
Expected: PASS

**Step 2: Run Doctor page tests**

Run: `npm test -- Doctor.test`
Expected: PASS

**Step 3: Run targeted Rust tests**

Run: `cargo test remote_doctor`
Expected: PASS

**Step 4: Run typecheck**

Run: `npm run build` or `npx tsc --noEmit`
Expected: PASS

**Step 5: Commit**

```bash
git add docs/mvp-checklist.md docs/plans/2026-03-17-remote-doctor-service-design.md
git commit -m "docs: document remote doctor repair flow"
```
