# ZeroClaw Orchestrator Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Integrate embedded `zeroclaw` sidecar as a planning engine and progressively migrate ClawPal install/doctor flows from hardcoded branching to plan-execute orchestration with adaptive access discovery.

**Architecture:** Keep current user-facing Install/Doctor entry points while introducing a new backend orchestration layer. Zeroclaw sidecar generates structured plans; ClawPal executes steps through a restricted data-plane toolset, enforces risk gates, and records audit/security events. Rollout is phased (M1-M4) with fallback to legacy logic at each phase.

**Tech Stack:** Tauri 2 (Rust), existing ClawPal backend commands and cli runner, React + TypeScript frontend, local JSON persistence under `~/.clawpal`, existing test surfaces (`cargo test`, `npm run typecheck`).

---

### Task 1: Add AAD Domain Types and Persistence (M1)

**Files:**
- Create: `src-tauri/src/access_discovery/types.rs`
- Create: `src-tauri/src/access_discovery/store.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/access_discovery.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn capability_profile_roundtrip() {
    let profile = CapabilityProfile::example_local("local");
    let text = serde_json::to_string(&profile).unwrap();
    let parsed: CapabilityProfile = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed.instance_id, "local");
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test access_discovery capability_profile_roundtrip -- --nocapture`  
Expected: FAIL with missing module/types.

**Step 3: Write minimal implementation**

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CapabilityProfile {
  pub instance_id: String,
  pub transport: String,
  pub probes: Vec<ProbeResult>,
  pub working_chain: Vec<String>,
  pub env_contract: std::collections::BTreeMap<String, String>,
  pub verified_at: u64,
  pub ttl_secs: u64,
}
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test access_discovery capability_profile_roundtrip -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/access_discovery/types.rs src-tauri/src/access_discovery/store.rs src-tauri/src/lib.rs src-tauri/tests/access_discovery.rs
git commit -m "feat(aad): add capability profile domain and persistence"
```

### Task 2: Implement Probe Engine with Restricted Command Strategy (M1)

**Files:**
- Create: `src-tauri/src/access_discovery/probe_engine.rs`
- Modify: `src-tauri/src/cli_runner.rs`
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/tests/access_discovery.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn probe_plan_has_fallbacks() {
    let plan = build_probe_plan_for_local();
    assert!(!plan.is_empty());
    assert!(plan.iter().any(|p| p.contains("--version")));
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test access_discovery probe_plan_has_fallbacks -- --nocapture`  
Expected: FAIL with missing probe engine.

**Step 3: Write minimal implementation**

```rust
pub fn build_probe_plan_for_local() -> Vec<String> { /* which/openclaw/--version/status */ }
pub fn run_probe_with_redaction(...) -> ProbeResult { /* masked logs */ }
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test access_discovery probe_plan_has_fallbacks -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/access_discovery/probe_engine.rs src-tauri/src/cli_runner.rs src-tauri/src/commands.rs src-tauri/tests/access_discovery.rs
git commit -m "feat(aad): add probe engine with command fallback strategy"
```

