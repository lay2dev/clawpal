# CLI-Based Config Refactoring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate ClawPal from direct openclaw.json read/write to using openclaw CLI commands, with a command queue for batched preview/apply/discard workflow.

**Architecture:** New `cli_runner.rs` module handles all openclaw CLI invocation (local via `Command::new`, remote via SSH exec). A `CommandQueue` (Tauri managed state) collects pending write operations. Preview uses `OPENCLAW_HOME` sandbox. Apply executes commands for real, with snapshot-based rollback on failure. Read operations migrate to CLI where structured output exists. Frontend gets a `PendingChangesBar` component and queue management APIs.

**Tech Stack:** Rust/Tauri (backend), React/TypeScript (frontend), openclaw CLI

---

### Task 1: Create cli_runner.rs — CLI Execution Primitives

**Files:**
- Create: `src-tauri/src/cli_runner.rs`
- Modify: `src-tauri/src/lib.rs` (add module declaration)

**Step 1: Create the cli_runner module with execution functions**

```rust
// src-tauri/src/cli_runner.rs

use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::models::resolve_paths;
use crate::ssh::SshConnectionPool;

// ---------------------------------------------------------------------------
// CLI execution primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Run an openclaw CLI command locally.
/// Returns CliOutput with stdout/stderr/exit_code.
pub fn run_openclaw(args: &[&str]) -> Result<CliOutput, String> {
    run_openclaw_with_env(args, None)
}

/// Run an openclaw CLI command locally with optional env overrides.
pub fn run_openclaw_with_env(
    args: &[&str],
    env: Option<&HashMap<String, String>>,
) -> Result<CliOutput, String> {
    let mut cmd = Command::new("openclaw");
    cmd.args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if let Some(env_vars) = env {
        for (k, v) in env_vars {
            cmd.env(k, v);
        }
    }

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run openclaw: {e}"))?;

    let exit_code = output.status.code().unwrap_or(-1);
    Ok(CliOutput {
        stdout: String::from_utf8_lossy(&output.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim_end().to_string(),
        exit_code,
    })
}

/// Run an openclaw CLI command on a remote host via SSH.
pub async fn run_openclaw_remote(
    pool: &SshConnectionPool,
    host_id: &str,
    args: &[&str],
) -> Result<CliOutput, String> {
    run_openclaw_remote_with_env(pool, host_id, args, None).await
}

/// Run an openclaw CLI command on a remote host with optional env prefix.
pub async fn run_openclaw_remote_with_env(
    pool: &SshConnectionPool,
    host_id: &str,
    args: &[&str],
    env: Option<&HashMap<String, String>>,
) -> Result<CliOutput, String> {
    let mut cmd_str = String::new();

    if let Some(env_vars) = env {
        for (k, v) in env_vars {
            cmd_str.push_str(&format!("{}='{}' ", k, v.replace('\'', "'\\''")));
        }
    }

    cmd_str.push_str("openclaw");
    for arg in args {
        cmd_str.push_str(&format!(" '{}'", arg.replace('\'', "'\\''")));
    }

    let result = pool.exec_login(host_id, &cmd_str).await?;
    Ok(CliOutput {
        stdout: result.stdout,
        stderr: result.stderr,
        exit_code: result.exit_code as i32,
    })
}

/// Strip leading non-JSON lines from CLI output (plugin logs, ANSI codes, etc.)
/// and parse as JSON Value.
pub fn parse_json_output(output: &CliOutput) -> Result<Value, String> {
    if output.exit_code != 0 {
        let details = if !output.stderr.is_empty() {
            &output.stderr
        } else {
            &output.stdout
        };
        return Err(format!("openclaw command failed ({}): {}", output.exit_code, details));
    }

    let raw = &output.stdout;
    let start = raw.find('{').or_else(|| raw.find('['))
        .ok_or_else(|| format!("No JSON found in output: {raw}"))?;
    let json_str = &raw[start..];
    serde_json::from_str(json_str).map_err(|e| format!("Failed to parse JSON: {e}"))
}
```

**Step 2: Add module declaration to lib.rs**

In `src-tauri/src/lib.rs`, add after `pub mod config_io;`:

```rust
pub mod cli_runner;
```

**Step 3: Add uuid dependency to Cargo.toml**

Check if `uuid` is already a dependency. If not, add:

```toml
uuid = { version = "1", features = ["v4"] }
```

**Step 4: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles (warnings about unused code are OK at this stage).

**Step 5: Commit**

```bash
git add src-tauri/src/cli_runner.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add cli_runner module with openclaw CLI execution primitives"
```

---

### Task 2: Add CommandQueue State and Data Structures

**Files:**
- Modify: `src-tauri/src/cli_runner.rs` (add queue types)
- Modify: `src-tauri/src/lib.rs` (register state)

**Step 1: Add CommandQueue types and state to cli_runner.rs**

Append to `src-tauri/src/cli_runner.rs`:

```rust
// ---------------------------------------------------------------------------
// Command Queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCommand {
    pub id: String,
    pub label: String,
    pub command: Vec<String>,
    pub created_at: String,
}

pub struct CommandQueue {
    commands: Mutex<Vec<PendingCommand>>,
}

impl CommandQueue {
    pub fn new() -> Self {
        Self {
            commands: Mutex::new(Vec::new()),
        }
    }

    pub fn enqueue(&self, label: String, command: Vec<String>) -> PendingCommand {
        let cmd = PendingCommand {
            id: Uuid::new_v4().to_string(),
            label,
            command,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.commands.lock().unwrap().push(cmd.clone());
        cmd
    }

    pub fn remove(&self, id: &str) -> bool {
        let mut cmds = self.commands.lock().unwrap();
        let before = cmds.len();
        cmds.retain(|c| c.id != id);
        cmds.len() < before
    }

    pub fn list(&self) -> Vec<PendingCommand> {
        self.commands.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.commands.lock().unwrap().clear();
    }

    pub fn is_empty(&self) -> bool {
        self.commands.lock().unwrap().is_empty()
    }

    pub fn len(&self) -> usize {
        self.commands.lock().unwrap().len()
    }
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Add chrono dependency if not present**

Check `src-tauri/Cargo.toml` for `chrono`. If not present, add:

```toml
chrono = { version = "0.4", features = ["serde"] }
```

**Step 3: Register CommandQueue as Tauri managed state**

In `src-tauri/src/lib.rs`, add import:

```rust
use crate::cli_runner::CommandQueue;
```

After `.manage(RemoteConfigBaselines::new())`, add:

```rust
.manage(CommandQueue::new())
```

**Step 4: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles cleanly.

**Step 5: Commit**

```bash
git add src-tauri/src/cli_runner.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add CommandQueue state for batched config operations"
```

---

### Task 3: Add Queue Management Tauri Commands

**Files:**
- Modify: `src-tauri/src/cli_runner.rs` (add Tauri commands)
- Modify: `src-tauri/src/lib.rs` (register commands)

**Step 1: Add Tauri command functions to cli_runner.rs**

Append to `src-tauri/src/cli_runner.rs`:

```rust
// ---------------------------------------------------------------------------
// Tauri Commands — Queue Management
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn queue_command(
    queue: tauri::State<CommandQueue>,
    label: String,
    command: Vec<String>,
) -> Result<PendingCommand, String> {
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    Ok(queue.enqueue(label, command))
}

#[tauri::command]
pub fn remove_queued_command(
    queue: tauri::State<CommandQueue>,
    id: String,
) -> Result<bool, String> {
    Ok(queue.remove(&id))
}

#[tauri::command]
pub fn list_queued_commands(
    queue: tauri::State<CommandQueue>,
) -> Result<Vec<PendingCommand>, String> {
    Ok(queue.list())
}

#[tauri::command]
pub fn discard_queued_commands(
    queue: tauri::State<CommandQueue>,
) -> Result<bool, String> {
    queue.clear();
    Ok(true)
}

#[tauri::command]
pub fn queued_commands_count(
    queue: tauri::State<CommandQueue>,
) -> Result<usize, String> {
    Ok(queue.len())
}
```

**Step 2: Register commands in lib.rs**

Add imports at top of `src-tauri/src/lib.rs`:

```rust
use crate::cli_runner::{
    CommandQueue,
    queue_command, remove_queued_command, list_queued_commands,
    discard_queued_commands, queued_commands_count,
};
```

Add to the `invoke_handler` list:

```rust
queue_command,
remove_queued_command,
list_queued_commands,
discard_queued_commands,
queued_commands_count,
```

**Step 3: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles cleanly.

**Step 4: Commit**

```bash
git add src-tauri/src/cli_runner.rs src-tauri/src/lib.rs
git commit -m "feat: add queue management Tauri commands"
```

---

### Task 4: Implement Preview Mechanism

**Files:**
- Modify: `src-tauri/src/cli_runner.rs` (add preview logic)
- Modify: `src-tauri/src/lib.rs` (register command)

**Step 1: Add preview Tauri command**

Append to `src-tauri/src/cli_runner.rs`:

```rust
// ---------------------------------------------------------------------------
// Preview — sandbox execution with OPENCLAW_HOME
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewQueueResult {
    pub commands: Vec<PendingCommand>,
    pub config_before: String,
    pub config_after: String,
    pub errors: Vec<String>,
}

#[tauri::command]
pub fn preview_queued_commands(
    queue: tauri::State<CommandQueue>,
) -> Result<PreviewQueueResult, String> {
    let commands = queue.list();
    if commands.is_empty() {
        return Err("No pending commands to preview".into());
    }

    let paths = resolve_paths();

    // Read current config
    let config_before = crate::config_io::read_text(&paths.config_path)?;

    // Set up sandbox directory
    let preview_dir = paths.clawpal_dir.join("preview").join(".openclaw");
    std::fs::create_dir_all(&preview_dir).map_err(|e| e.to_string())?;

    // Copy current config to sandbox
    let preview_config = preview_dir.join("openclaw.json");
    std::fs::copy(&paths.config_path, &preview_config).map_err(|e| e.to_string())?;

    let mut env = HashMap::new();
    env.insert(
        "OPENCLAW_HOME".to_string(),
        preview_dir.to_string_lossy().to_string(),
    );

    // Execute each command in sandbox
    let mut errors = Vec::new();
    for cmd in &commands {
        let args: Vec<&str> = cmd.command.iter().skip(1).map(|s| s.as_str()).collect();
        let result = run_openclaw_with_env(&args, Some(&env));
        match result {
            Ok(output) if output.exit_code != 0 => {
                let detail = if !output.stderr.is_empty() {
                    output.stderr.clone()
                } else {
                    output.stdout.clone()
                };
                errors.push(format!("{}: {}", cmd.label, detail));
                break;
            }
            Err(e) => {
                errors.push(format!("{}: {}", cmd.label, e));
                break;
            }
            _ => {}
        }
    }

    // Read result config from sandbox
    let config_after = if errors.is_empty() {
        crate::config_io::read_text(&preview_config)?
    } else {
        config_before.clone()
    };

    // Cleanup sandbox
    let _ = std::fs::remove_dir_all(paths.clawpal_dir.join("preview"));

    Ok(PreviewQueueResult {
        commands,
        config_before,
        config_after,
        errors,
    })
}
```

**Step 2: Register in lib.rs**

Add `preview_queued_commands` to imports and `invoke_handler`.

**Step 3: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles cleanly.

**Step 4: Commit**

```bash
git add src-tauri/src/cli_runner.rs src-tauri/src/lib.rs
git commit -m "feat: add preview mechanism using OPENCLAW_HOME sandbox"
```

---

### Task 5: Implement Apply with Snapshot Rollback

**Files:**
- Modify: `src-tauri/src/cli_runner.rs` (add apply logic)
- Modify: `src-tauri/src/lib.rs` (register command)

**Step 1: Add apply Tauri command**

Append to `src-tauri/src/cli_runner.rs`:

```rust
// ---------------------------------------------------------------------------
// Apply — execute queue for real, rollback on failure
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyQueueResult {
    pub ok: bool,
    pub applied_count: usize,
    pub total_count: usize,
    pub error: Option<String>,
    pub rolled_back: bool,
}

