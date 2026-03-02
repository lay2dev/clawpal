# Tech Debt Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all tracked technical debt items from the 2026-02-21 code review, prioritized by severity (Critical → Important → Suggestions).

**Architecture:** Incremental refactoring — each task is self-contained and independently committable. Critical fixes first (safety), then important fixes (correctness), then DRY/cleanup suggestions. No behavior changes; all tasks preserve existing functionality.

**Tech Stack:** Rust (Tauri backend), TypeScript/React (frontend), OpenSSH crate

**Source:** `docs/plans/2026-02-21-code-review-remaining.md`

---

## Phase 1: Critical Fixes

### Task 1: Fix `std::env::set_var` Unsafe in Multi-Threaded Context (C4)

**Files:**
- Modify: `src-tauri/src/commands.rs:25-74` (`resolve_openclaw_bin`)

**Step 1: Read current implementation**

The `resolve_openclaw_bin()` function at line 64 calls `std::env::set_var("PATH", ...)` which is unsafe in multi-threaded Rust 1.83+. The function already caches the resolved binary path via `OnceLock`, so the global PATH mutation is unnecessary — we just need child processes to see the extra directory.

**Step 2: Replace `set_var` with stored path prefix**

Replace the `std::env::set_var` call with storing the extra PATH directory in a second `OnceLock`, then using it when spawning child processes.

In `resolve_openclaw_bin`, change:

```rust
// OLD (line 64):
std::env::set_var("PATH", format!("{dir_str}:{current_path}"));

// NEW: store the extra dir so callers can pass it via Command::env
static EXTRA_PATH: OnceLock<Option<String>> = OnceLock::new();
```

Add a public helper:

```rust
/// Returns a modified PATH string with the openclaw bin directory prepended,
/// for use with `Command::env("PATH", ...)`. Returns None if no modification needed.
pub(crate) fn openclaw_env_path() -> Option<&'static str> {
    static EXTRA: OnceLock<Option<String>> = OnceLock::new();
    // Ensure resolve_openclaw_bin has run first
    resolve_openclaw_bin();
    EXTRA.get().and_then(|o| o.as_deref())
}
```

Then update all `Command::new(resolve_openclaw_bin())` call sites to chain `.env("PATH", ...)` when `openclaw_env_path()` returns `Some`.

**Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "fix(C4): replace unsafe set_var with Command::env for PATH"
```

---

### Task 2: Fix `set_global_model` May Clobber Fallbacks (I19)

**Files:**
- Modify: `src-tauri/src/commands.rs:1000-1024` (`set_global_model`)

**Step 1: Read current implementation**

The fallback code path at line 1017 uses `set_nested_value` which replaces the entire `agents.defaults.model` with a plain string, destroying any fallbacks array.

**Step 2: Add object-promotion before fallback**

Replace the fallback path so it always promotes to object format:

```rust
// After the existing object-path check (line 1015), replace the fallback:
// If model is being set and there was no existing object, create one:
match model {
    Some(v) => {
        let model_obj = serde_json::json!({ "primary": v });
        set_nested_value(&mut cfg, "agents.defaults.model", Some(model_obj))?;
    }
    None => {
        set_nested_value(&mut cfg, "agents.defaults.model", None)?;
    }
}
```

**Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "fix(I19): promote model to object format to preserve fallbacks"
```

---

### Task 3: Fix Double Mutex Gap in SSH `connect()` (I18)

**Files:**
- Modify: `src-tauri/src/ssh.rs:158-188` (unix `connect` method)
- Modify: `src-tauri/src/ssh.rs` (windows `connect` method — same fix)

**Step 1: Merge the two critical sections**

Move the `pool.insert(...)` at line 186-187 into the existing lock scope at line 160-184. The old session close + new session insert should happen under a single lock.

```rust
// Replace lines 158-188 with:
{
    let mut pool = self.connections.lock().await;
    if let Some(old) = pool.remove(&config.id) {
        match Arc::try_unwrap(old.session) {
            Ok(old_session) => {
                let _ = old_session.close().await;
            }
            Err(arc) => {
                tokio::spawn(async move {
                    for _ in 0..120 {
                        if Arc::strong_count(&arc) <= 1 { break; }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                    if let Ok(session) = Arc::try_unwrap(arc) {
                        let _ = session.close().await;
                    }
                });
            }
        }
    }
    pool.insert(config.id.clone(), SshConnection {
        session: Arc::new(session), home_dir, config: config.clone()
    });
}
Ok(())
```

Apply the same fix to the `#[cfg(not(unix))]` block's `connect` method.

**Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src-tauri/src/ssh.rs
git commit -m "fix(I18): merge double mutex gap in SSH connect to prevent race"
```

---

## Phase 2: Important Fixes

### Task 4: Extract Duplicate `shell_escape` / `shell_quote` (S11)

**Files:**
- Create: `src-tauri/src/util.rs`
- Modify: `src-tauri/src/commands.rs:16-19` (remove `shell_escape`, import from util)
- Modify: `src-tauri/src/ssh.rs:41-44` (remove `shell_quote`, import from util)
- Modify: `src-tauri/src/lib.rs` (add `mod util;`)

**Step 1: Create `src-tauri/src/util.rs`**

```rust
/// Shell-quote a string using single quotes with proper escaping.
pub(crate) fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
```

**Step 2: Update `lib.rs` to declare the module**

Add `mod util;` to `src-tauri/src/lib.rs`.

**Step 3: Update `commands.rs`**

Remove the local `shell_escape` function (lines 16-19). Add `use crate::util::shell_quote;`. Replace all calls to `shell_escape(...)` with `shell_quote(...)`.

**Step 4: Update `ssh.rs`**

Remove the local `shell_quote` function (lines 41-44). Add `use crate::util::shell_quote;`.

**Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 6: Commit**

```bash
git add src-tauri/src/util.rs src-tauri/src/commands.rs src-tauri/src/ssh.rs src-tauri/src/lib.rs
git commit -m "refactor(S11): extract shared shell_quote to crate::util"
```

---

### Task 5: Extract Duplicate `groupAgents` Function (S3)

**Files:**
- Create: `src/lib/agent-utils.ts`
- Modify: `src/pages/Home.tsx:26-55` (remove local `groupAgents`, import from agent-utils)
- Modify: `src/pages/Channels.tsx:20-55` (remove local `groupAgents`, import from agent-utils)

**Step 1: Create `src/lib/agent-utils.ts`**

Extract the `AgentGroup` interface and `groupAgents` function from Home.tsx into the shared module.

**Step 2: Update Home.tsx and Channels.tsx**

Remove the local `AgentGroup` type and `groupAgents` function from both files. Add `import { groupAgents, AgentGroup } from "@/lib/agent-utils";`.

**Step 3: Verify build**

Run: `npm run build`
Expected: No errors

**Step 4: Commit**

```bash
git add src/lib/agent-utils.ts src/pages/Home.tsx src/pages/Channels.tsx
git commit -m "refactor(S3): extract shared groupAgents to lib/agent-utils"
```

---

### Task 6: Extract Config Mutation Preamble (S13)

**Files:**
- Modify: `src-tauri/src/commands.rs` (add helper, refactor 7 occurrences)

**Step 1: Add helper function**

```rust
/// Load config for mutation, returning (paths, config, snapshot of pre-mutation state).
fn load_config_for_mutation() -> Result<(crate::models::OpenClawPaths, Value, String), String> {
    let paths = resolve_paths();
    let cfg = read_openclaw_config(&paths)?;
    let snapshot = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    Ok((paths, cfg, snapshot))
}
```

**Step 2: Replace all 7 occurrences of the pattern**

Search for the pattern:
```rust
let paths = resolve_paths();
let mut cfg = read_openclaw_config(&paths)?;
let current = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
```

Replace each with:
```rust
let (paths, mut cfg, current) = load_config_for_mutation()?;
```

**Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "refactor(S13): extract load_config_for_mutation helper"
```

---

### Task 7: Extract "Not Found" Guard Pattern (S14)

**Files:**
- Modify: `src-tauri/src/commands.rs` (add helper, refactor 4 occurrences)

**Step 1: Add helper function**

```rust
/// Check if CLI output indicates a "not found" error (e.g. config key missing).
fn is_cli_not_found(output: &crate::cli_runner::CliOutput) -> bool {
    output.exit_code != 0 && {
        let msg = format!("{} {}", output.stderr, output.stdout).to_lowercase();
        msg.contains("not found")
    }
}
```

**Step 2: Replace occurrences**

Find all instances of the pattern in `list_bindings`, `remote_list_bindings`, `list_channels_minimal`, `remote_list_channels_minimal` and replace with:

```rust
if is_cli_not_found(&output) {
    return Ok(Vec::new());
}
```

**Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "refactor(S14): extract is_cli_not_found helper"
```

---

### Task 8: Use `LazyLock` for Compiled Regexes (S4)

**Files:**
- Modify: `src-tauri/src/commands.rs:2103-2105` (`extract_version_from_text`)
- Modify: `src-tauri/src/doctor.rs:73-75` (`clean_and_write_json`)

**Step 1: Convert regex in `commands.rs`**

```rust
fn extract_version_from_text(input: &str) -> Option<String> {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\d+\.\d+(?:\.\d+){1,3}(?:[-+._a-zA-Z0-9]*)?").unwrap()
    });
    RE.find(input).map(|mat| mat.as_str().to_string())
}
```

**Step 2: Convert regex in `doctor.rs`**

```rust
fn clean_and_write_json(paths: &OpenClawPaths, text: &str) -> Result<(), String> {
    use std::sync::LazyLock;
    static TRAILING: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r",(\s*[}\]])").unwrap()
    });
    let normalized = TRAILING.replace_all(text, "$1");
    // ... rest unchanged
