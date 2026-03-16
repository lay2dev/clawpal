# Remote Repair Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a controlled remote repair flow where ClawPal requests a structured repair plan from bot B, validates it locally, executes it over SSH on remote instance A, reports results back, and loops until Doctor reports healthy or stop conditions are reached.

**Architecture:** Add a dedicated `remote_repair` Rust module that owns repair DSL types, planner integration, policy validation, SSH-backed execution, and session orchestration. Expose it through Tauri commands consumed by the Doctor page, while reusing the existing remote Doctor, SSH pool, and progress-event patterns.

**Tech Stack:** Tauri 2, Rust, React 18, TypeScript, Vitest, Cargo test

---

### Task 1: Define remote repair TypeScript contract

**Files:**
- Modify: `src/lib/types.ts`
- Test: `src/lib/__tests__/doctor-page-features.test.ts`

**Step 1: Write the failing test**

Add an assertion in `src/lib/__tests__/doctor-page-features.test.ts` that a remote repair state object with `status`, `currentRound`, `plan`, and `stepResults` is accepted by the UI-facing helpers without TypeScript errors.

**Step 2: Run test to verify it fails**

Run: `npm test -- doctor-page-features`
Expected: FAIL with missing remote repair types.

**Step 3: Write minimal implementation**

Add exact exported types for:

- `RemoteRepairSessionStatus`
- `RemoteRepairPlan`
- `RemoteRepairStep`
- `RemoteRepairStepResult`
- `RemoteRepairSession`
- `RemoteRepairStartResult`
- `RemoteRepairProgressEvent`

Keep fields aligned with the approved design and current API naming style in `src/lib/types.ts`.

**Step 4: Run test to verify it passes**

Run: `npm test -- doctor-page-features`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/__tests__/doctor-page-features.test.ts
git commit -m "feat: add remote repair frontend type contracts"
```

### Task 2: Add frontend API bindings for remote repair

**Files:**
- Modify: `src/lib/api.ts`
- Modify: `src/lib/types.ts`
- Test: `src/lib/__tests__/use-api-extra.test.ts`

**Step 1: Write the failing test**

Add a test in `src/lib/__tests__/use-api-extra.test.ts` that asserts the API object exposes these functions:

- `startRemoteRepairSession`
- `getRemoteRepairSession`
- `cancelRemoteRepairSession`

For remote instances, the input must include `hostId`.

**Step 2: Run test to verify it fails**

Run: `npm test -- use-api-extra`
Expected: FAIL because the API methods do not exist.

**Step 3: Write minimal implementation**

In `src/lib/api.ts`, add wrappers for new Tauri commands:

- `start_remote_repair_session`
- `get_remote_repair_session`
- `cancel_remote_repair_session`

Return the exact TypeScript types from Task 1.

**Step 4: Run test to verify it passes**

Run: `npm test -- use-api-extra`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/lib/api.ts src/lib/types.ts src/lib/__tests__/use-api-extra.test.ts
git commit -m "feat: add remote repair API bindings"
```

### Task 3: Create Rust remote repair type module

**Files:**
- Create: `src-tauri/src/remote_repair/types.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/remote_repair/types.rs`

**Step 1: Write the failing test**

Add unit tests in `src-tauri/src/remote_repair/types.rs` verifying:

- `RemoteRepairStepType` deserializes supported step kinds
- `RemoteRepairStopPolicy` provides safe defaults
- invalid step types fail deserialization

**Step 2: Run test to verify it fails**

Run: `cargo test remote_repair::types`
Expected: FAIL because the module does not exist.

**Step 3: Write minimal implementation**

Create `src-tauri/src/remote_repair/types.rs` with serde-serializable definitions for:

- session status
- plan
- step
- stop policy
- step result
- session snapshot
- planner request/response payloads

