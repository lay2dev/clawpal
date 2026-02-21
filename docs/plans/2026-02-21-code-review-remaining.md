# Code Review — Remaining Items

Date: 2026-02-21
Status: Tracked tech debt from full-project code review.
Commit with fixes applied: `4421750`

---

## CRITICAL — Must fix before public release

### C1. SSH Host Key Verification Disabled (MITM)

**File:** `src-tauri/src/ssh.rs:56-63`

`check_server_key` always returns `Ok(true)`. Any network attacker can intercept SSH connections, capture credentials, and execute commands on behalf of the user.

**Fix:** Parse `~/.ssh/known_hosts`, reject unknown keys, and prompt the user on first connection (trust-on-first-use). Consider using the `russh-keys` crate's `known_hosts` module or `openssh`-based connections which inherit the system SSH config.

**Effort:** Medium-high. Requires UI for "trust this host?" prompt, persistence of accepted keys, and integration with the existing SSH connection flow.

---

### C3. `curl | bash` Upgrade Pattern — No Integrity Verification

**File:** `src-tauri/src/commands.rs:5700` (`run_openclaw_upgrade`) and `:5718` (`remote_run_openclaw_upgrade`)

The upgrade command pipes `https://openclaw.ai/install.sh` directly into bash with no checksum or GPG verification. DNS hijack or server compromise leads to RCE.

**Fix options:**
1. Download the script first, verify a SHA-256 checksum published at a separate URL, then execute
2. Use a signed binary release mechanism instead of a shell script
3. At minimum, pin to HTTPS and document the risk

**Effort:** Requires upstream (openclaw.ai) to publish checksums or signed releases.

---

## IMPORTANT — Should fix

### I2. Blocking I/O in Synchronous Tauri Commands

**File:** `src-tauri/src/commands.rs` — multiple functions

Several sync Tauri commands perform heavy blocking I/O (subprocess spawning, TCP connect, HTTP requests):
- `get_system_status()` — reads config, spawns subprocesses, checks update cache
- `get_status_light()` — TCP connect with 500ms timeout
- `list_model_catalog()` — spawns `openclaw` subprocess
- `check_openclaw_update()` — subprocess + HTTP request

**Fix:** Convert to `async` Tauri commands using `spawn_blocking` for the blocking operations, or wrap in `tauri::async_runtime::spawn_blocking`.

**Effort:** Medium. Each function needs signature change + async wrapping. Some already use `spawn_blocking` (e.g., `trigger_cron_job`), so the pattern exists.

---

### I17. Oversized Components — App.tsx (~560 lines), Doctor.tsx (~850 lines)

**App.tsx** manages routing, toasts, SSH state, config dirty/apply/discard, update checks, analytics, cron polling, navigation, chat panel, dialogs, and toast stack (14+ useState calls).

**Doctor.tsx** has 15+ state variables with deeply nested JSX. The session analysis section alone (lines 443-733) contains nested AlertDialogs with 10+ line inline handlers.

**Suggested extractions:**
- `App.tsx`: `useToast()` hook, `useConfigDirty()` hook, `<Sidebar>` component, `<ToastStack>` component
- `Doctor.tsx`: `<SessionAnalysisPanel>`, `<SessionRow>`, `<BackupsSection>`

**Effort:** Medium-high. Careful refactoring needed to avoid regressions.

---

### I16. AutocompleteField Missing ARIA Roles

**File:** `src/pages/Settings.tsx:57-126`

The custom autocomplete renders a dropdown using plain `<div>` elements with no ARIA roles (`role="listbox"`, `role="option"`), no `aria-expanded`, no `aria-activedescendant`, and no keyboard navigation (arrow keys).

**Fix:** Add proper ARIA attributes or replace with a library component (Radix Combobox, cmdk).

**Effort:** Medium. If replacing with a library component, straightforward. If adding ARIA manually, need to implement keyboard navigation too.

---

## SUGGESTIONS — Nice to have

### S1. Split `commands.rs` (~6150 lines) Into Modules

The file contains models, DTOs, local commands, remote commands, helpers, cron, watchdog, backup/restore, and session analysis.

**Suggested structure:**
```
commands/
  mod.rs          — re-exports
  models.rs       — DTOs/structs
  local.rs        — local instance commands
  remote.rs       — remote/SSH commands
  cron.rs         — cron + watchdog
  backup.rs       — backup/restore
  sessions.rs     — session management
  helpers.rs      — shared utilities
```

---

### S2. DRY up Local/Remote API Branching

Nearly every API call repeats:
```ts
if (isRemote) {
  if (!isConnected) return;
  api.remoteSomething(instanceId)...
} else {
  api.something()...
}
```

**Fix:** Create a `useApi()` hook or trait-based `ConfigBackend` abstraction that auto-selects local vs remote.

---

### S3. Duplicate `groupAgents` Function

Identical function in `Home.tsx` and `Channels.tsx`. Extract to `src/lib/agent-utils.ts`.

---

### S4. Regex Compiled on Every Call

`doctor.rs:74` and `commands.rs:2009` compile regexes on each invocation. Use `std::sync::LazyLock` or `lazy_static!`.

---

### S5. Config File Concurrent Access

Multiple Tauri commands read-modify-write the config file without locking. Two concurrent commands can lose each other's changes. Consider a Mutex around config operations.

---

### S6. `resolve_paths()` Side Effects

`src-tauri/src/models.rs:35-68` — the path resolution function contains filesystem migration logic. Should be a separate explicit startup step.

---

### S7. `state.ts` Naming

Only used by `Doctor.tsx`, but named `AppState`. Rename to `DoctorState` or inline into Doctor.tsx.

---

### S8. Accessibility — Nav Buttons, Status Dots

- Nav buttons in `App.tsx` missing `aria-current="page"` for active state
- Escalated cron badge and update dot (colored `<span>`) lack accessible labels
- Consider `<nav aria-label="Main navigation">` wrapper

---

### S9. Empty `.catch(() => {})` in Multiple Files

Cron.tsx data loading, App.tsx analytics — silently swallowed errors. At minimum log to console for debugging.

---

### S10. Chat.tsx Hardcoded `AGENT_ID = "main"`

If no agent named "main" exists, chat fails. After loading agents, validate that current `agentId` is in the list; if not, default to first available.