#[tauri::command]
pub fn apply_queued_commands(
    queue: tauri::State<CommandQueue>,
) -> Result<ApplyQueueResult, String> {
    let commands = queue.list();
    if commands.is_empty() {
        return Err("No pending commands to apply".into());
    }

    let paths = resolve_paths();
    let total_count = commands.len();

    // Save snapshot before applying (for rollback)
    let config_before = crate::config_io::read_text(&paths.config_path)?;
    let _ = crate::history::add_snapshot(
        &paths.history_dir,
        &paths.metadata_path,
        Some("pre-apply".to_string()),
        "queue-apply",
        true,
        &config_before,
        None,
    );

    // Execute each command for real
    let mut applied_count = 0;
    for cmd in &commands {
        let args: Vec<&str> = cmd.command.iter().skip(1).map(|s| s.as_str()).collect();
        let result = run_openclaw(&args);
        match result {
            Ok(output) if output.exit_code != 0 => {
                let detail = if !output.stderr.is_empty() {
                    output.stderr.clone()
                } else {
                    output.stdout.clone()
                };

                // Rollback: restore config from snapshot
                let _ = crate::config_io::write_text(&paths.config_path, &config_before);

                queue.clear();
                return Ok(ApplyQueueResult {
                    ok: false,
                    applied_count,
                    total_count,
                    error: Some(format!("Step {} failed ({}): {}", applied_count + 1, cmd.label, detail)),
                    rolled_back: true,
                });
            }
            Err(e) => {
                let _ = crate::config_io::write_text(&paths.config_path, &config_before);
                queue.clear();
                return Ok(ApplyQueueResult {
                    ok: false,
                    applied_count,
                    total_count,
                    error: Some(format!("Step {} failed ({}): {}", applied_count + 1, cmd.label, e)),
                    rolled_back: true,
                });
            }
            Ok(_) => {
                applied_count += 1;
            }
        }
    }

    // All succeeded — clear queue and restart gateway
    queue.clear();

    // Restart gateway (best effort, don't fail the whole apply)
    let gateway_result = run_openclaw(&["gateway", "restart"]);
    if let Err(e) = &gateway_result {
        eprintln!("Warning: gateway restart failed after apply: {e}");
    }

    Ok(ApplyQueueResult {
        ok: true,
        applied_count,
        total_count,
        error: None,
        rolled_back: false,
    })
}
```

**Step 2: Register in lib.rs**

Add `apply_queued_commands` to imports and `invoke_handler`.

**Step 3: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles cleanly.

**Step 4: Commit**

```bash
git add src-tauri/src/cli_runner.rs src-tauri/src/lib.rs
git commit -m "feat: add apply mechanism with snapshot rollback"
```

---

### Task 6: Implement Remote Queue (Preview + Apply via SSH)

**Files:**
- Modify: `src-tauri/src/cli_runner.rs` (add remote variants)
- Modify: `src-tauri/src/lib.rs` (register commands)

**Step 1: Add remote command queue state**

The remote queue needs to be per-host. Add to `cli_runner.rs`:

```rust
// ---------------------------------------------------------------------------
// Remote Command Queue — per-host queues
// ---------------------------------------------------------------------------

pub struct RemoteCommandQueues {
    queues: Mutex<HashMap<String, Vec<PendingCommand>>>,
}

impl RemoteCommandQueues {
    pub fn new() -> Self {
        Self {
            queues: Mutex::new(HashMap::new()),
        }
    }

