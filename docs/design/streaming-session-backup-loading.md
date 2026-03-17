# Design: Stream-Based Session & Backup Loading

> **Status**: Design only — no code changes  
> **Author**: @ocbot (design), requested by @chen.yu  
> **Date**: 2026-03-17

## Problem

Current session and backup loading paths eagerly collect all data into memory before returning to the frontend:

| Operation | Memory Pattern | Risk |
|-----------|---------------|------|
| `analyze_sessions` / `remote_analyze_sessions` | Reads every `.jsonl` file, parses every line → builds full `Vec<AgentSessionAnalysis>` in memory | O(total session bytes) |
| `list_session_files` / `remote_list_session_files` | Scans all agent dirs, collects all `SessionFile` entries | O(file count × entry size) |
| `preview_session` / `remote_preview_session` | Reads entire JSONL into memory, parses all lines | O(session file size) |
| `backup_before_upgrade` / `remote_backup_before_upgrade` | `copy_dir_recursive` copies all files synchronously | O(backup size) |
| `list_backups` / `remote_list_backups` | Collects all backup metadata at once | Low risk, but blocks on N `du` calls |
| `restore_from_backup` | `restore_dir_recursive` copies everything synchronously | O(backup size) |

For users with hundreds of sessions (common on long-running instances) or large backups (`agents/` + `memory/` can exceed 100MB), these operations cause:

1. **Peak memory spikes** — entire dataset materialized in Rust `Vec`s before Tauri serializes to JSON
2. **UI unresponsiveness** — single large Tauri `invoke` blocks the frontend Promise; no progress indication
3. **SSH timeouts** — remote `sftp_read` for large JSONL files can exceed the 30s timeout
4. **OOM risk on constrained devices** (Raspberry Pi, small VPS)

---

## Design Goals

- **Bounded memory usage**: process data in chunks, never hold the full dataset in memory at once
- **Progressive UI updates**: frontend receives data incrementally, renders partial results
- **Cancellation support**: user can abort long-running scans
- **Backward compatibility**: existing Tauri command API surface unchanged for simple/fast cases
- **Uniform local/remote pattern**: same streaming abstraction works for both local FS and SSH/SFTP

---

## Architecture

### 1. Tauri Event Channel Pattern

Replace single-response `invoke` commands with **streaming commands** that emit Tauri events and return a lightweight handle:

```
Frontend                      Tauri Backend
   │                              │
   ├─ invoke("analyze_sessions_  │
   │   stream", {batchSize:50}) ──►  spawn_blocking {
   │                              │    for each agent {
   │  ◄── emit("sessions:chunk", │      for each batch of 50 sessions {
   │      {agent, sessions, ...}) │        emit → frontend
   │  ◄── emit("sessions:chunk", │      }
   │      {agent, sessions, ...}) │    }
   │  ◄── emit("sessions:done",  │    emit done
   │      {totalAgents, ...})    ─┤  }
   │                              │
   ├── Result<StreamHandle> ◄─────┤  (immediate: returns handle for cancellation)
```

### 2. Stream Command Variants

#### 2.1 `analyze_sessions_stream`

```rust
#[tauri::command]
pub async fn analyze_sessions_stream(
    app: AppHandle,
    batch_size: Option<usize>,  // default 50
) -> Result<String, String> {
    let handle_id = uuid::Uuid::new_v4().to_string();
    let batch_size = batch_size.unwrap_or(50);
    let cancel_token = CancellationToken::new();
    CANCEL_TOKENS.insert(handle_id.clone(), cancel_token.clone());

    let hid = handle_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = resolve_paths();
        let agents_root = paths.base_dir.join("agents");

        let mut agent_count = 0u32;
        for agent_entry in fs::read_dir(&agents_root).into_iter().flatten().flatten() {
            if cancel_token.is_cancelled() { break; }

            // ... scan sessions for this agent using BufReader (line-by-line) ...
            // Classify each session, accumulate into batch buffer

            for chunk in sessions.chunks(batch_size) {
                if cancel_token.is_cancelled() { break; }
                app.emit("sessions:chunk", json!({
                    "handleId": hid,
                    "agent": agent_name,
                    "sessions": chunk,
                })).ok();
            }
            agent_count += 1;
        }

        app.emit("sessions:done", json!({
            "handleId": hid,
            "totalAgents": agent_count,
        })).ok();
        CANCEL_TOKENS.remove(&hid);
    });

    Ok(handle_id)
}
```

