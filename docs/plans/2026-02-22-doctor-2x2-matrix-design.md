# Doctor Agent 2x2 Matrix Design

Date: 2026-02-22

## Problem

The current doctor agent only supports one scenario: a remote gateway diagnosing the local machine. In practice, users need to diagnose remote instances too — e.g., "my remote openclaw is broken, help me fix it from here." The local-fixes-local case is also rare since if the local instance is broken, the gateway might not be running either.

## Core Architecture

Decouple **Agent Source** (where the AI agent runs) from **Execution Target** (where commands execute).

```
Agent Source (Gateway)           Execution Target
┌──────────────────┐            ┌──────────────────┐
│ Local Gateway    │            │ Local machine    │
│ Hosted Service   │ ──────>   │ Remote SSH host  │
│ (SSH Gateway*)   │            │                  │
└──────────────────┘            └──────────────────┘
        WebSocket                  local exec / SSH exec
```

*SSH Gateway deferred to future — requires SSH port forwarding to reach a remote gateway's WebSocket.

### Supported Scenarios (Priority Order)

| # | Agent Source | Target | Use Case |
|---|-------------|--------|----------|
| 1 | Hosted Service | Remote SSH | "Fix my remote server from the cloud" |
| 2 | Local Gateway | Remote SSH | "My local openclaw diagnoses my remote server" |
| 3 | Hosted Service | Local | "Cloud agent fixes my local machine" |
| 4 | Local Gateway | Local | "Local agent fixes local" (rare) |

SSH Remote Gateway deferred — complex (port forwarding) and lower priority.

### Key Principle

The gateway doesn't know or care whether commands execute locally or via SSH. ClawPal is the intermediary that routes `node.invoke` to either local exec or SSH exec based on the user's target selection.

## Target Selection

Auto-infer from the active instance tab:

- **Local tab active** → target = local
- **Remote tab active** → target = that SSH host
- **"Change target"** link lets user override manually (dropdown of local + all configured SSH hosts)

This avoids a separate target selector cluttering the UI while keeping full control available.

## Unified Security Model

Security policy is **identical** for local and remote execution. The same rules apply regardless of where commands run.

### Sensitive Path Blacklist (Hard Block)

These paths are **always blocked** for both read and write, on both local and remote targets. No approval can override this.

```
~/.ssh/
~/.gnupg/
~/.aws/
~/.config/gcloud/
~/.azure/
~/.kube/config
~/.docker/config.json
~/.netrc
~/.npmrc (when it contains auth tokens)
~/.env
~/.bash_history
~/.zsh_history
/etc/shadow
/etc/sudoers
```

Implementation: `SENSITIVE_PATH_PATTERNS` array checked via `starts_with` after tilde expansion, before any other validation.

### Read Commands

- **First invocation of each command**: show in chat, user clicks to approve
- **Subsequent invocations of same command pattern**: auto-execute silently
- Track approved patterns in a `Set<string>` (e.g., `"read_file:/etc/openclaw/*"`)
- Pattern matching: command + path prefix, not exact path

### Write Commands

- **Always require explicit user approval** (Execute / Skip buttons)
- No auto-execute, no pattern-based approval
- Applies to: `write_file`, `run_command`

### Command Validation (Unified)

Same `ALLOWED_COMMAND_PREFIXES` and `DANGEROUS_PATTERNS` for both local and remote:

```rust
const ALLOWED_COMMAND_PREFIXES: &[&str] = &[
    "openclaw ", "cat ", "ls ", "head ", "tail ", "wc ",
    "grep ", "find ", "systemctl status", "journalctl ",
    "ps ", "which ", "echo ", "date", "uname", "hostname",
    "df ", "free ", "uptime",
];

const DANGEROUS_PATTERNS: &[&str] = &[
    ";", "|", "&&", "||", "`", "$(", ">", "<", "\n", "\r",
];
```

## Remote Execution Path

### New Function: `execute_remote_command`

Parallel to `execute_local_command`, routes to SSH:

```rust
async fn execute_remote_command(
    pool: &SshConnectionPool,
    host_id: &str,
    command: &str,
    args: &Value,
) -> Result<Value, String>
```

Command mapping:

| Command | Local | Remote |
|---------|-------|--------|
| `read_file` | `tokio::fs::read_to_string` | `pool.sftp_read(host_id, path)` |
| `list_files` | `tokio::fs::read_dir` | `pool.sftp_ls(host_id, path)` |
| `write_file` | `tokio::fs::write` | `pool.sftp_write(host_id, path, content)` |
| `run_command` | `sh -c cmd` | `pool.exec(host_id, cmd)` |
| `read_config` | local fs | `pool.sftp_read(host_id, remote_config_path)` |
| `system_info` | local APIs | `pool.exec(host_id, "openclaw --version && uname -a")` |
| `validate_config` | `run_doctor()` | `pool.exec(host_id, "openclaw doctor --json")` |

### New Command: `collect_doctor_context_remote`

Single Tauri command that SSHes into the target and collects all diagnostic context in one call:

```rust
#[tauri::command]
async fn collect_doctor_context_remote(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<String, String>
```

Executes via SSH:
- `openclaw --version`
- `cat /path/to/openclaw.json` (infer path from `openclaw doctor --json` output or standard locations)
- `openclaw doctor --json`
- `cat /path/to/error.log` (tail 100 lines)
- `uname -a`, `hostname`

Returns the same JSON shape as `collect_doctor_context` so the gateway sees identical context regardless of target.

### Security: Same Rules Apply

- `validate_sensitive_path()` runs on both local and remote paths
- `validate_command()` uses same prefix whitelist and dangerous pattern check
- Remote `write_file` also checks for symlinks where possible (`stat` via SSH before write)

## Tauri Command Layer Changes

### Modified Commands

```rust
// Add target parameter to approve
#[tauri::command]
async fn doctor_approve_invoke(
    client: State<'_, NodeClient>,
    pool: State<'_, SshConnectionPool>,
    app: AppHandle,
    invoke_id: String,
    target: String,        // "local" or host_id
) -> Result<Value, String>
```

Dispatch logic:
```rust
let result = if target == "local" {
    execute_local_command(command, &args).await?
} else {
    execute_remote_command(&pool, &target, command, &args).await?
};
```

### New Commands

```rust
#[tauri::command]
async fn collect_doctor_context_remote(
    pool: State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<String, String>
```

### Removed

- `doctor_ssh_forward` / `doctor_ssh_forward_close` — deferred, remove stubs

## Frontend Changes

### Target Display

In the Doctor page, below the agent source selector:

```
Target: Remote — server-1 (192.168.1.100)  [Change]
```

Auto-inferred from active tab. "Change" opens a dropdown with:
- Local machine
- All configured SSH hosts from settings

### Approval UX

- First-time read: tool call card appears with "Allow" button
- After first approval, subsequent reads with matching pattern auto-execute
- Track `approvedPatterns: Set<string>` in the `useDoctorAgent` hook
- Pattern format: `"command:pathPrefix"` (e.g., `"read_file:/etc/openclaw/"`)
- Writes always show Execute/Skip buttons (unchanged)

### API Signature Changes

```typescript
// api.ts
doctorApproveInvoke: (invokeId: string, target: string) => invoke(...)
collectDoctorContextRemote: (hostId: string) => invoke<string>(...)
```

### Hook Changes (`useDoctorAgent`)

```typescript
// New state
const [target, setTarget] = useState<string>("local");
const [approvedPatterns, setApprovedPatterns] = useState<Set<string>>(new Set());

// In doctor:invoke handler
if (invoke.type === "read") {
    const pattern = `${invoke.command}:${extractPathPrefix(invoke.args)}`;
    if (approvedPatterns.has(pattern)) {
        autoApprove(invoke.id);  // silent
    } else {
        // show in chat, wait for user click
    }
}
```

### Start Diagnosis Flow

```typescript
const handleStart = async () => {
    const context = target === "local"
        ? await api.collectDoctorContext()
        : await api.collectDoctorContextRemote(target);
    await connect(gatewayUrl);
    await startDiagnosis(context);
};
```

## Implementation Order

1. Sensitive path blacklist (shared security, both local and remote)
2. `execute_remote_command` in `doctor_commands.rs`
3. `collect_doctor_context_remote` Tauri command
4. Modify `doctor_approve_invoke` to accept target parameter
5. Remove SSH forward stubs
6. Frontend: target selector with auto-infer
7. Frontend: first-time approval pattern tracking in hook
8. Frontend: API signature updates
9. i18n updates
10. Integration test

## Key Files

| File | Action |
|---|---|
| `src-tauri/src/doctor_commands.rs` | Add remote exec, sensitive path blacklist, modify approve |
| `src-tauri/src/lib.rs` | Register new command, remove old stubs |
| `src/lib/use-doctor-agent.ts` | Add target state, approval patterns |
| `src/lib/api.ts` | Update signatures, add new API |
| `src/lib/use-api.ts` | Update signatures |
| `src/pages/Doctor.tsx` | Target selector UI, flow changes |
| `src/components/DoctorChat.tsx` | First-time approval UX |
| `src/lib/types.ts` | Minor type updates |
| `src/locales/en.json` | New translation keys |
| `src/locales/zh.json` | New translation keys |
