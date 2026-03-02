# OpenClaw Install Onboarding Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a user-select-first OpenClaw installation hub (Local/WSL2/Docker/Remote SSH) that guides users step-by-step and then seamlessly returns them to existing Home/Recipes configuration flows.

**Architecture:** Implement a vertical slice with a new Home-level Install Hub UI, a typed frontend session/state model, and new Tauri install commands backed by a session manager. Start with deterministic command generation + step execution contract, then wire method-specific runners and final readiness handoff to existing status refresh/navigation.

**Tech Stack:** React + TypeScript (Vite, i18next), Tauri 2 (Rust), existing API bridge (`src/lib/api.ts`), existing invoke handler (`src-tauri/src/lib.rs`), Rust integration tests in `src-tauri/tests`.

---

### Task 1: Define Install Domain Types (Frontend + Rust)

**Files:**
- Create: `src-tauri/src/install/types.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/types.ts`

**Step 1: Write the failing test**

```rust
// src-tauri/tests/install_api.rs
#[test]
fn install_session_serialization_roundtrip() {
    let json = r#"{"method":"local","state":"idle"}"#;
    let parsed: clawpal::install::types::InstallSession = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.method.as_str(), "local");
    assert_eq!(parsed.state.as_str(), "idle");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test install_api install_session_serialization_roundtrip -- --nocapture`  
Expected: FAIL with unresolved `install` module/types.

**Step 3: Write minimal implementation**

```rust
// src-tauri/src/install/types.rs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum InstallMethod { Local, Wsl2, Docker, RemoteSsh }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum InstallState { Idle, SelectedMethod, PrecheckRunning, PrecheckFailed, PrecheckPassed, InstallRunning, InstallFailed, InstallPassed, InitRunning, InitFailed, InitPassed, Ready }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstallSession { pub id: String, pub method: InstallMethod, pub state: InstallState }
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test install_api install_session_serialization_roundtrip -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/install/types.rs src-tauri/src/lib.rs src/lib/types.ts src-tauri/tests/install_api.rs
git commit -m "feat(install): add shared install session domain types"
```

### Task 2: Add Tauri Install Session Commands Skeleton

