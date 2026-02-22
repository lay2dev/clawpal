# Doctor Agent — Remote Service API Design

Date: 2026-02-22

## Overview

Design the communication protocol between ClawPal and remote doctor services. Based on research, we adopt the **openclaw node protocol** — ClawPal connects as a node client to a doctor gateway via WebSocket.

## Key Insight

OpenClaw already has a complete node protocol (`clawgo` reference implementation in Go, ~1400 lines). The gateway can invoke tools on remote nodes via `node.invoke` RPC. This is exactly what we need:

- Doctor agent runs on a remote openclaw gateway
- ClawPal connects as a node
- Agent's tool calls arrive as `node.invoke` RPCs
- ClawPal executes locally (with user confirmation) and returns results

## Wire Protocol

Protocol version: **3**. Transport: JSON over WebSocket.

### Frame Types

Three frame types, discriminated by `type` field:

```json
// Request (either direction)
{ "type": "req", "id": "<uuid>", "method": "<name>", "params": { ... } }

// Response (matches a request by id)
{ "type": "res", "id": "<uuid>", "ok": true, "payload": { ... } }
{ "type": "res", "id": "<uuid>", "ok": false, "error": { "code": "...", "message": "..." } }

// Event (fire-and-forget, either direction)
{ "type": "event", "event": "<name>", "payload": { ... } }
```

Plus `ping`/`pong` for keepalive (every 30s).

## Connection Lifecycle

### 1. Connect

WebSocket to gateway URL (e.g. `wss://doctor.openclaw.ai:18789`).

### 2. Pair (first time only)

```json
→ { "type": "pair-request",
    "nodeId": "clawpal-<uuid>",
    "displayName": "ClawPal on <hostname>",
    "platform": "macos|windows|linux",
    "version": "<clawpal-version>",
    "deviceFamily": "desktop",
    "caps": [],
    "commands": ["read_file", "write_file", "run_command", "list_files",
                 "read_config", "validate_config", "system_info"],
    "permissions": {} }

← { "type": "pair-ok", "token": "<auth-token>" }
```

Save `nodeId` + `token` to ClawPal settings.

### 3. Hello (every reconnect)

```json
→ { "type": "hello",
    "nodeId": "clawpal-<uuid>",
    "token": "<saved-token>",
    "displayName": "ClawPal on <hostname>",
    "platform": "macos",
    "version": "<clawpal-version>",
    "deviceFamily": "desktop",
    "caps": [],
    "commands": ["read_file", "write_file", "run_command", "list_files",
                 "read_config", "validate_config", "system_info"],
    "permissions": {} }

← { "type": "hello-ok", "serverName": "Doctor Gateway" }
```

### 4. Start Doctor Session

ClawPal sends diagnostic context as an agent request:

```json
→ { "type": "req",
    "id": "<uuid>",
    "method": "agent",
    "params": {
      "idempotencyKey": "<uuid>",
      "agentId": "doctor",
      "message": "<doctor-context-json>",
      "sessionId": "<optional-existing-session>"
    } }
```

The `message` contains the auto-collected diagnostic context:

```json
{
  "type": "doctor_diagnosis_request",
  "target": {
    "instanceType": "local|ssh",
    "instanceId": "<id>",
    "hostname": "<hostname>"
  },
  "context": {
    "openclawVersion": "2026.2.21-2 | unknown | not_installed",
    "openclawPath": "/usr/local/bin/openclaw | null",
    "configContent": "<openclaw.json content or error>",
    "doctorReport": { "score": 85, "issues": [...] },
    "errorLog": "<last 50 lines of error log>",
    "systemInfo": {
      "os": "macOS 15.2",
      "arch": "aarch64",
      "shell": "zsh",
      "path": "/usr/local/bin:...",
      "homeDir": "/Users/xxx"
    }
  }
}
```

### 5. Receive Tool Calls (node.invoke)

Gateway sends tool invocations as the doctor agent works:

```json
← { "type": "req",
    "id": "invoke-001",
    "method": "node.invoke",
    "params": {
      "command": "read_file",
      "args": {
        "path": "~/.openclaw/logs/error.log",
        "maxLines": 100
      },
      "invokeId": "<uuid>"
    } }
```

### 6. Execute & Return Results

ClawPal shows the tool call to user. After auto-execute (reads) or user confirmation (writes):

```json
→ { "type": "res",
    "id": "invoke-001",
    "ok": true,
    "payload": {
      "output": "<file contents or command output>",
      "exitCode": 0
    } }
```

Or if user rejects:

```json
→ { "type": "res",
    "id": "invoke-001",
    "ok": false,
    "error": {
      "code": "USER_REJECTED",
      "message": "User declined to execute this operation"
    } }
```

### 7. Receive Agent Replies (streaming)

Agent text responses stream as chat events:

```json
← { "type": "event",
    "event": "chat",
    "payload": {
      "runId": "<uuid>",
      "sessionKey": "doctor",
      "state": "delta|final",
      "message": {
        "role": "assistant",
        "content": [{ "type": "text", "text": "I found the issue..." }]
      }
    } }
```

`state: "final"` indicates the agent turn is complete. If more tool calls follow, the cycle repeats from step 5.

### 8. Follow-up Messages

User can send additional messages in the same session:

```json
→ { "type": "req",
    "id": "<uuid>",
    "method": "agent",
    "params": {
      "idempotencyKey": "<uuid>",
      "agentId": "doctor",
      "message": "Can you also check the gateway logs?",
      "sessionId": "<session-id-from-previous-response>"
    } }
```

