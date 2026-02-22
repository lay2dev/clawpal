# Doctor Agent Design

Date: 2026-02-22

## Overview

Add AI-powered diagnostic and repair capabilities to ClawPal's existing Doctor feature. When static checks can't fix the problem (or when openclaw itself is broken), an external AI agent steps in to diagnose and fix issues through a multi-turn tool-use conversation.

## Architecture

```
┌─────────────────────────────────────────────┐
│  UI Layer: Doctor.tsx + Chat.tsx (extended)  │
│  - Doctor page adds "AI Diagnose" section   │
│  - Click opens Chat in doctor mode          │
│  - Chat gains tool-call display + confirm   │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│  Agent Source (user selects manually):       │
│  1. Local openclaw instance agent           │
│  2. Other SSH openclaw instance agent       │
│  3. Remote doctor service (hosted by us)    │
│  4. (v2) codex / claude code                │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│  Tool Execution: reuse existing Tauri cmds  │
│  - read_file → sftp_read / fs::read         │
│  - write_file → sftp_write / fs::write      │
│  - run_command → ssh_exec / Command::new    │
│  - read_config, validate_config, etc.       │
│  All write ops require user confirmation    │
└─────────────────────────────────────────────┘
```

Key decisions:
- ClawPal acts as middleman — agent never directly accesses the target
- HTTP POST per-turn communication (fits user confirmation rhythm)
- Doctor context auto-collected (config, logs, system info) as initial prompt

## Agent Source Selection

User manually selects from available sources on the Doctor page:

| Source | Communication | Notes |
|---|---|---|
| Local openclaw instance | Reuse `chatViaOpenclaw()` POST to local gateway | When current instance is healthy |
| SSH remote openclaw instance | Reuse `remote_chat_via_openclaw()` via SSH | User's other configured SSH hosts |
| Remote doctor service | New HTTP POST to hosted API | Needs new API endpoint |
| codex/claude code | TBD (v2) | Optional, not in MVP |

First two sources fully reuse existing code — only the chat context/prompt differs. Remote doctor service needs a new HTTP client.

Unavailable sources shown as disabled with reason tooltip.

## Chat.tsx Extension — Tool-use Support

### New Props

```typescript
interface ChatProps {
  mode?: "chat" | "doctor";  // default "chat"
  targetInstance?: string;    // doctor mode: instance being diagnosed
  agentSource?: AgentSource;  // doctor mode: which agent to use
}

type AgentSource =
  | { type: "local" }
  | { type: "ssh"; hostId: string }
  | { type: "remote-doctor" }
```

### Extended Message Type

```typescript
interface Message {
  role: "user" | "assistant" | "tool_call" | "tool_result";
  content: string;
  toolCall?: {
    id: string;
    name: string;           // e.g. "read_file", "write_file"
    args: Record<string, unknown>;
    status: "pending" | "approved" | "rejected" | "executed";
    result?: string;
  };
}
```

### UI Behavior

- Normal assistant messages → chat bubbles (same as now)
- `tool_call` messages → card showing what agent wants to do, with "Execute" and "Skip" buttons
- Read operations (read_file, list_files) → auto-execute, no confirmation needed
- Write operations (write_file, run_command) → require user click "Execute" to confirm
- `tool_result` → collapsible display of execution result

### Doctor Context Auto-collection

On entering doctor mode, ClawPal auto-collects target instance info as the first system message:
- openclaw version / availability
- config content
- doctor report (existing static check results)
- recent error log (last 50 lines)
- system info (OS, PATH, etc.)

## Rust Backend Changes

Only 2 new Tauri commands needed:

### 1. Remote doctor service HTTP client

```rust
#[tauri::command]
async fn doctor_chat(
    endpoint: String,       // remote doctor service URL
    messages: Vec<Message>, // context + conversation history + tool results
) -> Result<DoctorResponse, String>

struct DoctorResponse {
    message: Option<String>,      // agent's text reply
    tool_calls: Vec<ToolCall>,    // operations agent wants to perform
    done: bool,                   // whether diagnosis is complete
}
```

### 2. Diagnostic context collector

```rust
#[tauri::command]
fn collect_doctor_context(instance_id: String) -> DoctorContext {
    // Bundles: config, version, doctor report, error logs, system info
    // Works for both local and SSH instances (reuses existing code)
}
```

Everything else is reused — no new tool execution logic needed in backend.

## Doctor.tsx Integration

Add "AI Diagnose" section next to existing Health card:

```
┌─ Health ──────────────┐  ┌─ AI Diagnose ─────────┐
│ Score: 85             │  │ Select assistant:      │
│ ⚠ WARN: field.agents │  │ [Local] [SSH-1] [Remote]│
│ [Fix All] [Refresh]   │  │                        │
└───────────────────────┘  │ [Start Diagnosis]      │
                           └────────────────────────┘
```

## User Flow

```
User opens Doctor page → sees static check results
    → static fix works → click Fix All, done
    → doesn't work → select agent source → click "Start Diagnosis"
        → Chat opens in doctor mode, auto-sends diagnostic context
        → Agent: "I see config issues, let me read the error log"
        → [read_file: ~/.openclaw/logs/error.log] ← auto-executed
        → Agent: "Found corrupted session, I need to delete and rebuild"
        → [write_file: ~/.openclaw/sessions/xxx] ← user clicks "Execute"
        → Agent: "Fixed. Let me verify."
        → [run_command: openclaw doctor] ← user clicks "Execute"
        → Agent: "All checks pass. Repair complete ✓"
```

Error handling:
- Agent source unavailable → button disabled + reason shown
- Agent response timeout → show retry button
- Tool execution fails → result sent back to agent, it tries another approach

## Scope

### MVP (v1)
- Chat.tsx extended with doctor mode (tool call display + confirmation flow)
- Doctor.tsx adds agent source selection + start diagnosis entry
- `collect_doctor_context` command
- `doctor_chat` command (remote doctor service)
- Local/SSH openclaw instances reuse existing chat channel

### Not in MVP
- codex / claude code integration (v2)
- Automatic agent source selection/priority
- Tool call diff preview (e.g. showing file changes before write_file)
- Diagnosis history persistence
- Parallel diagnosis of multiple issues

### Dependencies
- Remote doctor service: deploy an openclaw agent on server, expose HTTP API
- That API must support receiving tool results and continuing conversation (multi-turn tool-use)
