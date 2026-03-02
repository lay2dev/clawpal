# Codex Execution Prompt: GUI-CLI-Agent Three-Layer Refactor

## How to Execute

Use the `superpowers:executing-plans` skill to execute this refactor. The plan file is at `docs/plans/2026-02-26-gui-cli-agent-layers-plan.md`.

For each phase, also use `superpowers:verification-before-completion` before committing to ensure the phase actually compiles and passes tests.

## Context

You are implementing a major architectural refactor for ClawPal, a Tauri desktop app that manages OpenClaw instances. The refactor extracts business logic from Tauri commands into a shared `clawpal-core` library crate, adds a standalone `clawpal-cli` binary, and replaces the SSH implementation.

**Read these two documents before writing any code:**
- `docs/plans/2026-02-26-gui-cli-agent-layers-design.md` — architecture decisions
- `docs/plans/2026-02-26-gui-cli-agent-layers-plan.md` — 10-phase implementation plan

You are on branch `refactor/gui-cli-agent-layers`. Commit after each phase.

## Execution Rules

1. **One phase at a time.** Complete and verify each phase before moving to the next. Do not skip ahead.
2. **Each phase must compile.** `cargo build` for all workspace members must pass. The Tauri app must still compile (it does not need to launch, but `cargo build -p clawpal` must succeed).
3. **Preserve existing behavior.** When extracting code from `src-tauri/` to `clawpal-core/`, update the Tauri side to call `clawpal_core::*` instead of local functions. Do not break existing Tauri commands — they should delegate to core.
4. **The 9400-line `commands.rs` shrinks gradually.** Each phase moves some functions out. Do not try to refactor it all at once. Only touch the parts relevant to the current phase.
5. **Add unit tests in clawpal-core** for each module you create. At minimum: one test per public function that exercises the happy path. Use `#[cfg(test)]` modules.
6. **CLI output is JSON to stdout.** All `clawpal-cli` subcommands should serialize results as JSON. Human-readable output is not needed yet.
7. **Do not modify the frontend (src/) until Phase 10.** Phases 1-9 are Rust-only.

## Phase Execution Order

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7 → Phase 8 → Phase 9 → Phase 10
```

Phases 4, 5, 6 can be done in any order after Phase 3, but do them sequentially (not in parallel) to keep commits clean.

## Starting Phase 1

Start with Phase 1: Workspace Setup + Core Crate Skeleton.

Key details:
- Root `Cargo.toml` should be a workspace with `members = ["clawpal-core", "clawpal-cli", "src-tauri"]`
- `clawpal-core` is a library crate (`lib.rs` with empty pub mod declarations)
- `clawpal-cli` is a binary crate using `clap` derive macros
- `src-tauri/Cargo.toml` gets `clawpal-core = { path = "../clawpal-core" }` as a dependency
- The existing `src-tauri/Cargo.toml` has `[package]` at the top — it is NOT currently a workspace member. You need to restructure so the root `Cargo.toml` is the workspace definition and `src-tauri/Cargo.toml` is a member.

After Phase 1 compiles, commit with message: `refactor: phase 1 — workspace setup and core crate skeleton`

Then read Phase 2 from the plan and continue.

## Important Code Context

- `src-tauri/src/commands.rs` (9400 lines) — the monolith to decompose. Contains all business logic. Functions are organized by domain (SSH, profiles, agents, health, etc.) but all in one file.
- `src-tauri/src/cli_runner.rs` — OpenClaw CLI invocation, command queueing, preview/apply logic
- `src-tauri/src/ssh.rs` (52KB) — SSH connection pool using `openssh` crate (Unix-only). Being replaced by `russh` in Phase 5.
- `src-tauri/src/install/` — Install orchestration, already modular (types, runners, session store)
- `src-tauri/src/runtime/zeroclaw/` — LLM agent adapter (doctor + install modes)

## Commit Convention

Use this format for each phase commit:
```
refactor: phase N — short description

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
```