**Memory bound**: at most `batch_size` `SessionAnalysis` structs + 1 agent's `sessions.json` metadata in memory at any time.

#### 2.2 `preview_session_stream`

For large session files, stream messages in pages:

```rust
#[tauri::command]
pub async fn preview_session_stream(
    app: AppHandle,
    agent_id: String,
    session_id: String,
    page_size: Option<usize>,  // default 100 messages
) -> Result<String, String> {
    let handle_id = uuid::Uuid::new_v4().to_string();
    let page_size = page_size.unwrap_or(100);

    // spawn_blocking:
    //   open file with BufReader
    //   read + parse page_size JSONL lines at a time
    //   emit "session-preview:page" { handleId, messages: [...], offset }
    //   emit "session-preview:done" { handleId, totalMessages }

    Ok(handle_id)
}
```

**Memory bound**: at most `page_size` parsed messages at a time.

#### 2.3 `remote_analyze_sessions_stream`

The remote case already uses a single SSH command that outputs a JSON array.

**Preferred approach — incremental JSON parse of SSH stdout**:

```rust
pub async fn remote_analyze_sessions_stream(
    pool: State<'_, SshConnectionPool>,
    app: AppHandle,
    host_id: String,
    batch_size: Option<usize>,
) -> Result<String, String> {
    let handle_id = uuid::Uuid::new_v4().to_string();
    let batch_size = batch_size.unwrap_or(50);

    // Use pool.exec_streaming() — new method returning mpsc::Receiver<Vec<u8>>
    // Feed bytes into serde_json::StreamDeserializer
    // Accumulate deserialized entries into batches of batch_size
    // Emit "sessions:chunk" for each batch
    // Emit "sessions:done" on stream EOF

    Ok(handle_id)
}
```

**Alternative — paginated SSH** (if `russh` streaming proves difficult):

Split into `remote_count_sessions` + `remote_analyze_sessions_page(offset, limit)`, using `awk`/`sed` for server-side pagination. More SSH round-trips but strictly bounded memory per call.

#### 2.4 Backup/Restore Progress Events

Backup/restore operations benefit from **progress events** rather than data streaming:

```rust
#[tauri::command]
pub async fn backup_before_upgrade_stream(
    app: AppHandle,
) -> Result<BackupInfo, String> {
    // Same copy logic, but emit progress events:
    // "backup:progress" { phase: "config", filesCopied: 1, bytesTotal: N }
    // "backup:progress" { phase: "agents", filesCopied: 42, bytesTotal: N }
    // "backup:progress" { phase: "memory", filesCopied: 5, bytesTotal: N }
    // Return final BackupInfo as before
}
```

For remote backups (SSH `cp` + `du`), emit phase markers:

```
"backup:progress" { phase: "snapshot" }   → started
"backup:progress" { phase: "config" }     → config copied
"backup:progress" { phase: "agents" }     → agents dir copied
"backup:progress" { phase: "memory" }     → memory dir copied
```

Remote restore follows the same pattern.

---

### 3. SSH Streaming Layer

Add a streaming exec method to `SshConnectionPool`:

```rust
impl SshConnectionPool {
    /// Execute a command and yield stdout in chunks via a bounded channel.
    pub async fn exec_streaming(
        &self,
        id: &str,
        script: &str,
        chunk_size: usize,         // bytes per read
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, String>>, String> {
        let (tx, rx) = mpsc::channel(16);  // bounded → backpressure

        // Acquire SSH session from pool
        // Spawn async task:
        //   loop { read up to chunk_size bytes from channel stdout }
        //   send each chunk through tx
        //   close tx on EOF or error

        Ok(rx)
    }
}
```

This enables the remote session analysis to process SSH output incrementally without buffering the entire stdout (currently `result.stdout` holds the whole string).

---

### 4. Frontend Integration

#### 4.1 `useSessionStream` Hook

