#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/_common.sh"

run_fmt_check() {
  local fmt_scope="${CLAWPAL_FMT_SCOPE:-all}"

  if [ "$fmt_scope" = "staged" ]; then
    local staged_rs=()
    mapfile -t staged_rs < <(git diff --cached --name-only --diff-filter=ACMR -- "*.rs")
    if [ "${#staged_rs[@]}" -gt 0 ]; then
      status_line "cargo fmt" "checking staged Rust files only"
      cargo fmt --manifest-path Cargo.toml --all -- --check "${staged_rs[@]}"
      return
    fi
    status_line "cargo fmt" "no staged Rust files; skipping format check"
    return
  fi

  status_line "cargo fmt" "checking full workspace"
  cargo fmt --manifest-path Cargo.toml --all -- --check
}

cd_repo_root
require_command cargo git

section "Rust CI"
status_line "Repo root" "$(pwd)"

section "Format"
run_fmt_check

section "Clippy"
cargo clippy --manifest-path Cargo.toml -p clawpal-core -- -D warnings

section "Core Tests"
cargo test --manifest-path Cargo.toml -p clawpal-core

section "Perf Metrics Test"
cargo test --manifest-path Cargo.toml -p clawpal --test perf_metrics

section "Result"
echo "Rust CI passed."
