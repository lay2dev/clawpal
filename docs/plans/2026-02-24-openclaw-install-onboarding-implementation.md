# OpenClaw Install Onboarding Implementation

## Date
2026-02-25

## Scope Delivered

Implemented MVP install onboarding flow that shifts ClawPal entry from post-install configuration to install-first guidance.

### UI

- Added `InstallHub` on Home page.
- User-select-first method picker: `local`, `wsl2`, `docker`, `remote_ssh`.
- Added session lifecycle UI:
  - create install session
  - run `precheck/install/init/verify`
  - show per-step status (`pending/running/success/failed`)
  - show result summary and command list
  - retry failed step
- Added ready-state handoff actions:
  - `Run Doctor`
  - `Open Recipes`

### Backend (Tauri)

- Added install domain types and step result schema.
- Added install session store (in-memory).
- Added commands:
  - `install_create_session`
  - `install_get_session`
  - `install_run_step`
  - `install_list_methods`
- Added method-specific runners:
  - `install/runners/local.rs`
  - `install/runners/wsl2.rs`
  - `install/runners/docker.rs`
  - `install/runners/remote_ssh.rs`

### API / Types

- Extended `src/lib/api.ts` with install APIs.
- Extended `src/lib/types.ts` with install types.
- Extended `src/lib/use-api.ts` with install methods for UI.

## Verification

Executed locally:

```bash
npm run typecheck
npm run build
cd src-tauri && cargo test --test install_api -- --nocapture
```

Results:
- TypeScript checks passed.
- Production build passed.
- `install_api` tests passed.

## Notes / Known Gaps

- Current runners provide auditable command plans and deterministic flow behavior.
- Real host/container execution integration per method is not completed yet.
- Session store is in-memory only (not persisted across app restarts).
