# GUI - CLI(s) - Agent Three-Layer Architecture

## Problem

Relying entirely on an LLM agent to drive install/management flows is unpredictable. The agent operates on raw shell commands with no guardrails — it may use different approaches each time, forget verification steps, or produce inconsistent output. Core business logic needs to be deterministic and hardcoded, with the agent serving as an orchestrator and exception handler.

## Architecture

```
┌─────────────────────────────────────┐
│  GUI (Tauri + React)                │
│  Display data, capture user intent  │
│  Calls clawpal-core in-process      │
└──────────────┬──────────────────────┘
               │ user intent
┌──────────────▼──────────────────────┐
│  Agent (zeroclaw LLM)               │
│  Orchestrate CLI commands           │
│  Handle exceptions with fine-grained│
│  commands when coarse ones fail     │
│  Tools: clawpal CLI + openclaw CLI  │
└──────────────┬──────────────────────┘
               │ CLI calls (subprocess)
┌──────────────▼──────────────────────┐
│  CLI(s)                             │
│  clawpal CLI — install, SSH,        │
│    profiles, multi-instance, health │
│  openclaw CLI — config, agents,     │
│    gateway, channels                │
└─────────────────────────────────────┘
```

## Key Decisions

1. **Shared library crate** — `clawpal-core` is a Rust library. `clawpal-cli` and `src-tauri` both consume it. CLI is independently runnable without Tauri.
2. **Two complementary CLIs** — `clawpal` manages instance lifecycle (install, connect, health). `openclaw` manages a single instance internally (config, agents, gateway). Agent calls whichever is appropriate.
3. **Coarse + fine-grained commands** — Coarse commands (e.g. `clawpal install docker`) are hardcoded orchestration of fine-grained steps. Agent uses coarse commands for happy path, fine-grained for patching exceptions.
4. **Agent is zeroclaw (LLM)** — Same LLM agent, but tool set changes from arbitrary shell to two structured CLIs.
5. **Rust SSH library (russh)** — Replace system `ssh` command with `russh` for cross-platform consistency. Stateless connect-execute-disconnect model per CLI call. No connection pool in core. GUI layer can optionally cache connections.
6. **Experience persistence deferred** — Agent patching exceptions is valuable, but persisting those experiences for future sessions is deferred until the three-layer architecture is running and we can observe real agent behavior patterns.

## Project Structure

```
clawpal-core/
├── src/
│   ├── lib.rs              # Public API entry
│   ├── instance.rs         # Instance registry (list/add/remove, local JSON)
│   ├── install/
│   │   ├── mod.rs          # Coarse: install_docker(), install_local()
│   │   └── docker.rs       # Fine: pull(), configure(), up()
│   ├── connect.rs          # Register existing instances (docker/ssh)
│   ├── health.rs           # Health check (calls openclaw status)
│   ├── ssh/
│   │   ├── mod.rs          # connect/disconnect/exec via russh
│   │   ├── config.rs       # Parse ~/.ssh/config
│   │   └── registry.rs     # SSH host CRUD
│   ├── profile.rs          # Model profile management
│   └── openclaw.rs         # openclaw CLI invocation wrapper

clawpal-cli/                # Binary crate, clap entry point, calls core
src-tauri/                  # Tauri app, IPC entry point, also calls core
```

Design principles:
- All public functions return `Result<T, ClawpalError>` where T is serializable. CLI serializes to JSON stdout. Tauri returns directly to frontend.
- `ssh/mod.rs` implements stateless single-connection model: `SshSession::connect(config) -> exec(cmd) -> disconnect()`. No connection pool. Tauri layer can cache above this if needed.
- `openclaw.rs` wraps openclaw CLI invocation — resolve binary path, set `OPENCLAW_HOME`, run command, parse output. Keeps the boundary between clawpal and openclaw explicit in code.
- `instance.rs` manages `~/.clawpal/instances.json` — all registered instances with id, type, label, paths.

## CLI Commands

```
# Instance management
clawpal instance list                     # List all instances (local + docker + ssh)
clawpal instance remove <id>              # Unregister instance (keeps data)

# Installation (coarse = hardcoded pipeline, fine = atomic steps)
clawpal install docker [--home PATH]      # Coarse: pull -> configure -> up -> verify
clawpal install docker pull               # Fine: pull docker-compose.yml
clawpal install docker configure          # Fine: generate secrets, env
clawpal install docker up                 # Fine: docker compose up -d
clawpal install local                     # Coarse: run official install script -> verify

# Connect existing instances
clawpal connect docker --home PATH [--label NAME]
clawpal connect ssh --host H [--port P] [--user U]

# Health
clawpal health check <instance-id>        # Check single instance
clawpal health check --all                # Check all instances

# SSH management
clawpal ssh connect <host-id>             # Establish SSH connection
clawpal ssh disconnect <host-id>          # Disconnect
clawpal ssh list                          # List registered SSH hosts

# Model profiles
clawpal profile list
clawpal profile add --provider P --model M [--api-key K]
clawpal profile remove <id>
clawpal profile test <id>                 # Test profile connectivity
```

Output conventions:
- Structured JSON output (default for stdout)
- Exit codes: 0 success, 1 business error (JSON error body), 2 argument error
- Coarse commands print progress to stderr, final result to stdout

## Agent Tool Set

```json
{
  "tools": [
    {
      "name": "clawpal",
      "description": "ClawPal CLI - manage instance lifecycle",
      "parameters": {
        "args": "string (full subcommand, e.g. 'install docker --home ~/.clawpal/test')"
      }
    },
    {
      "name": "openclaw",
      "description": "OpenClaw CLI - manage single instance internals",
      "parameters": {
        "args": "string (full subcommand, e.g. 'agents list')",
        "instance": "string (optional instance ID, auto-sets OPENCLAW_HOME)"
      }
    }
  ]
}
```

When agent runs inside Tauri process, tool calls route to clawpal-core library functions (no subprocess). When agent runs standalone, tool calls spawn CLI processes.

Security is simplified: agent can only invoke these two tools with bounded subcommands. No arbitrary shell access.

## GUI Role

Deterministic operations go directly through clawpal-core (no agent):
- Instance list, health check, SSH management, profile CRUD, instance register/remove

Only orchestration and exception handling flows involve the agent:
- Install flows, doctor diagnosis

InstallHub behavior change:
1. User clicks install tag
2. GUI runs coarse command (`clawpal install docker`) directly
3. Success -> register instance, done
4. Failure -> show error + "Let AI help fix" button
5. User clicks -> agent engages with fine-grained commands to diagnose and repair

Agent shifts from protagonist to fallback. 90% of cases are deterministic; agent only appears when things go wrong.
