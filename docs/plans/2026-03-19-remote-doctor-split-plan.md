# Remote Doctor Module Split Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Split the oversized remote doctor Rust implementation into focused modules, tighten internal naming, and keep the public behavior, command interface, events, and repair semantics unchanged.

**Architecture:** Convert `src-tauri/src/remote_doctor.rs` into a directory module with clear responsibility boundaries: shared types, session infrastructure, config/identity helpers, agent planner helpers, plan execution, and repair-loop orchestration. Preserve the existing top-level entrypoint and move tests alongside the responsibilities they verify so the refactor stays behaviorally stable.

**Tech Stack:** Rust, Tauri 2, Tokio, Serde, Cargo test

---

### Task 1: Create the module shell

**Files:**
- Create: `src-tauri/src/remote_doctor/mod.rs`
- Create: `src-tauri/src/remote_doctor/types.rs`
- Create: `src-tauri/src/remote_doctor/session.rs`
- Create: `src-tauri/src/remote_doctor/config.rs`
- Create: `src-tauri/src/remote_doctor/agent.rs`
- Create: `src-tauri/src/remote_doctor/plan.rs`
- Create: `src-tauri/src/remote_doctor/repair_loops.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Write the failing test**

Add a minimal compile-oriented unit test module in `src-tauri/src/remote_doctor/types.rs` that references `TargetLocation` and `PlanKind`, and wire `src-tauri/src/lib.rs` to `pub mod remote_doctor;`.

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::types`
Expected: FAIL because the directory module does not exist yet.

**Step 3: Write minimal implementation**

Create the directory module files with empty or placeholder implementations and move only the shared constants plus the public `start_remote_doctor_repair` export into the new `mod.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::types`
Expected: PASS with the placeholder module structure compiling.

**Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/remote_doctor
git commit -m "refactor: scaffold remote doctor module layout"
```

### Task 2: Move shared types and parsing helpers

**Files:**
- Modify: `src-tauri/src/remote_doctor/types.rs`
- Modify: `src-tauri/src/remote_doctor/mod.rs`
- Test: `src-tauri/src/remote_doctor/types.rs`

**Step 1: Write the failing test**

Add tests for:

- `parse_target_location("local_openclaw")`
- `parse_target_location("remote_openclaw")`
- `parse_target_location("elsewhere")`
- `RepairRoundObservation::new(...)` generating a stable diagnosis signature

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::types`
Expected: FAIL because the moved types and helpers are not implemented in the new module.

**Step 3: Write minimal implementation**

Move these definitions into `types.rs`:

- `TargetLocation`
- `PlanKind`
- `PlanCommand`
- `PlanResponse`
- `CommandResult`
- `RemoteDoctorProtocol`
- `ClawpalServerPlanResponse`
- `ClawpalServerPlanStep`
- `RemoteDoctorRepairResult`
- `RemoteDoctorProgressEvent`
- `ConfigExcerptContext`
- `RepairRoundObservation`
- `StoredRemoteDoctorIdentity`
- `parse_target_location`

Re-export only the pieces other modules need from `mod.rs`.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::types`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/types.rs src-tauri/src/remote_doctor/mod.rs
git commit -m "refactor: move remote doctor shared types"
```

### Task 3: Move session logging and completion helpers

**Files:**
- Modify: `src-tauri/src/remote_doctor/session.rs`
- Modify: `src-tauri/src/remote_doctor/mod.rs`
- Test: `src-tauri/src/remote_doctor/session.rs`

**Step 1: Write the failing test**

Add tests for:

- `append_session_log` writing a JSONL line into the expected temp directory
- `emit_session_progress` building the expected `planKind` string
- completion helpers preserving `session_id`, `round`, and `last_command`

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::session`
Expected: FAIL because logging and completion helpers still live elsewhere.

**Step 3: Write minimal implementation**

Move and rename:

- `remote_doctor_log_dir` -> `session_log_dir`
- `append_remote_doctor_log` -> `append_session_log`
- `emit_progress` -> `emit_session_progress`
- `result_for_completion`
- `result_for_completion_with_warnings`

Update internal call sites to use the new names.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::session`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/session.rs src-tauri/src/remote_doctor/mod.rs
git commit -m "refactor: extract remote doctor session helpers"
```

### Task 4: Move config, identity, and target I/O helpers

**Files:**
- Modify: `src-tauri/src/remote_doctor/config.rs`
- Modify: `src-tauri/src/remote_doctor/mod.rs`
- Test: `src-tauri/src/remote_doctor/config.rs`

**Step 1: Write the failing test**

Add tests for:

- `load_gateway_config` preferring app preferences over config file port
- `build_gateway_credentials` returning `None` when the token override is empty
- `load_or_create_remote_doctor_identity` persisting a usable identity
- `build_config_excerpt_context` capturing parse errors for invalid JSON

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::config`
Expected: FAIL because the helpers have not been moved or renamed yet.

**Step 3: Write minimal implementation**

Move and rename into `config.rs`:

- `remote_doctor_gateway_config` -> `load_gateway_config`
- `remote_doctor_gateway_credentials` -> `build_gateway_credentials`
- `remote_doctor_identity_path`
- `load_or_create_remote_doctor_identity`
- `read_target_config`
- `read_target_config_raw`
- `build_config_excerpt_context`
- `config_excerpt_log_summary`
- `empty_config_excerpt_context`
- `empty_diagnosis`
- `write_target_config`
- `write_target_config_raw`
- `restart_target_gateway`
- rescue diagnosis helpers and rescue preflight helpers

Keep non-I/O pure diagnosis summarizers near the same module if they are only used by config and repair loops.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::config`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/config.rs src-tauri/src/remote_doctor/mod.rs
git commit -m "refactor: extract remote doctor config and target io"
```

### Task 5: Move agent-planner-specific helpers

**Files:**
- Modify: `src-tauri/src/remote_doctor/agent.rs`
- Modify: `src-tauri/src/remote_doctor/mod.rs`
- Test: `src-tauri/src/remote_doctor/agent.rs`

**Step 1: Write the failing test**

Add tests for:

- `ensure_agent_workspace_ready` writing bootstrap files
- `build_agent_plan_prompt` containing target location, config excerpt, and command constraints
- `parse_agent_plan_response` extracting the JSON payload correctly
- `next_agent_plan_kind_for_round` switching from investigate to repair after prior results

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::agent`
Expected: FAIL because the planner helpers remain in the monolithic file.

**Step 3: Write minimal implementation**

Move and rename into `agent.rs`:

- protocol selection helpers
- next-plan-kind helpers
- agent id and session key helpers
- workspace bootstrap file helper
- `ensure_local_remote_doctor_agent_ready` -> `ensure_agent_workspace_ready`
- bridge connection helper
- `extract_json_block`
- `build_agent_plan_prompt`
- `parse_agent_plan_response`
- `run_agent_request_with_bridge`

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::agent`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/agent.rs src-tauri/src/remote_doctor/mod.rs
git commit -m "refactor: extract remote doctor agent planner helpers"
```

### Task 6: Move plan parsing, validation, and command execution

**Files:**
- Modify: `src-tauri/src/remote_doctor/plan.rs`
- Modify: `src-tauri/src/remote_doctor/mod.rs`
- Test: `src-tauri/src/remote_doctor/plan.rs`

**Step 1: Write the failing test**

Add tests for:

- `build_shell_command` escaping single quotes
- `parse_invoke_argv` supporting command-string payloads
- `validate_plan_command_argv` rejecting unsupported `openclaw` commands
- `parse_plan_response` filling in a generated `plan_id` when missing

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::plan`
Expected: FAIL because the execution and validation helpers have not moved yet.

**Step 3: Write minimal implementation**

Move into `plan.rs`:

- `request_plan`
- `request_clawpal_server_plan`
- step/final result reporting helpers
- `parse_plan_response`
- `parse_invoke_argv`
- `execute_clawpal_command`
- `execute_clawpal_doctor_command`
- `config_read_response`
- `decode_base64_config_payload`
- `execute_invoke_payload`
- `shell_escape`
- `build_shell_command`
- `execute_command`
- validation helpers
- `execute_plan_command`
- `command_result_stdout`

