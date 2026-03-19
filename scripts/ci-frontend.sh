#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/_common.sh"

cd_repo_root
require_command bun

section "Frontend CI"
status_line "Repo root" "$(pwd)"

section "Install"
bun install --frozen-lockfile

section "Typecheck"
bun run typecheck

section "Build"
bun run build

section "Result"
echo "Frontend CI passed."