### Task 3: Add AAD Entry Command + Legacy Fallback Integration (M1)

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/use-api.ts`
- Test: `src-tauri/tests/access_discovery.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn ensure_access_profile_falls_back_when_probe_fails() {
    let result = ensure_access_profile_for_test("local").await.unwrap();
    assert!(result.used_legacy_fallback || !result.working_chain.is_empty());
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test access_discovery ensure_access_profile_falls_back_when_probe_fails -- --nocapture`  
Expected: FAIL with missing command.

**Step 3: Write minimal implementation**

```rust
#[tauri::command]
pub async fn ensure_access_profile(instance_id: String, transport: String) -> Result<EnsureAccessResult, String> {
  // load profile -> try chain -> reprobe -> fallback to legacy
}
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test access_discovery ensure_access_profile_falls_back_when_probe_fails -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/api.ts src/lib/use-api.ts src-tauri/tests/access_discovery.rs
git commit -m "feat(aad): integrate ensure-access command with legacy fallback"
```

### Task 4: Add Security Event Pipeline for Secret Exposure (M1)

**Files:**
- Create: `src-tauri/src/orchestrator/security_events.rs`
- Modify: `src-tauri/src/access_discovery/probe_engine.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/lib/types.ts`
- Test: `src-tauri/tests/access_discovery.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn secret_detector_flags_api_key_like_output() {
    let events = detect_security_events("api_key=sk-test-1234567890");
    assert!(events.iter().any(|e| e.code == "secret_exposed"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test access_discovery secret_detector_flags_api_key_like_output -- --nocapture`  
Expected: FAIL with missing detector.

**Step 3: Write minimal implementation**

```rust
pub fn detect_security_events(text: &str) -> Vec<SecurityEvent> { /* regex + mask */ }
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test access_discovery secret_detector_flags_api_key_like_output -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/orchestrator/security_events.rs src-tauri/src/access_discovery/probe_engine.rs src-tauri/src/commands.rs src/lib/types.ts src-tauri/tests/access_discovery.rs
git commit -m "feat(security): add secret exposure event detection for probe logs"
```

### Task 5: Add Sidecar Lifecycle Manager Skeleton (M2 Prep)

**Files:**
- Create: `src-tauri/src/agent_sidecar/manager.rs`
- Create: `src-tauri/src/agent_sidecar/types.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/tests/sidecar_manager.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn sidecar_state_transitions_idle_to_running() {
    let mut mgr = SidecarManager::new_for_test();
    assert_eq!(mgr.state(), SidecarState::Idle);
    mgr.mark_running_for_test();
    assert_eq!(mgr.state(), SidecarState::Running);
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test sidecar_manager sidecar_state_transitions_idle_to_running -- --nocapture`  
Expected: FAIL with missing manager module.

**Step 3: Write minimal implementation**

```rust
pub enum SidecarState { Idle, Starting, Running, Failed }
pub struct SidecarManager { /* path, pid, health */ }
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test sidecar_manager sidecar_state_transitions_idle_to_running -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/agent_sidecar/manager.rs src-tauri/src/agent_sidecar/types.rs src-tauri/src/lib.rs src-tauri/src/commands.rs src-tauri/tests/sidecar_manager.rs
git commit -m "feat(sidecar): add zeroclaw sidecar lifecycle manager skeleton"
```

### Task 6: Define Plan/Result Protocol DTOs and Validator (M2 Prep)

**Files:**
- Create: `src-tauri/src/orchestrator/protocol.rs`
- Modify: `src-tauri/src/orchestrator/security_events.rs`
- Test: `src-tauri/tests/orchestrator_protocol.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn high_risk_step_requires_rollback_hint() {
    let plan = sample_plan_without_high_risk_rollback();
    let err = validate_plan(&plan).unwrap_err();
    assert!(err.contains("rollback_hint"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test orchestrator_protocol high_risk_step_requires_rollback_hint -- --nocapture`  
Expected: FAIL with missing protocol validator.

**Step 3: Write minimal implementation**

```rust
pub struct ActionPlan { /* plan_id, task_type, target, steps */ }
pub fn validate_plan(plan: &ActionPlan) -> Result<(), String> { /* risk gate */ }
```

**Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test orchestrator_protocol high_risk_step_requires_rollback_hint -- --nocapture`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src-tauri/src/orchestrator/protocol.rs src-tauri/src/orchestrator/security_events.rs src-tauri/tests/orchestrator_protocol.rs
git commit -m "feat(orchestrator): add action plan protocol and validator"
```

### Task 7: Minimal UI Hook for Access Profile Health (M1 UX)

**Files:**
- Modify: `src/lib/api.ts`
- Modify: `src/lib/use-api.ts`
- Modify: `src/pages/Doctor.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh.json`

**Step 1: Write the failing test**

No frontend test harness is available. Use typecheck gate and manual runtime checks.

**Step 2: Run check to verify baseline**

Run: `npm run typecheck`  
Expected: PASS before changes.

**Step 3: Write minimal implementation**

```ts
// doctor section: Access Discovery
// - last verified time
// - transport
// - reprobe button
```

**Step 4: Run check to verify changes**

Run: `npm run typecheck`  
Expected: PASS.

**Step 5: Commit**

```bash
git add src/lib/api.ts src/lib/use-api.ts src/pages/Doctor.tsx src/locales/en.json src/locales/zh.json
git commit -m "feat(doctor): show access discovery profile status and reprobe action"
```

### Task 8: End-to-End Verification + Documentation Update

**Files:**
- Modify: `docs/mvp-checklist.md`
- Modify: `docs/plans/2026-02-25-zeroclaw-orchestrator-design.md`

**Step 1: Run Rust tests**

Run: `cd src-tauri && cargo test -- --nocapture`  
Expected: PASS (or known external-env failures clearly documented).

**Step 2: Run frontend checks**

Run: `npm run typecheck && npm run build`  
Expected: PASS.

**Step 3: Manual smoke test checklist**

- Local instance: first run probes and stores capability profile
- SSH instance: failure path surfaces probe evidence and fallback result
- Docker instance: openclaw command chain persists and is reused
- Secret-like output triggers `security_alert` and masked log output

**Step 4: Update docs**

- Mark M1 acceptance in `docs/mvp-checklist.md`
- Record known limitations and next milestones (M2-M4)

**Step 5: Commit**

```bash
git add docs/mvp-checklist.md docs/plans/2026-02-25-zeroclaw-orchestrator-design.md
git commit -m "docs: add zeroclaw orchestrator m1 verification and acceptance updates"
```

---

## M2-M4 (Planning Summary)

- M2: sidecar plan generation wired to install flow (legacy fallback retained)
- M3: doctor flow migrated to plan-executor with static-check fallback
- M4: unified orchestration panel + full audit timeline + mode control