Add `pub mod remote_repair;` in `src-tauri/src/lib.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_repair::types`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/remote_repair/types.rs
git commit -m "feat: define remote repair rust types"
```

### Task 4: Implement local policy validation

**Files:**
- Create: `src-tauri/src/remote_repair/policy.rs`
- Modify: `src-tauri/src/remote_repair/types.rs`
- Test: `src-tauri/src/remote_repair/policy.rs`

**Step 1: Write the failing test**

Add tests that verify:

- `write_file` outside allowed paths is blocked
- `run_command` without `allowlist_tag` is blocked
- unknown `allowlist_tag` is blocked
- a safe service restart plan is accepted

**Step 2: Run test to verify it fails**

Run: `cargo test remote_repair::policy`
Expected: FAIL because validation is not implemented.

**Step 3: Write minimal implementation**

Implement:

- allowed step kinds
- allowed path matcher for `~/.openclaw` and explicit runtime dirs
- allowlist tags mapped to controlled command prefixes/templates
- result model describing `blocked` reason per step

Do not execute anything yet; only validate.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_repair::policy`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/remote_repair/policy.rs src-tauri/src/remote_repair/types.rs
git commit -m "feat: add remote repair policy validation"
```

### Task 5: Implement SSH-backed step executor

**Files:**
- Create: `src-tauri/src/remote_repair/executor.rs`
- Modify: `src-tauri/src/ssh.rs`
- Modify: `src-tauri/src/remote_repair/types.rs`
- Test: `src-tauri/src/remote_repair/executor.rs`

**Step 1: Write the failing test**

Add executor tests with a fake transport that verify:

- a validated `run_command` step returns `passed`
- a timed out step returns `failed`
- a blocked step is never executed
- `stop` returns a skipped/no-op result with explanatory message

**Step 2: Run test to verify it fails**

Run: `cargo test remote_repair::executor`
Expected: FAIL because no executor exists.

**Step 3: Write minimal implementation**

Create an executor abstraction over SSH operations so tests can use a fake transport.

Support:

- `run_command`
- `restart_service`
- `read_file`
- `write_file`
- `collect_logs`
- `health_check`
- `stop`

Reuse `SshConnectionPool` for the real implementation, but keep transport injection for tests.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_repair::executor`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/remote_repair/executor.rs src-tauri/src/ssh.rs src-tauri/src/remote_repair/types.rs
git commit -m "feat: add remote repair ssh executor"
```

### Task 6: Implement session store and orchestration loop

**Files:**
- Create: `src-tauri/src/remote_repair/session.rs`
- Create: `src-tauri/src/remote_repair/orchestrator.rs`
- Create: `src-tauri/src/remote_repair/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/remote_repair/orchestrator.rs`

**Step 1: Write the failing test**

Add orchestration tests using a fake planner and fake executor to verify:

- healthy-after-first-round completes the session
- repeated failures trigger `session_exhausted`
- blocked plan triggers `policy_blocked`
- planner error triggers `planning_failed`

**Step 2: Run test to verify it fails**

Run: `cargo test remote_repair::orchestrator`
Expected: FAIL because session/orchestrator modules do not exist.

**Step 3: Write minimal implementation**

Implement:

- in-memory session registry keyed by session ID
- state transitions from `diagnosing` through `completed/blocked/failed`
- stop policy handling
- round history
- session snapshot read API

Use injected planner and executor traits so the loop is fully testable without real SSH or bot B.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_repair::orchestrator`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/remote_repair/mod.rs src-tauri/src/remote_repair/session.rs src-tauri/src/remote_repair/orchestrator.rs src-tauri/src/lib.rs
git commit -m "feat: add remote repair orchestration loop"
```

### Task 7: Integrate remote Doctor recheck into orchestrator

**Files:**
- Modify: `src-tauri/src/commands/doctor.rs`
- Modify: `src-tauri/src/remote_repair/orchestrator.rs`
- Test: `src-tauri/src/remote_repair/orchestrator.rs`

**Step 1: Write the failing test**

Extend orchestration tests to verify the session only ends in `completed` when a post-round Doctor recheck returns healthy, and remains failing when planner says stop but Doctor still reports issues.

**Step 2: Run test to verify it fails**

Run: `cargo test remote_repair::orchestrator`
Expected: FAIL because Doctor recheck is not wired into session completion logic.

**Step 3: Write minimal implementation**

Use the existing remote Doctor path as the health truth source for remote instances. The orchestrator must:

- run an initial diagnosis for the target host
- re-run diagnosis after each round
- only mark success on healthy diagnosis

Keep diagnosis data in the session snapshot so the frontend can render it.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_repair::orchestrator`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/commands/doctor.rs src-tauri/src/remote_repair/orchestrator.rs
git commit -m "feat: require doctor recheck for remote repair completion"
```

### Task 8: Add planner client with mock-first fallback

**Files:**
- Create: `src-tauri/src/remote_repair/planner_client.rs`
- Modify: `src-tauri/src/remote_repair/orchestrator.rs`
- Test: `src-tauri/src/remote_repair/planner_client.rs`

**Step 1: Write the failing test**

Add planner client tests verifying:

- valid planner response parses into `RemoteRepairPlan`
- invalid JSON response becomes `planning_failed`
- mock planner mode returns a deterministic safe plan for a gateway restart scenario

**Step 2: Run test to verify it fails**

Run: `cargo test remote_repair::planner_client`
Expected: FAIL because the planner client does not exist.

**Step 3: Write minimal implementation**

Implement a planner trait and client module with:

- a mock planner for local development/tests
- a real planner adapter interface placeholder for bot B
- request/response normalization and schema checks

Do not hardcode network transport details into the orchestrator.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_repair::planner_client`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/remote_repair/planner_client.rs src-tauri/src/remote_repair/orchestrator.rs
git commit -m "feat: add remote repair planner client abstraction"
```

### Task 9: Expose Tauri commands for session lifecycle

**Files:**
- Create: `src-tauri/src/commands/remote_repair.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/commands_delegation.rs`

**Step 1: Write the failing test**

Add command delegation coverage in `src-tauri/tests/commands_delegation.rs` for:

- `start_remote_repair_session`
- `get_remote_repair_session`
- `cancel_remote_repair_session`

The test should assert the commands are registered and return serializable results.

**Step 2: Run test to verify it fails**

Run: `cargo test --test commands_delegation`
Expected: FAIL because the commands are not registered.

**Step 3: Write minimal implementation**

Add Tauri commands that:

- start a session for a `hostId`
- return the current session snapshot
- cancel a running session

Register them in `src-tauri/src/lib.rs` and re-export from `src-tauri/src/commands/mod.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test --test commands_delegation`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/commands/remote_repair.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands_delegation.rs
git commit -m "feat: add remote repair tauri commands"
```