```

**Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/doctor.rs
git commit -m "refactor(S4): use LazyLock for compiled regexes"
```

---

### Task 9: Extract Duplicate Model Profile Matching (S15)

**Files:**
- Modify: `src/pages/Home.tsx` (extract `findProfileIdByModelValue`, DRY up the two usages)

**Step 1: Read and understand both usages of profile matching in Home.tsx**

Find the `currentModelProfileId` memo and the per-agent model select, identify the shared logic.

**Step 2: Extract `findProfileIdByModelValue` function**

Add a local helper near the top of the component or in `agent-utils.ts`:

```typescript
function findProfileIdByModelValue(
  modelValue: string | undefined,
  profiles: ModelProfile[]
): string | undefined {
  if (!modelValue) return undefined;
  const normalized = modelValue.trim().toLowerCase();
  return profiles.find(p =>
    p.value.trim().toLowerCase() === normalized
  )?.id;
}
```

**Step 3: Replace both usages**

**Step 4: Verify build**

Run: `npm run build`
Expected: No errors

**Step 5: Commit**

```bash
git add src/pages/Home.tsx
git commit -m "refactor(S15): extract findProfileIdByModelValue helper"
```

---

### Task 10: Rename `state.ts` / `AppState` → `DoctorState` (S7)

**Files:**
- Modify: `src/lib/state.ts` (rename `AppState` → `DoctorState`, rename file if used only by Doctor)
- Modify: all importers of `state.ts`

**Step 1: Verify state.ts is only used by Doctor**

Search for imports of `state.ts` across the codebase.

**Step 2: Rename types inline**

Rename `AppState` → `DoctorState`, `initialState` → `initialDoctorState` if appropriate, and update all references.

**Step 3: Verify build**

Run: `npm run build`
Expected: No errors

**Step 4: Commit**

```bash
git add src/lib/state.ts src/pages/Doctor.tsx
git commit -m "refactor(S7): rename AppState to DoctorState for clarity"
```

---

### Task 11: Add `console.warn` to Silent `.catch(() => {})` (S9)

**Files:**
- Modify: `src/pages/Cron.tsx:130,132`
- Modify: `src/pages/Home.tsx:96,772`
- Modify: `src/pages/Settings.tsx:161`
- Modify: `src/App.tsx:100,110-111`
- Modify: `src/components/PendingChangesBar.tsx:59,73`

**Step 1: Replace empty catches with console.warn**

For each `.catch(() => {})`, replace with `.catch((e) => console.warn("[context]:", e))` where `[context]` identifies the operation (e.g., "loadJobs", "checkUpdate", "analytics").

**Step 2: Verify build**

Run: `npm run build`
Expected: No errors

**Step 3: Commit**

```bash
git add src/pages/Cron.tsx src/pages/Home.tsx src/pages/Settings.tsx src/App.tsx src/components/PendingChangesBar.tsx
git commit -m "refactor(S9): replace silent .catch with console.warn for debuggability"
```

---

## Phase 3: Deferred / Out of Scope

The following items are **not included** in this refactor pass:

| Item | Reason |
|------|--------|
| **C3** (curl\|bash upgrade) | Requires upstream openclaw.ai to publish checksums |
| **I2** (blocking I/O in sync commands) | Larger effort, needs separate design |
| **I17** (oversized App.tsx/Doctor.tsx) | Large component split, separate task |
| **I16** (ARIA roles) | Accessibility pass is a separate workstream |
| **S1** (split commands.rs into modules) | Major restructuring, separate task |
| **S5** (config concurrent access Mutex) | Needs design for async Mutex around config |
| **S6** (resolve_paths side effects) | Startup refactor, separate task |
| **S8** (nav accessibility) | Accessibility pass |
| **S10** (Chat.tsx AGENT_ID) | Needs agent discovery API design |
| **S12** (SSH Unix/Windows dedup) | Large platform abstraction refactor |

---

## Summary

| Phase | Tasks | Effort | Priority |
|-------|-------|--------|----------|
| Phase 1 (Critical) | Tasks 1-3 | Low | Must-fix |
| Phase 2 (Cleanup) | Tasks 4-11 | Low-Medium | Should-fix |
| Phase 3 (Deferred) | 10 items | Medium-High | Separate workstreams |

**Total estimated tasks:** 11 (this plan)
**Branch:** `refactor/tech-debt-cleanup`