Keep the file focused on “planner output to executable command results”.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::plan`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/plan.rs src-tauri/src/remote_doctor/mod.rs
git commit -m "refactor: extract remote doctor plan execution"
```

### Task 7: Move repair loops and public entrypoint orchestration

**Files:**
- Modify: `src-tauri/src/remote_doctor/repair_loops.rs`
- Modify: `src-tauri/src/remote_doctor/mod.rs`
- Test: `src-tauri/src/remote_doctor/repair_loops.rs`

**Step 1: Write the failing test**

Add tests for:

- generic remote doctor loop stopping on a healthy detect plan
- round limit failure surfacing the expected message
- agent or legacy fallback preserving the same fallback order as before

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::repair_loops`
Expected: FAIL because the orchestration code still lives in the old monolithic file.

**Step 3: Write minimal implementation**

Move into `repair_loops.rs`:

- `run_remote_doctor_repair_loop`
- `run_clawpal_server_repair_loop`
- `run_agent_planner_repair_loop`
- `start_remote_doctor_repair_impl`

Leave `#[tauri::command] pub async fn start_remote_doctor_repair(...)` in `mod.rs` as the only public entrypoint wrapper.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::repair_loops`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor/repair_loops.rs src-tauri/src/remote_doctor/mod.rs
git commit -m "refactor: extract remote doctor repair orchestration"
```

### Task 8: Split the tests by responsibility

**Files:**
- Create: `src-tauri/src/remote_doctor/tests/mod.rs`
- Create: `src-tauri/src/remote_doctor/tests/types.rs`
- Create: `src-tauri/src/remote_doctor/tests/session.rs`
- Create: `src-tauri/src/remote_doctor/tests/config.rs`
- Create: `src-tauri/src/remote_doctor/tests/agent.rs`
- Create: `src-tauri/src/remote_doctor/tests/plan.rs`
- Create: `src-tauri/src/remote_doctor/tests/repair_loops.rs`
- Create: `src-tauri/src/remote_doctor/tests/live_e2e.rs`
- Delete: `src-tauri/src/remote_doctor.rs`

**Step 1: Write the failing test**

Move one existing assertion from the monolithic `mod tests` into each new test file and keep the old monolithic block temporarily disabled so compilation fails until module wiring is fixed.

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor::tests`
Expected: FAIL because the new test modules are not fully wired and some helpers are not imported yet.

**Step 3: Write minimal implementation**

Split the existing tests into themed files:

- pure type and parser tests
- session/logging tests
- config and identity tests
- agent planner tests
- plan execution and validation tests
- repair loop tests
- live e2e tests guarded by the same environment checks as today

Delete the old monolithic `remote_doctor.rs` only after the directory module fully compiles.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor::tests`
Expected: PASS, with live e2e tests still skipping when their environment variables are absent.

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor src-tauri/src/lib.rs
git commit -m "refactor: split remote doctor tests by module"
```

### Task 9: Run focused regression verification

**Files:**
- Modify: `docs/plans/2026-03-19-remote-doctor-split-plan.md`

**Step 1: Write the failing test**

No new test code. This task verifies the refactor did not change behavior.

**Step 2: Run test to verify it fails**

Run: `cargo test remote_doctor`
Expected: If anything still fails, fix the failing import, visibility, or naming regression before proceeding.

**Step 3: Write minimal implementation**

Fix only the regressions surfaced by the focused remote doctor test run. Do not introduce unrelated cleanup.

**Step 4: Run test to verify it passes**

Run: `cargo test remote_doctor`
Expected: PASS, with environment-gated live tests skipping when not configured.

**Step 5: Commit**

```bash
git add src-tauri/src/remote_doctor src-tauri/src/lib.rs docs/plans/2026-03-19-remote-doctor-split-plan.md
git commit -m "test: verify remote doctor module split"
```
