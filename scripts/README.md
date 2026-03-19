# Local CI Scripts

These scripts mirror the repository CI checks locally without installing system packages, running Docker or SSH remote perf probes, or invoking Playwright.

## Scripts

- `scripts/ci-frontend.sh`
  Runs `bun install --frozen-lockfile`, `bun run typecheck`, and `bun run build`.
- `scripts/ci-rust.sh`
  Runs `cargo fmt --check`, `cargo clippy -p clawpal-core -- -D warnings`, `cargo test -p clawpal-core`, and `cargo test -p clawpal --test perf_metrics`.
- `scripts/ci-metrics.sh`
  Runs the local metrics gate and prints a readable report covering bundle gzip size, `perf_metrics`, `command_perf_e2e`, commit-size warnings, and large-file warnings.
- `scripts/ci-coverage.sh`
  Runs `cargo llvm-cov` for `clawpal-core` and `clawpal-cli`.
- `scripts/ci-all.sh`
  Runs the frontend, Rust, metrics, and coverage scripts in order and stops on the first failure.
- `scripts/install-hooks.sh`
  Installs the git pre-commit hook by symlinking `scripts/pre-commit` into the current repo's hooks directory.
- `scripts/pre-commit`
  Runs frontend CI, Rust CI, and metrics CI before each commit.

All scripts resolve the repo root from their own path and can be run from anywhere inside the worktree.

## Hard And Soft Gates

`scripts/ci-metrics.sh` behaves differently from the other scripts:

- Hard gates fail the script:
  - total built JavaScript gzip size must be `<= 512 KB`
  - `cargo test -p clawpal --test perf_metrics` must pass
  - `cargo test -p clawpal --test command_perf_e2e` must pass
- Soft gates only report warnings:
  - individual commit size should stay at `<= 500` changed lines
  - tracked Rust and TS/TSX files over `500` lines are listed as warnings

## Hook Install

Install the hook once per worktree:

```bash
./scripts/install-hooks.sh
```

The hook uses `CLAWPAL_FMT_SCOPE=staged` when it calls `scripts/ci-rust.sh`, so `cargo fmt --check` narrows to staged `.rs` files when there are any. The rest of the Rust checks still run normally.

## Skip Or Bypass

- Skip the hook for a single commit with `git commit --no-verify`.
- Run scripts individually if you only want one check, for example `./scripts/ci-metrics.sh`.
- If `cargo llvm-cov` is missing, install it with `cargo install cargo-llvm-cov` before running `./scripts/ci-coverage.sh`.