```typescript
// src/lib/use-session-stream.ts

interface StreamState {
  agents: Map<string, AgentSessionAnalysis>;
  loading: boolean;
  progress: { received: number; done: boolean };
}

function useSessionStream() {
  const [state, setState] = useState<StreamState>({
    agents: new Map(),
    loading: false,
    progress: { received: 0, done: false },
  });

  const handleRef = useRef<string | null>(null);

  const start = useCallback(async (batchSize = 50) => {
    setState({ agents: new Map(), loading: true, progress: { received: 0, done: false } });

    const handleId: string = await invoke("analyze_sessions_stream", { batchSize });
    handleRef.current = handleId;

    const unlisten1 = await listen<SessionChunkEvent>("sessions:chunk", (event) => {
      const { handleId: hid, agent, sessions } = event.payload;
      if (hid !== handleId) return;

      setState(prev => {
        const next = new Map(prev.agents);
        const existing = next.get(agent);
        const merged = existing
          ? { ...existing, sessions: [...existing.sessions, ...sessions] }
          : buildAgentAnalysis(agent, sessions);
        // Recompute counts
        merged.totalFiles = merged.sessions.length;
        merged.totalSizeBytes = merged.sessions.reduce((s, x) => s + x.sizeBytes, 0);
        merged.emptyCount = merged.sessions.filter(s => s.category === "empty").length;
        merged.lowValueCount = merged.sessions.filter(s => s.category === "low_value").length;
        merged.valuableCount = merged.sessions.filter(s => s.category === "valuable").length;
        next.set(agent, merged);

        return {
          agents: next,
          loading: true,
          progress: { received: prev.progress.received + sessions.length, done: false },
        };
      });
    });

    const unlisten2 = await listen<SessionDoneEvent>("sessions:done", (event) => {
      if (event.payload.handleId !== handleId) return;
      setState(prev => ({ ...prev, loading: false, progress: { ...prev.progress, done: true } }));
      unlisten1();
      unlisten2();
    });

    return handleId;
  }, []);

  const cancel = useCallback(async () => {
    if (handleRef.current) {
      await invoke("cancel_stream", { handleId: handleRef.current });
      setState(prev => ({ ...prev, loading: false }));
    }
  }, []);

  return { ...state, start, cancel };
}
```

#### 4.2 Progressive Rendering in `SessionAnalysisPanel`

```
┌─────────────────────────────────────────────────────┐
│  Sessions Analysis                        [Cancel]  │
│                                                     │
│  [Agent: main]    ████████░░░░  42 sessions loaded  │
│    ● empty (12)  ● low_value (8)  ● valuable (22)  │
│    [Details ▼]                                      │
│                                                     │
│  [Agent: cron]    ████████████  18/18 complete       │
│    ● empty (3)  ● valuable (15)                     │
│    [Details ▼]                                      │
│                                                     │
│  [Loading agent: discord...]  ░░░░░░░░░░            │
│                                                     │
│  Total: 60 sessions loaded...                       │
└─────────────────────────────────────────────────────┘
```

Agents render as soon as their first chunk arrives. Badge counts update progressively. The user can expand details and interact with already-loaded agents while the scan continues.

#### 4.3 Auto-Selection Logic

Frontend auto-selects eager vs. stream based on dataset size:

```typescript
// In SessionAnalysisPanel:
const files = await ua.listSessionFiles();  // lightweight, fast
if (files.length > SESSION_STREAM_THRESHOLD) {  // e.g. 100
  // Use streaming variant
  const { start } = useSessionStream();
  await start();
} else {
  // Use existing eager variant (simpler, fine for small datasets)
  const analysis = await ua.analyzeSessions();
  setSessionAnalysis(analysis);
}
```

---

### 5. Cancellation

Global cancel token registry:

```rust
use dashmap::DashMap;
use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;

static CANCEL_TOKENS: Lazy<DashMap<String, CancellationToken>> = Lazy::new(DashMap::new);

#[tauri::command]
pub fn cancel_stream(handle_id: String) -> Result<(), String> {
    if let Some((_, token)) = CANCEL_TOKENS.remove(&handle_id) {
        token.cancel();
    }
    Ok(())
}
```

All stream loops check `cancel_token.is_cancelled()` before each emit. On cancellation:
- Emit `"sessions:cancelled"` event with `handleId`
- Clean up token from `CANCEL_TOKENS`
- Frontend listener detects cancel event, sets `loading = false`

---

### 6. Memory Budget

Target: **< 10MB peak** for session analysis on a 1000-session instance.

| Component | Current Peak | After Streaming |
|-----------|-------------|-----------------|
| Local analysis (500 sessions) | ~50MB (all JSONL parsed + full Vec) | ~2MB (batch of 50 + BufReader 8KB) |
| Remote analysis (500 sessions) | ~5MB (SSH stdout string) | ~500KB (streaming parse buffer) |
| Session preview (10MB JSONL) | ~10MB (full string) | ~200KB (page of 100 messages) |
| Backup copy (100MB data) | ~8KB (OS copy buffer) | ~8KB (unchanged, add progress events) |

