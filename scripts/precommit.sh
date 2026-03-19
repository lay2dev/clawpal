#!/usr/bin/env bash
set -euo pipefail

# All-in-one script to run the same checks as the pre-commit hook.
# Usage:
#   ./scripts/precommit.sh          # run all checks
#   ./scripts/precommit.sh --staged # narrow cargo fmt to staged .rs files only

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd -P)"
cd "$REPO_ROOT"

if [ "${1:-}" = "--staged" ]; then
  export CLAWPAL_FMT_SCOPE=staged
fi

printf "\n== Harness Pre-commit Check ==\n"
printf "Repo root          %s\n" "$REPO_ROOT"

printf "\n== Frontend CI ==\n"
"$REPO_ROOT/scripts/ci-frontend.sh"

printf "\n== Rust CI ==\n"
"$REPO_ROOT/scripts/ci-rust.sh"

printf "\n== Metrics CI ==\n"
if ! "$REPO_ROOT/scripts/ci-metrics.sh"; then
  printf "\n❌ Pre-commit check FAILED: one or more hard metrics gates failed.\n" >&2
  exit 1
fi

printf "\n✅ All pre-commit checks passed.\n"
