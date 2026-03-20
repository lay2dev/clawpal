#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/_common.sh"

cd_repo_root
require_command cargo

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "cargo-llvm-cov is required. Install it with: cargo install cargo-llvm-cov" >&2
  exit 127
fi

section "Coverage"
status_line "Repo root" "$(pwd)"

cargo llvm-cov --manifest-path Cargo.toml --package clawpal-core --package clawpal-cli
