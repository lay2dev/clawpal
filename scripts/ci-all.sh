#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/_common.sh"

cd_repo_root

section "Run All Local CI"
"$(pwd)/scripts/ci-frontend.sh"
"$(pwd)/scripts/ci-rust.sh"
"$(pwd)/scripts/ci-metrics.sh"
"$(pwd)/scripts/ci-coverage.sh"

section "Result"
echo "All local CI scripts passed."
