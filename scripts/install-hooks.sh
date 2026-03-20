#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/_common.sh"

cd_repo_root
require_command git ln

hook_path="$(git rev-parse --git-path hooks/pre-commit)"
mkdir -p "$(dirname "$hook_path")"
ln -sfn "$(pwd)/scripts/pre-commit" "$hook_path"

section "Hooks"
status_line "Installed" "$hook_path -> $(pwd)/scripts/pre-commit"