**Files:**
- Create: `src-tauri/src/install/commands.rs`
- Create: `src-tauri/src/install/session_store.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api.ts`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn create_session_returns_selected_method_state() {
    let session = clawpal::install::commands::create_session_for_test("local").await.unwrap();
    assert_eq!(session.state.as_str(), "selected_method");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test install_api create_session_returns_selected_method_state -- --nocapture`  
Expected: FAIL with missing command/store symbols.

**Step 3: Write minimal implementation**

```rust
#[tauri::command]
pub async fn install_create_session(method: String) -> Result<InstallSession, String> {
    // allocate id + put into in-memory store
}

#[tauri::command]
pub async fn install_get_session(session_id: String) -> Result<InstallSession, String> { /* ... */ }
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test install_api create_session_returns_selected_method_state -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/install/commands.rs src-tauri/src/install/session_store.rs src-tauri/src/lib.rs src/lib/api.ts src-tauri/tests/install_api.rs
git commit -m "feat(install): add install session create/get command skeleton"
```

### Task 3: Implement Step Execution Contract + Error Codes

**Files:**
- Modify: `src-tauri/src/install/types.rs`
- Modify: `src-tauri/src/install/commands.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn run_step_precheck_updates_state_and_next_step() {
    let session = create_session_for_test("local").await.unwrap();
    let result = run_step_for_test(&session.id, "precheck").await.unwrap();
    assert!(result.ok);
    assert_eq!(result.next_step.as_deref(), Some("install"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test install_api run_step_precheck_updates_state_and_next_step -- --nocapture`  
Expected: FAIL with missing `install_run_step` behavior.

**Step 3: Write minimal implementation**

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstallStepResult {
  pub ok: bool,
  pub summary: String,
  pub error_code: Option<String>,
  pub next_step: Option<String>,
}

#[tauri::command]
pub async fn install_run_step(session_id: String, step: String) -> Result<InstallStepResult, String> {
  // enforce state transition table + emit env_missing/permission_denied/network_error/command_failed/validation_failed
}
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test install_api run_step_precheck_updates_state_and_next_step -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/install/types.rs src-tauri/src/install/commands.rs src/lib/types.ts src/lib/api.ts src-tauri/tests/install_api.rs
git commit -m "feat(install): add step execution contract with typed error codes"
```

### Task 4: Add Method Capability Discovery (User-Select-First Guardrails)

**Files:**
- Modify: `src-tauri/src/install/commands.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn list_methods_returns_all_four_methods() {
    let methods = list_methods_for_test().await.unwrap();
    let names: Vec<String> = methods.into_iter().map(|m| m.method).collect();
    assert_eq!(names, vec!["local", "wsl2", "docker", "remote_ssh"]);
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test install_api list_methods_returns_all_four_methods -- --nocapture`  
Expected: FAIL due to missing list command.

**Step 3: Write minimal implementation**

```rust
#[tauri::command]
pub async fn install_list_methods() -> Result<Vec<InstallMethodCapability>, String> {
  // return all methods + availability hints (no hard-block for MVP)
}
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test install_api list_methods_returns_all_four_methods -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/install/commands.rs src/lib/types.ts src/lib/api.ts src-tauri/tests/install_api.rs
git commit -m "feat(install): add install method capability discovery"
```

### Task 5: Build Home Install Hub Shell (Method Picker + Session Lifecycle)

**Files:**
- Create: `src/components/InstallHub.tsx`
- Modify: `src/pages/Home.tsx`
- Modify: `src/lib/use-api.ts`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh.json`

**Step 1: Write the failing test**

Since frontend test runner is not configured in this repo, use compile-time acceptance as gate for MVP UI scaffolding.

Create expected contract snippet in `InstallHub.tsx`:

```tsx
const methods = await ua.listInstallMethods();
```

**Step 2: Run test to verify it fails**

Run: `npm run typecheck`  
Expected: FAIL with missing `listInstallMethods` / install types in `useApi`.

**Step 3: Write minimal implementation**

```tsx
// InstallHub: method list + "Start" -> installCreateSession(method)
// Home: render <InstallHub /> near top CTA section
```

**Step 4: Run test to verify it passes**

Run: `npm run typecheck`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/components/InstallHub.tsx src/pages/Home.tsx src/lib/use-api.ts src/locales/en.json src/locales/zh.json
git commit -m "feat(home): add install hub entry with user-selected install method"
```

### Task 6: Implement Step Runner UI (Precheck/Install/Init/Verify)

**Files:**
- Modify: `src/components/InstallHub.tsx`
- Modify: `src/lib/use-api.ts`
- Modify: `src/lib/types.ts`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh.json`

**Step 1: Write the failing test**

Introduce required call flow in component:

```tsx
await ua.installRunStep(session.id, "precheck");
```

**Step 2: Run test to verify it fails**

Run: `npm run typecheck`  
Expected: FAIL if step API response fields/state mapping missing.

**Step 3: Write minimal implementation**

```tsx
// Add step cards with status badge, retry current step button,
// command summary panel, and next-step CTA.
```

**Step 4: Run test to verify it passes**

Run: `npm run typecheck`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/components/InstallHub.tsx src/lib/use-api.ts src/lib/types.ts src/locales/en.json src/locales/zh.json
git commit -m "feat(install-ui): add step-by-step runner with retry and logs"
```

### Task 7: Wire Ready-State Handoff to Existing Config Flow

**Files:**
- Modify: `src/components/InstallHub.tsx`
- Modify: `src/pages/Home.tsx`
- Modify: `src/App.tsx`

**Step 1: Write the failing test**

Add explicit expected callback use:

```tsx
onReady?.({ navigateTo: "home", suggestDoctor: true, suggestRecipe: true });
```

**Step 2: Run test to verify it fails**

Run: `npm run typecheck`  
Expected: FAIL until callback props and wiring are added.

**Step 3: Write minimal implementation**

```tsx
// On ready:
// 1) refresh status/agents
// 2) auto navigate to home
// 3) show next-step CTAs: Run Doctor / Open Recipes
```

**Step 4: Run test to verify it passes**

Run: `npm run typecheck && npm run build`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/components/InstallHub.tsx src/pages/Home.tsx src/App.tsx
git commit -m "feat(install): hand off ready install sessions to existing config workflow"
```

### Task 8: Add Method-Specific Runners for Local/WSL2/Docker/Remote SSH (MVP)

**Files:**
- Create: `src-tauri/src/install/runners/mod.rs`
- Create: `src-tauri/src/install/runners/local.rs`
- Create: `src-tauri/src/install/runners/wsl2.rs`
- Create: `src-tauri/src/install/runners/docker.rs`
- Create: `src-tauri/src/install/runners/remote_ssh.rs`
- Modify: `src-tauri/src/install/commands.rs`
- Modify: `src-tauri/tests/install_api.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn local_precheck_returns_command_summary() {
    let result = run_local_precheck_for_test().await.unwrap();
    assert!(result.commands.len() > 0);
    assert!(result.summary.contains("precheck"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test install_api local_precheck_returns_command_summary -- --nocapture`  
Expected: FAIL with missing runners.

**Step 3: Write minimal implementation**

```rust
pub trait InstallRunner {
  async fn run_precheck(&self) -> Result<InstallStepResult, InstallError>;
  async fn run_install(&self) -> Result<InstallStepResult, InstallError>;
  async fn run_init(&self) -> Result<InstallStepResult, InstallError>;
  async fn run_verify(&self) -> Result<InstallStepResult, InstallError>;
}
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test install_api -- --nocapture`  
Expected: PASS for install test suite.

**Step 5: Commit**

```bash
git add src-tauri/src/install/runners src-tauri/src/install/commands.rs src-tauri/tests/install_api.rs
git commit -m "feat(install): add method-specific install runners for local/wsl2/docker/remote"
```

### Task 9: Documentation + Checklist Update

**Files:**
- Modify: `README.md`
- Modify: `docs/mvp-checklist.md`
- Create: `docs/plans/2026-02-24-openclaw-install-onboarding-implementation.md`

**Step 1: Write the failing test**

Use docs acceptance checklist entries as explicit targets (manual test script block in doc).

**Step 2: Run test to verify it fails**

Run manual walkthrough: start app -> Home -> Install Hub -> each method visible -> one method reaches `ready` -> auto return Home.

Expected: At least one unchecked item before docs update.

**Step 3: Write minimal implementation**

Update docs with:
- new install flow entry
- success criteria
- known limitations

**Step 4: Run test to verify it passes**

Run: `npm run typecheck && npm run build && cd src-tauri && cargo test --test install_api -- --nocapture`  
Expected: PASS, checklist updated.

**Step 5: Commit**

```bash
git add README.md docs/mvp-checklist.md docs/plans/2026-02-24-openclaw-install-onboarding-implementation.md
git commit -m "docs: document install onboarding flow and MVP acceptance"
```

## Verification Commands (Final Gate)

Run in order:

1. `npm run typecheck`
2. `npm run build`
3. `cd src-tauri && cargo test --test install_api -- --nocapture`
4. `cd src-tauri && cargo test --test remote_api -- --nocapture`

Expected:
- TypeScript compile passes
- Vite production build passes
- New install API tests pass
- Existing remote API tests remain green

## Notes for Executor

- Keep changes atomic and follow conventional commits.
- Do not block MVP on full automatic installers; command-summary + guided steps is enough.
- Reuse existing SSH host model (`SshHost`) for `remote_ssh` path to avoid duplicate credential stores.
- Ensure logs redact credentials before rendering in UI or returning via Tauri commands.