### Task 10: Add progress events for frontend updates

**Files:**
- Modify: `src-tauri/src/remote_repair/orchestrator.rs`
- Modify: `src-tauri/src/commands/remote_repair.rs`
- Test: `src-tauri/src/remote_repair/orchestrator.rs`

**Step 1: Write the failing test**

Add a test proving that round start, step completion, blocked, and completed transitions emit progress snapshots serializable as frontend events.

**Step 2: Run test to verify it fails**

Run: `cargo test remote_repair::orchestrator`
Expected: FAIL because event payloads are not emitted.

**Step 3: Write minimal implementation**

Emit Tauri events similar to `doctor:assistant-progress`, with event payload containing:

- `sessionId`
- `status`
- `round`
- `stepId`
- `stepStatus`
- `message`

Keep event names stable and explicit, for example `remote-repair:progress`.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_repair::orchestrator`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/remote_repair/orchestrator.rs src-tauri/src/commands/remote_repair.rs
git commit -m "feat: emit remote repair progress events"
```

### Task 11: Build Doctor page remote repair UI

**Files:**
- Modify: `src/pages/Doctor.tsx`
- Create: `src/components/RemoteRepairTimeline.tsx`
- Create: `src/components/RemoteRepairPlanDialog.tsx`
- Create: `src/components/RemoteRepairSessionBanner.tsx`
- Test: `src/pages/__tests__/Doctor.test.tsx`
- Test: `src/components/__tests__/DoctorRecoveryOverview.test.tsx`

**Step 1: Write the failing test**