## Tool Definitions

Commands that ClawPal advertises in `hello.commands` and executes locally:

### Read Operations (auto-execute, no confirmation)

| Command | Args | Description |
|---|---|---|
| `read_file` | `{ path, maxLines? }` | Read file contents |
| `list_files` | `{ path, pattern? }` | List directory contents |
| `read_config` | `{}` | Read openclaw.json |
| `system_info` | `{}` | OS, PATH, openclaw version, etc. |
| `validate_config` | `{}` | Run doctor static checks |

### Write Operations (require user confirmation)

| Command | Args | Description |
|---|---|---|
| `write_file` | `{ path, content }` | Write/overwrite a file |
| `run_command` | `{ command, args?, cwd? }` | Execute a shell command |

### Tool → Tauri Command Mapping

All tools map to existing ClawPal capabilities:

| Tool | Local | SSH Remote |
|---|---|---|
| `read_file` | `std::fs::read_to_string` | `sftp_read` |
| `write_file` | `std::fs::write` | `sftp_write` |
| `run_command` | `Command::new` | `ssh_exec` / `exec_login` |
| `list_files` | `std::fs::read_dir` | `sftp_list` |
| `read_config` | `read_raw_config` | `remote_read_raw_config` |
| `validate_config` | `run_doctor_command` | `remote_run_doctor_command` |
| `system_info` | `get_system_status` | `remote_get_system_status` |

## Rust Implementation

### New module: `src-tauri/src/node_client.rs`

```rust
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub struct NodeClient {
    node_id: String,
    token: Option<String>,
    ws_tx: mpsc::Sender<Message>,
    // Pending invoke requests awaiting user confirmation
    pending_invokes: HashMap<String, InvokeRequest>,
}

pub struct InvokeRequest {
    pub id: String,
    pub command: String,
    pub args: Value,
    pub invoke_id: String,
}

impl NodeClient {
    pub async fn connect(url: &str) -> Result<Self, String>;
    pub async fn pair(&mut self, display_name: &str) -> Result<(), String>;
    pub async fn hello(&mut self) -> Result<(), String>;
    pub async fn start_doctor_session(&self, context: DoctorContext) -> Result<(), String>;
    pub async fn send_message(&self, message: &str, session_id: &str) -> Result<(), String>;
    pub async fn respond_invoke(&self, id: &str, result: InvokeResult) -> Result<(), String>;
    pub async fn reject_invoke(&self, id: &str, reason: &str) -> Result<(), String>;
}
```

### New Tauri Commands

```rust
// Connect to a doctor gateway as a node
#[tauri::command]
async fn doctor_connect(url: String) -> Result<(), String>;

// Disconnect from doctor gateway
#[tauri::command]
async fn doctor_disconnect() -> Result<(), String>;

// Start a doctor diagnosis session
#[tauri::command]
async fn doctor_start(context: DoctorContext) -> Result<(), String>;

// Send a follow-up message
#[tauri::command]
async fn doctor_send(message: String) -> Result<(), String>;

// Approve a pending tool invocation
#[tauri::command]
async fn doctor_approve_invoke(invoke_id: String) -> Result<(), String>;

// Reject a pending tool invocation
#[tauri::command]
async fn doctor_reject_invoke(invoke_id: String, reason: String) -> Result<(), String>;

// Collect diagnostic context (already in previous design)
#[tauri::command]
fn collect_doctor_context(instance_id: String) -> Result<DoctorContext, String>;
```

### Frontend Events (Tauri → React)

The node client emits Tauri events that React listens to:

| Event | Payload | Description |
|---|---|---|
| `doctor:connected` | `{}` | WebSocket connected and hello-ok received |
| `doctor:disconnected` | `{ reason }` | Connection lost |
| `doctor:chat-delta` | `{ text }` | Streaming agent text |
| `doctor:chat-final` | `{ text, sessionId }` | Agent turn complete |
| `doctor:invoke` | `{ invokeId, command, args }` | Tool call received, awaiting decision |
| `doctor:invoke-result` | `{ invokeId, output }` | Tool executed, result sent back |
| `doctor:error` | `{ message }` | Error from gateway |

## Agent Source Unification

All three agent sources now use the same node protocol:

| Source | Gateway URL | Notes |
|---|---|---|
| Local openclaw | `ws://localhost:18789` | Connect to local gateway as node |
| SSH remote openclaw | `ws://<ssh-host>:18789` (via SSH tunnel) | Forward port, then connect |
| Remote doctor service | `wss://doctor.openclaw.ai` | Connect to hosted gateway |

This means **one implementation** covers all sources. The only difference is the WebSocket URL.

For SSH remote: ClawPal creates an SSH port forward (`localhost:random → remote:18789`) then connects to `ws://localhost:random`.

## Security

- First-time pairing requires approval on the gateway side (or auto-approve for the hosted doctor service)
- Token stored in ClawPal settings, per gateway URL
- Write operations always require user confirmation in ClawPal UI
- No auth needed for MVP (internal testing phase)
- Future: API key or openclaw account binding

## Error Handling

| Scenario | Behavior |
|---|---|
| Gateway unreachable | Show error, offer retry |
| Connection dropped mid-session | Auto-reconnect with backoff (1s, 2s, 4s... max 30s), resume session |
| Tool execution fails | Return error in `invoke-res`, agent tries alternative |
| User rejects tool call | Return `USER_REJECTED` error, agent adapts |
| Agent timeout (no response for 60s) | Show timeout message, offer to resend |
| Pairing rejected | Show message, suggest checking gateway config |