    pub fn enqueue(&self, host_id: &str, label: String, command: Vec<String>) -> PendingCommand {
        let cmd = PendingCommand {
            id: Uuid::new_v4().to_string(),
            label,
            command,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.queues
            .lock()
            .unwrap()
            .entry(host_id.to_string())
            .or_default()
            .push(cmd.clone());
        cmd
    }

    pub fn remove(&self, host_id: &str, id: &str) -> bool {
        let mut queues = self.queues.lock().unwrap();
        if let Some(cmds) = queues.get_mut(host_id) {
            let before = cmds.len();
            cmds.retain(|c| c.id != id);
            return cmds.len() < before;
        }
        false
    }

    pub fn list(&self, host_id: &str) -> Vec<PendingCommand> {
        self.queues
            .lock()
            .unwrap()
            .get(host_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn clear(&self, host_id: &str) {
        self.queues.lock().unwrap().remove(host_id);
    }

    pub fn len(&self, host_id: &str) -> usize {
        self.queues
            .lock()
            .unwrap()
            .get(host_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

impl Default for RemoteCommandQueues {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Add remote Tauri commands**

```rust
#[tauri::command]
pub fn remote_queue_command(
    queues: tauri::State<RemoteCommandQueues>,
    host_id: String,
    label: String,
    command: Vec<String>,
) -> Result<PendingCommand, String> {
    if command.is_empty() {
        return Err("command cannot be empty".into());
    }
    Ok(queues.enqueue(&host_id, label, command))
}

#[tauri::command]
pub fn remote_remove_queued_command(
    queues: tauri::State<RemoteCommandQueues>,
    host_id: String,
    id: String,
) -> Result<bool, String> {
    Ok(queues.remove(&host_id, &id))
}

#[tauri::command]
pub fn remote_list_queued_commands(
    queues: tauri::State<RemoteCommandQueues>,
    host_id: String,
) -> Result<Vec<PendingCommand>, String> {
    Ok(queues.list(&host_id))
}

#[tauri::command]
pub fn remote_discard_queued_commands(
    queues: tauri::State<RemoteCommandQueues>,
    host_id: String,
) -> Result<bool, String> {
    queues.clear(&host_id);
    Ok(true)
}

#[tauri::command]
pub fn remote_queued_commands_count(
    queues: tauri::State<RemoteCommandQueues>,
    host_id: String,
) -> Result<usize, String> {
    Ok(queues.len(&host_id))
}

#[tauri::command]
pub async fn remote_preview_queued_commands(
    pool: tauri::State<'_, SshConnectionPool>,
    queues: tauri::State<'_, RemoteCommandQueues>,
    host_id: String,
) -> Result<PreviewQueueResult, String> {
    let commands = queues.list(&host_id);
    if commands.is_empty() {
        return Err("No pending commands to preview".into());
    }

    // Read current config via SSH
    let config_before = pool.sftp_read(&host_id, "~/.openclaw/openclaw.json").await?;

    // Set up sandbox on remote
    pool.exec(&host_id, "mkdir -p ~/.clawpal/preview/.openclaw").await?;
    pool.exec(&host_id, "cp ~/.openclaw/openclaw.json ~/.clawpal/preview/.openclaw/openclaw.json").await?;

    // Execute each command in sandbox
    let mut errors = Vec::new();
    for cmd in &commands {
        let args: Vec<&str> = cmd.command.iter().skip(1).map(|s| s.as_str()).collect();
        let mut env = HashMap::new();
        env.insert(
            "OPENCLAW_HOME".to_string(),
            "~/.clawpal/preview/.openclaw".to_string(),
        );

        match run_openclaw_remote_with_env(&pool, &host_id, &args, Some(&env)).await {
            Ok(output) if output.exit_code != 0 => {
                let detail = if !output.stderr.is_empty() {
                    output.stderr.clone()
                } else {
                    output.stdout.clone()
                };
                errors.push(format!("{}: {}", cmd.label, detail));
                break;
            }
            Err(e) => {
                errors.push(format!("{}: {}", cmd.label, e));
                break;
            }
            _ => {}
        }
    }

    // Read result config from sandbox
    let config_after = if errors.is_empty() {
        pool.sftp_read(&host_id, "~/.clawpal/preview/.openclaw/openclaw.json").await?
    } else {
        config_before.clone()
    };

    // Cleanup
    let _ = pool.exec(&host_id, "rm -rf ~/.clawpal/preview").await;

    Ok(PreviewQueueResult {
        commands,
        config_before,
        config_after,
        errors,
    })
}

#[tauri::command]
pub async fn remote_apply_queued_commands(
    pool: tauri::State<'_, SshConnectionPool>,
    queues: tauri::State<'_, RemoteCommandQueues>,
    host_id: String,
) -> Result<ApplyQueueResult, String> {
    let commands = queues.list(&host_id);
    if commands.is_empty() {
        return Err("No pending commands to apply".into());
    }
    let total_count = commands.len();

    // Save snapshot
    let config_before = pool.sftp_read(&host_id, "~/.openclaw/openclaw.json").await?;
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let snapshot_path = format!("~/.clawpal/snapshots/{ts}-queue-apply.json");
    let _ = pool.exec(&host_id, "mkdir -p ~/.clawpal/snapshots").await;
    let _ = pool.sftp_write(&host_id, &snapshot_path, &config_before).await;

    // Execute each command
    let mut applied_count = 0;
    for cmd in &commands {
        let args: Vec<&str> = cmd.command.iter().skip(1).map(|s| s.as_str()).collect();
        match run_openclaw_remote(&pool, &host_id, &args).await {
            Ok(output) if output.exit_code != 0 => {
                let detail = if !output.stderr.is_empty() {
                    output.stderr.clone()
                } else {
                    output.stdout.clone()
                };
                // Rollback
                let _ = pool.sftp_write(&host_id, "~/.openclaw/openclaw.json", &config_before).await;
                queues.clear(&host_id);
                return Ok(ApplyQueueResult {
                    ok: false,
                    applied_count,
                    total_count,
                    error: Some(format!("Step {} failed ({}): {}", applied_count + 1, cmd.label, detail)),
                    rolled_back: true,
                });
            }
            Err(e) => {
                let _ = pool.sftp_write(&host_id, "~/.openclaw/openclaw.json", &config_before).await;
                queues.clear(&host_id);
                return Ok(ApplyQueueResult {
                    ok: false,
                    applied_count,
                    total_count,
                    error: Some(format!("Step {} failed ({}): {}", applied_count + 1, cmd.label, e)),
                    rolled_back: true,
                });
            }
            Ok(_) => {
                applied_count += 1;
            }
        }
    }

    queues.clear(&host_id);

    // Restart gateway
    let _ = pool.exec_login(&host_id, "openclaw gateway restart").await;

    Ok(ApplyQueueResult {
        ok: true,
        applied_count,
        total_count,
        error: None,
        rolled_back: false,
    })
}
```

**Step 3: Register state and commands in lib.rs**

Add to imports:
```rust
use crate::cli_runner::RemoteCommandQueues;
```

Add state: `.manage(RemoteCommandQueues::new())`

Add all remote queue commands to `invoke_handler`.

**Step 4: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles cleanly.

**Step 5: Commit**

```bash
git add src-tauri/src/cli_runner.rs src-tauri/src/lib.rs
git commit -m "feat: add remote command queue with SSH-based preview and apply"
```

---

### Task 7: Add Frontend API Bindings for Queue

**Files:**
- Modify: `src/lib/types.ts` (add queue types)
- Modify: `src/lib/api.ts` (add queue API calls)
- Modify: `src/lib/use-api.ts` (add queue dispatch)

**Step 1: Add TypeScript types**

In `src/lib/types.ts`, add:

```typescript
export interface PendingCommand {
  id: string;
  label: string;
  command: string[];
  createdAt: string;
}

export interface PreviewQueueResult {
  commands: PendingCommand[];
  configBefore: string;
  configAfter: string;
  errors: string[];
}

export interface ApplyQueueResult {
  ok: boolean;
  appliedCount: number;
  totalCount: number;
  error: string | null;
  rolledBack: boolean;
}
```

**Step 2: Add API bindings**

In `src/lib/api.ts`, add to the `api` object:

```typescript
// Queue management
queueCommand: (label: string, command: string[]): Promise<PendingCommand> =>
  invoke("queue_command", { label, command }),
removeQueuedCommand: (id: string): Promise<boolean> =>
  invoke("remove_queued_command", { id }),
listQueuedCommands: (): Promise<PendingCommand[]> =>
  invoke("list_queued_commands", {}),
discardQueuedCommands: (): Promise<boolean> =>
  invoke("discard_queued_commands", {}),
previewQueuedCommands: (): Promise<PreviewQueueResult> =>
  invoke("preview_queued_commands", {}),
applyQueuedCommands: (): Promise<ApplyQueueResult> =>
  invoke("apply_queued_commands", {}),
queuedCommandsCount: (): Promise<number> =>
  invoke("queued_commands_count", {}),

// Remote queue management
remoteQueueCommand: (hostId: string, label: string, command: string[]): Promise<PendingCommand> =>
  invoke("remote_queue_command", { hostId, label, command }),
remoteRemoveQueuedCommand: (hostId: string, id: string): Promise<boolean> =>
  invoke("remote_remove_queued_command", { hostId, id }),
remoteListQueuedCommands: (hostId: string): Promise<PendingCommand[]> =>
  invoke("remote_list_queued_commands", { hostId }),
remoteDiscardQueuedCommands: (hostId: string): Promise<boolean> =>
  invoke("remote_discard_queued_commands", { hostId }),
remotePreviewQueuedCommands: (hostId: string): Promise<PreviewQueueResult> =>
  invoke("remote_preview_queued_commands", { hostId }),
remoteApplyQueuedCommands: (hostId: string): Promise<ApplyQueueResult> =>
  invoke("remote_apply_queued_commands", { hostId }),
remoteQueuedCommandsCount: (hostId: string): Promise<number> =>
  invoke("remote_queued_commands_count", { hostId }),
```

**Step 3: Add to use-api.ts dispatch**

In `src/lib/use-api.ts`, add to the `useMemo` return object:

```typescript
// Queue
queueCommand: dispatch(api.queueCommand, api.remoteQueueCommand),
removeQueuedCommand: dispatch(api.removeQueuedCommand, api.remoteRemoveQueuedCommand),
listQueuedCommands: dispatch(api.listQueuedCommands, api.remoteListQueuedCommands),
discardQueuedCommands: dispatch(api.discardQueuedCommands, api.remoteDiscardQueuedCommands),
previewQueuedCommands: dispatch(api.previewQueuedCommands, api.remotePreviewQueuedCommands),
applyQueuedCommands: dispatch(api.applyQueuedCommands, api.remoteApplyQueuedCommands),
queuedCommandsCount: dispatch(api.queuedCommandsCount, api.remoteQueuedCommandsCount),
```

**Step 4: Run frontend build**

Run: `cd /Users/zhixian/Codes/clawpal && npm run build 2>&1 | tail -10`
Expected: Builds cleanly (with unused variable warnings OK).

**Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/api.ts src/lib/use-api.ts
git commit -m "feat: add frontend API bindings for command queue"
```

---

### Task 8: Create PendingChangesBar Component

**Files:**
- Create: `src/components/PendingChangesBar.tsx`
- Modify: `src/App.tsx` (mount the component)

**Step 1: Create PendingChangesBar component**

Create `src/components/PendingChangesBar.tsx` — a bottom bar that appears when the queue is non-empty. Shows pending count, expand to see command list, and Preview/Apply/Discard buttons.

Key requirements:
- Poll `queuedCommandsCount()` on a short interval (2s) or use an event-based approach
- When expanded, show the list of pending commands with their labels
- Each command row has a delete button (calls `removeQueuedCommand`)
- Preview button opens a modal showing config diff (reuse existing diff display pattern)
- Apply button calls `applyQueuedCommands()` and shows result
- Discard button calls `discardQueuedCommands()` with confirmation

Look at existing components (e.g., the current dirty state bar in `App.tsx` or `Home.tsx`) for UI patterns and styling conventions. The new bar should replace the existing baseline/dirty state mechanism.

**Step 2: Mount in App.tsx**

Add `<PendingChangesBar />` in the app layout, positioned as a sticky bottom bar or notification area. It should be visible across all pages.

**Step 3: Run frontend build**

Run: `npm run build 2>&1 | tail -10`
Expected: Builds cleanly.

**Step 4: Commit**

```bash
git add src/components/PendingChangesBar.tsx src/App.tsx
git commit -m "feat: add PendingChangesBar component for queue management UI"
```

---

### Task 9: Migrate Write Operations — Agent Create/Delete

**Files:**
- Modify: `src/components/CreateAgentDialog.tsx` (use queue instead of direct create)
- Modify: `src/pages/Home.tsx` (use queue for delete)

**Step 1: Change CreateAgentDialog to enqueue instead of direct create**

In `CreateAgentDialog.tsx`, find where `createAgent()` is called. Replace with:

```typescript
// Before: await api.createAgent(agentId, modelValue, independent);
// After:
const command = ["openclaw", "agents", "add", agentId, "--non-interactive"];
if (modelValue) {
  command.push("--model", modelValue);
}
if (independent && workspace) {
  command.push("--workspace", workspace);
}
await api.queueCommand(`Create agent: ${agentId}`, command);
```

**Step 2: Change Home.tsx delete to enqueue**

Find where `deleteAgent()` is called. Replace with:

```typescript
// Before: await api.deleteAgent(agentId);
// After:
await api.queueCommand(
  `Delete agent: ${agentId}`,
  ["openclaw", "agents", "delete", agentId, "--force"]
);
```

**Step 3: Run frontend build**

Run: `npm run build 2>&1 | tail -10`
Expected: Builds cleanly.

**Step 4: Test manually**

Run: `npm run tauri dev`
- Create an agent → should appear in queue, not immediately in agent list
- Delete an agent → should appear in queue
- Preview → should show config diff
- Apply → should execute and refresh

**Step 5: Commit**

```bash
git add src/components/CreateAgentDialog.tsx src/pages/Home.tsx
git commit -m "refactor: migrate agent create/delete to command queue"
```

---

### Task 10: Migrate Write Operations — Model Settings

**Files:**
- Modify: `src/pages/Home.tsx` (model changes use queue)

**Step 1: Change setGlobalModel to enqueue**

Find where `setGlobalModel()` is called. Replace with:

```typescript
// Before: await api.setGlobalModel(modelValue);
// After:
if (modelValue) {
  await api.queueCommand(
    `Set global model: ${modelValue}`,
    ["openclaw", "config", "set", "agents.defaults.model.primary", modelValue]
  );
} else {
  await api.queueCommand(
    "Clear global model override",
    ["openclaw", "config", "unset", "agents.defaults.model.primary"]
  );
}
```

**Step 2: Change setAgentModel to enqueue**

Find where `setAgentModel()` is called. Replace with:

```typescript
// Before: await api.setAgentModel(agentId, modelValue);
// After:
if (modelValue) {
  await api.queueCommand(
    `Set model for ${agentId}: ${modelValue}`,
    ["openclaw", "config", "set", `agents.list[id=${agentId}].model`, modelValue]
  );
} else {
  await api.queueCommand(
    `Clear model override for ${agentId}`,
    ["openclaw", "config", "unset", `agents.list[id=${agentId}].model`]
  );
}
```

Note: The exact `config set` path for agent-specific model may need verification. Test with `openclaw config get agents --json` to confirm the path structure. If agents are in an array (`agents.list[]`), the path syntax may differ — check openclaw's dot-path support for array elements.

**Step 3: Run frontend build and test**

Run: `npm run build 2>&1 | tail -10`
Test model changes in dev mode to verify correct CLI paths.

**Step 4: Commit**

```bash
git add src/pages/Home.tsx
git commit -m "refactor: migrate model settings to command queue"
```

---

### Task 11: Migrate Write Operations — Channel Binding

**Files:**
- Modify: `src/pages/Channels.tsx` (channel binding uses queue)

**Step 1: Change assignChannelAgent to enqueue**

Find where `assignChannelAgent()` is called. Replace with the equivalent `openclaw config set` or `openclaw agents add --bind` command.

The exact CLI command depends on openclaw's binding mechanism. Test:
```bash
openclaw config get bindings --json
```

If bindings are a simple config path, use `config set`. If there's a dedicated CLI command, use that instead.

**Step 2: Run frontend build and test**

**Step 3: Commit**

```bash
git add src/pages/Channels.tsx
git commit -m "refactor: migrate channel binding to command queue"
```

---

### Task 12: Migrate Read Operations to CLI

**Files:**
- Modify: `src-tauri/src/commands.rs` (replace direct JSON reads with CLI calls)

**Step 1: Migrate list_agents_overview to use CLI**

Replace the current implementation that reads `openclaw.json` and manually parses agents with:

```rust
#[tauri::command]
pub fn list_agents_overview() -> Result<Vec<AgentOverview>, String> {
    let output = crate::cli_runner::run_openclaw(&["agents", "list", "--json"])?;
    let json = crate::cli_runner::parse_json_output(&output)?;
    // Parse the CLI JSON output into Vec<AgentOverview>
    // The CLI output format may differ from the current struct — map fields accordingly
    // ...
}
```

Note: The CLI output includes fields like `id`, `identityName`, `identityEmoji`, `workspace`, `model`, `bindings`, `isDefault`. Map these to the existing `AgentOverview` struct, adjusting field names as needed.

**Step 2: Migrate list_channels_minimal to use CLI**

```rust
#[tauri::command]
pub fn list_channels_minimal() -> Result<Vec<ChannelNode>, String> {
    let output = crate::cli_runner::run_openclaw(&["channels", "list", "--json"])?;
    let json = crate::cli_runner::parse_json_output(&output)?;
    // Parse into Vec<ChannelNode>
    // ...
}
```

**Step 3: Migrate list_bindings to use CLI**

```rust
#[tauri::command]
pub fn list_bindings() -> Result<Vec<Value>, String> {
    let output = crate::cli_runner::run_openclaw(&["config", "get", "bindings", "--json"])?;
    let json = crate::cli_runner::parse_json_output(&output)?;
    // ...
}
```

**Step 4: Migrate remote variants**

For each local read migrated above, update the remote variant to use `run_openclaw_remote` instead of `sftp_read` + JSON parsing:

```rust
#[tauri::command]
pub async fn remote_list_agents_overview(
    pool: tauri::State<'_, SshConnectionPool>,
    host_id: String,
) -> Result<Vec<AgentOverview>, String> {
    let output = crate::cli_runner::run_openclaw_remote(&pool, &host_id, &["agents", "list", "--json"]).await?;
    let json = crate::cli_runner::parse_json_output(&output)?;
    // Same parsing logic as local
}
```

**Step 5: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles cleanly.

**Step 6: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "refactor: migrate read operations to openclaw CLI"
```

---

### Task 13: Add Read Cache

**Files:**
- Modify: `src-tauri/src/cli_runner.rs` (add cache layer)

**Step 1: Add a simple cache struct**

```rust
// ---------------------------------------------------------------------------
// Read Cache — invalidated on Apply
// ---------------------------------------------------------------------------

pub struct CliCache {
    cache: Mutex<HashMap<String, (std::time::Instant, String)>>,
}

impl CliCache {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Get cached value if still valid.
    /// key: a unique identifier for the query (e.g., "local:agents-list" or "remote:host1:agents-list")
    /// ttl: None means cache forever (until invalidated), Some(duration) for TTL-based
    pub fn get(&self, key: &str, ttl: Option<std::time::Duration>) -> Option<String> {
        let cache = self.cache.lock().unwrap();
        cache.get(key).and_then(|(ts, val)| {
            if let Some(ttl) = ttl {
                if ts.elapsed() < ttl {
                    Some(val.clone())
                } else {
                    None
                }
            } else {
                Some(val.clone())
            }
        })
    }

    pub fn set(&self, key: String, value: String) {
        self.cache
            .lock()
            .unwrap()
            .insert(key, (std::time::Instant::now(), value));
    }

    /// Invalidate all cache entries (called after Apply).
    pub fn invalidate_all(&self) {
        self.cache.lock().unwrap().clear();
    }

    /// Invalidate entries matching a prefix (e.g., "remote:host1:").
    pub fn invalidate_prefix(&self, prefix: &str) {
        self.cache
            .lock()
            .unwrap()
            .retain(|k, _| !k.starts_with(prefix));
    }
}

impl Default for CliCache {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: Register as Tauri state**

In `lib.rs`:
```rust
use crate::cli_runner::CliCache;
// ...
.manage(CliCache::new())
```

**Step 3: Add cache invalidation to apply_queued_commands**

In `apply_queued_commands`, after successful apply, add:

```rust
// Invalidate all cached reads
if let Ok(cache) = cache_state.try_get::<CliCache>() {
    cache.invalidate_all();
}
```

(Pass `CliCache` as a Tauri state parameter to the apply commands.)

**Step 4: Use cache in read operations**

In `list_agents_overview` and similar read commands, wrap the CLI call with cache:

```rust
let cache_key = "local:agents-list";
if let Some(cached) = cache.get(cache_key, None) {
    return serde_json::from_str(&cached).map_err(|e| e.to_string());
}
let output = run_openclaw(&["agents", "list", "--json"])?;
// ... parse ...
cache.set(cache_key.to_string(), serde_json::to_string(&result).unwrap());
```

For model catalog, use TTL: `cache.get("local:model-catalog", Some(Duration::from_secs(600)))`

**Step 5: Run cargo check**

**Step 6: Commit**

```bash
git add src-tauri/src/cli_runner.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: add read cache with apply-based invalidation"
```

---

### Task 14: Remove Old Baseline/Dirty Mechanism

**Files:**
- Modify: `src-tauri/src/commands.rs` (remove old baseline functions)
- Modify: `src-tauri/src/lib.rs` (unregister old commands)
- Modify: `src/lib/api.ts` (remove old API bindings)
- Modify: `src/lib/use-api.ts` (remove old dispatch entries)

**Step 1: Remove from commands.rs**

Remove or deprecate these functions:
- `save_config_baseline` (line ~3661)
- `check_config_dirty` (line ~3680)
- `discard_config_changes` (line ~3698)
- `apply_pending_changes` (line ~3725)
- Their `remote_` counterparts
- `RemoteConfigBaselines` struct and its usage
- `write_config_with_snapshot` helper (line ~2853) — only if no longer used
- `set_nested_value` helper (line ~2871) — only if no longer used

Also remove the direct-write commands that are now replaced by queue:
- `set_global_model`, `set_agent_model`
- `assign_channel_agent`, `update_channel_config`, `delete_channel_node`
- `create_agent`, `delete_agent` (the direct-write versions)
- Their `remote_` counterparts

**Step 2: Remove from lib.rs**

Remove all unregistered commands from imports and `invoke_handler`.
Remove `.manage(RemoteConfigBaselines::new())`.

**Step 3: Remove from api.ts and use-api.ts**

Remove the old API bindings and dispatch entries for removed commands.

**Step 4: Fix any frontend references**

Search for any remaining references to removed APIs and update them to use the queue equivalents.

Run: `npm run build 2>&1 | tail -20` — fix any TypeScript errors.

**Step 5: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: remove old baseline/dirty mechanism and direct-write commands

Replaced by command queue with preview/apply/discard workflow."
```

---

### Task 15: Migrate Recipe Application to Command Queue

**Files:**
- Modify: `src-tauri/src/commands.rs` or `src-tauri/src/cli_runner.rs`
- Modify: `src/pages/Cook.tsx` or equivalent recipe UI

**Step 1: Create a recipe-to-commands decomposition function**

The current `apply_config_patch` takes a JSON patch template and applies it as a whole. The new approach decomposes it into individual CLI commands.

```rust
/// Decompose a recipe patch into a sequence of CLI commands.
pub fn decompose_recipe_patch(
    patch_template: &str,
    params: &serde_json::Map<String, Value>,
) -> Result<Vec<(String, Vec<String>)>, String> {
    // 1. Render the template with params
    // 2. Walk the resulting JSON object
    // 3. For each top-level change, generate the appropriate CLI command:
    //    - agents.list additions → openclaw agents add ...
    //    - agents.defaults.model → openclaw config set ...
    //    - channels.* → openclaw channels add ... or config set
    //    - bindings → openclaw config set bindings ...
    // 4. Return Vec of (label, command) tuples
    todo!("Implement based on recipe template structure")
}
```

Note: This is the most complex part of the migration. The exact implementation depends on how recipe templates are structured. Read `src-tauri/src/recipe.rs` and example recipes to understand the patch format.

An alternative simpler approach: use `openclaw config set` for each top-level key in the patch, passing the JSON value. This avoids needing to know the semantics of each key.

**Step 2: Update the frontend recipe apply flow**

Instead of calling `applyConfigPatch`, the frontend should:
1. Call the decomposition function to get the command list
2. Enqueue all commands
3. Let the user preview and apply via the standard queue flow

**Step 3: Test with existing recipes**

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor: decompose recipe application into CLI command queue"
```

---

### Task 16: Slim Down config_io.rs

**Files:**
- Modify: `src-tauri/src/config_io.rs`

**Step 1: Remove write functions no longer needed**

After all write operations go through CLI, `write_json` and `write_text` for openclaw.json are only needed by:
- Snapshot/history system (still writes to `~/.clawpal/`)
- Rollback (still needs to write config for recovery)

Keep `write_text` and `write_json` but add a comment that they're only used for snapshots and rollback recovery, not for normal config operations.

Remove `read_openclaw_config` if all reads now go through CLI. Keep `read_json` for snapshot reads.

**Step 2: Run cargo check**

**Step 3: Commit**

```bash
git add src-tauri/src/config_io.rs
git commit -m "refactor: slim down config_io.rs — writes only for snapshots/rollback"
```

---

### Task 17: Full Build Verification

**Step 1: Full cargo build**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: Compiles with no errors.

**Step 2: Frontend build**

Run: `npm run build 2>&1 | tail -10`
Expected: Compiles with no errors.

**Step 3: Manual testing checklist**

Run: `npm run tauri dev`

- [ ] App launches
- [ ] Agent list loads (via CLI)
- [ ] Channel list loads (via CLI)
- [ ] Create agent → appears in queue
- [ ] Delete agent → appears in queue
- [ ] Change model → appears in queue
- [ ] PendingChangesBar shows correct count
- [ ] Preview shows config diff
- [ ] Apply executes and clears queue
- [ ] Gateway restarts after apply
- [ ] Discard clears queue
- [ ] Rollback works when a command fails
- [ ] Remote: all above work via SSH
- [ ] Recipe apply works via queue

**Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: resolve issues found during integration testing"
```

---

## Files Summary

| File | Action | Description |
|------|--------|-------------|
| `src-tauri/src/cli_runner.rs` | Create | CLI execution, CommandQueue, RemoteCommandQueues, CliCache, preview/apply logic |
| `src-tauri/src/lib.rs` | Modify | Register new module, states, and commands |
| `src-tauri/src/commands.rs` | Modify | Remove direct-write functions, migrate reads to CLI |
| `src-tauri/src/config_io.rs` | Modify | Slim down — keep only snapshot/rollback helpers |
| `src-tauri/Cargo.toml` | Modify | Add uuid, chrono if not present |
| `src/lib/types.ts` | Modify | Add PendingCommand, PreviewQueueResult, ApplyQueueResult |
| `src/lib/api.ts` | Modify | Add queue APIs, remove old direct-write APIs |
| `src/lib/use-api.ts` | Modify | Add queue dispatch, remove old dispatch entries |
| `src/components/PendingChangesBar.tsx` | Create | Queue management UI component |
| `src/App.tsx` | Modify | Mount PendingChangesBar |
| `src/components/CreateAgentDialog.tsx` | Modify | Use queue instead of direct create |
| `src/pages/Home.tsx` | Modify | Use queue for agent/model operations |
| `src/pages/Channels.tsx` | Modify | Use queue for channel binding |
| `src/pages/Cook.tsx` | Modify | Recipe decomposition into queue |

## What Does NOT Change

- `src-tauri/src/ssh.rs` — transport layer unchanged
- `src-tauri/src/history.rs` — snapshot mechanism reused as-is
- `src-tauri/src/doctor.rs` — already uses CLI
- `src-tauri/src/recipe.rs` — template rendering reused, only apply method changes

## Risk Mitigation

- **CLI output format changes**: openclaw CLI output is not a stable API. Pin to known-good output parsing, add fallback to direct JSON read if CLI fails.
- **Performance**: CLI spawn overhead (~50ms per call) mitigated by cache. Batch reads where possible.
- **Partial apply failure**: Snapshot-based rollback restores config. Filesystem side effects (agent dirs) are benign and reusable.
- **Recipe decomposition complexity**: Start with `config set` for each top-level key as a simple approach. Refine to use high-level CLI commands (agents add, channels add) iteratively.