---

## Data Flow Diagrams

### Session Analysis — Local, Streamed

```
┌──────────────┐     ┌───────────────────┐     ┌──────────────┐
│  Frontend    │     │  Tauri Backend     │     │  Filesystem  │
│  (React)     │     │  (spawn_blocking)  │     │              │
├──────────────┤     ├───────────────────┤     ├──────────────┤
│              │────►│ analyze_sessions_  │     │              │
│              │     │ stream(batch:50)   │     │              │
│              │◄────│ → handle_id        │     │              │
│              │     │                    │     │              │
│              │     │ for agent in dirs: │────►│ read_dir()   │
│              │     │   load meta.json   │◄────│ sessions.json│
│              │     │   for file in dir: │────►│ open(f.jsonl)│
│              │     │     BufReader.lines│◄────│ line by line │
│              │     │     classify()     │     │              │
│              │     │     if batch full: │     │              │
│ ◄── event ───│◄────│       emit chunk   │     │              │
│  render()    │     │                    │     │              │
│ ◄── event ───│◄────│   emit agent done  │     │              │
│  render()    │     │                    │     │              │
│ ◄── event ───│◄────│ emit all done      │     │              │
│  setLoading  │     │                    │     │              │
│  (false)     │     │                    │     │              │
└──────────────┘     └───────────────────┘     └──────────────┘
```

### Session Analysis — Remote, Streamed

```
┌──────────────┐     ┌───────────────────┐     ┌──────────────┐
│  Frontend    │     │  Tauri Backend     │     │  Remote Host │
├──────────────┤     ├───────────────────┤     │  (via SSH)   │
│              │────►│ remote_analyze_    │     ├──────────────┤
│              │     │ sessions_stream()  │     │              │
│              │◄────│ → handle_id        │     │              │
│              │     │                    │────►│ exec(script) │
│              │     │ exec_streaming()   │◄────│ stdout: [    │
│              │     │                    │◄────│   {entry1},  │
│              │     │ StreamDeserializer │◄────│   {entry2},  │
│              │     │   batch 50 entries │◄────│   ...        │
│ ◄── event ───│◄────│     emit chunk     │◄────│   {entryN}   │
│              │     │                    │◄────│ ]            │
│ ◄── event ───│◄────│ emit done          │     │              │
└──────────────┘     └───────────────────┘     └──────────────┘
```

---

## Migration Path

| Phase | Scope | Breaking Changes |
|-------|-------|-----------------|
| **Phase 1** | Add `*_stream` command variants alongside existing commands | None |
| **Phase 2** | Frontend auto-selects stream vs. eager based on dataset size | None |
| **Phase 3** | Deprecate eager variants (log warning if dataset > threshold) | None |
| **Phase 4** | Remove eager variants | Major version bump |

---

## New Dependencies

| Crate | Purpose | Notes |
|-------|---------|-------|
| `tokio_util::sync::CancellationToken` | Cooperative cancellation for stream loops | Add to `src-tauri/Cargo.toml` |
| `dashmap` | Concurrent cancel token registry | Or use `std::sync::Mutex<HashMap>` for simpler alternative |
| `uuid` | Handle ID generation | Already in deps (used by install session) |

`serde_json::StreamDeserializer` and `tokio::sync::mpsc` are already available.

---

## Open Questions

1. **Batch size tuning**: 50 sessions/chunk is an estimate. Should we benchmark on Pi/low-memory VPS to find the sweet spot between Tauri event overhead and memory?

2. **Remote streaming feasibility**: `russh` SSH channel read may buffer internally — need to verify that `exec_streaming` can actually yield partial stdout before the remote command completes. If not, fallback to paginated SSH approach.

3. **Event ordering & React batching**: Tauri events are ordered per-emitter, but React 18 may batch multiple `setState` calls from rapid events. Does the chunk merge logic need sequence numbers or is insertion order sufficient?

4. **Compression for remote**: For remote analysis with many sessions, should we gzip the SSH stdout? Depends on whether bottleneck is network bandwidth or client-side parse time.

5. **`list_session_files` streaming**: This operation returns lightweight entries (no JSONL parsing). Is it worth adding a stream variant, or is the eager version fast enough even for 1000+ files?