Add Doctor page tests verifying that when the active instance is remote and diagnosis needs repair:

- the page shows a “请求修复计划” action
- plan summary can be reviewed before execution
- progress UI updates when a session snapshot changes
- blocked/failed/completed states render distinct messages

**Step 2: Run test to verify it fails**

Run: `npm test -- Doctor`
Expected: FAIL because the remote repair UI does not exist.

**Step 3: Write minimal implementation**

Implement:

- remote repair CTA in `Doctor.tsx`
- session banner showing status/round
- dialog summarizing returned plan before execution
- step timeline with latest stdout/stderr preview

Preserve existing Doctor flows for local repair and rescue-bot repair.

**Step 4: Run test to verify it passes**

Run: `npm test -- Doctor`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/pages/Doctor.tsx src/components/RemoteRepairTimeline.tsx src/components/RemoteRepairPlanDialog.tsx src/components/RemoteRepairSessionBanner.tsx src/pages/__tests__/Doctor.test.tsx src/components/__tests__/DoctorRecoveryOverview.test.tsx
git commit -m "feat: add remote repair doctor ui"
```

### Task 12: Persist audit logs for remote repair sessions

**Files:**
- Create: `src-tauri/src/remote_repair/audit.rs`
- Modify: `src-tauri/src/remote_repair/orchestrator.rs`
- Modify: `src-tauri/src/models.rs`
- Test: `src-tauri/src/remote_repair/audit.rs`

**Step 1: Write the failing test**

Add tests verifying that each session writes an audit record containing:

- session metadata
- round summaries
- step results
- terminal status

The audit file should land under the ClawPal data directory, not the OpenClaw config directory.

**Step 2: Run test to verify it fails**

Run: `cargo test remote_repair::audit`
Expected: FAIL because audit persistence is not implemented.

**Step 3: Write minimal implementation**

Write audit records under a deterministic path such as `.clawpal/remote-repair/<session-id>.json`, using existing app data path resolution helpers.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_repair::audit`
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/remote_repair/audit.rs src-tauri/src/remote_repair/orchestrator.rs src-tauri/src/models.rs
git commit -m "feat: persist remote repair audit logs"
```

### Task 13: Document runtime configuration and operator guidance

**Files:**
- Modify: `README.md`
- Modify: `docs/mvp-checklist.md`
- Modify: `docs/plans/2026-03-16-remote-repair-design.md`

**Step 1: Write the failing test**

No automated test. Perform a docs gap review and verify the repo does not yet document remote repair session flow, safety boundaries, or operator-visible failure modes.

**Step 2: Verify missing documentation**

Run: `rg -n "remote repair|远程修复|repair plan" README.md docs/mvp-checklist.md docs/plans/2026-03-16-remote-repair-design.md`
Expected: the runtime/operator guidance is incomplete.

**Step 3: Write minimal implementation**

Document:

- how remote repair works end to end
- why plans are locally validated
- where audit logs are stored
- what is intentionally blocked in v1
- acceptance checklist entries for remote repair

**Step 4: Verify documentation is present**

Run: `rg -n "remote repair|远程修复|repair plan|audit" README.md docs/mvp-checklist.md docs/plans/2026-03-16-remote-repair-design.md`
Expected: matching lines in all updated docs.

**Step 5: Commit**

```bash
git add README.md docs/mvp-checklist.md docs/plans/2026-03-16-remote-repair-design.md
git commit -m "docs: add remote repair operator guidance"
```

### Task 14: Final verification

**Files:**
- Verify only

**Step 1: Run targeted Rust tests**

Run: `cargo test remote_repair`
Expected: PASS.

**Step 2: Run targeted frontend tests**

Run: `npm test -- Doctor`
Expected: PASS.

**Step 3: Run broader command wiring tests**

Run: `cargo test --test commands_delegation`
Expected: PASS.

**Step 4: Run project build checks**

Run: `npm run build`
Expected: PASS.

Run: `cargo test`
Expected: PASS or known unrelated failures documented.

**Step 5: Commit**

```bash
git add .
git commit -m "feat: add structured remote repair flow"
```
