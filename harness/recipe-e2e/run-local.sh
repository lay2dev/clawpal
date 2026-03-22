#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

OPENCLAW_IMAGE="${OPENCLAW_IMAGE:-clawpal-recipe-openclaw:latest}"
HARNESS_IMAGE="${HARNESS_IMAGE:-clawpal-recipe-harness:latest}"
ARTIFACT_ROOT="${REPO_ROOT}/harness/artifacts/recipe-e2e"
SCREENSHOT_DIR="${ARTIFACT_ROOT}/screenshots"
REPORT_DIR="${ARTIFACT_ROOT}/report"

mkdir -p "${SCREENSHOT_DIR}" "${REPORT_DIR}"

echo "Building ${OPENCLAW_IMAGE}"
docker build \
  -t "${OPENCLAW_IMAGE}" \
  -f "${REPO_ROOT}/harness/recipe-e2e/openclaw-container/Dockerfile" \
  "${REPO_ROOT}"

echo "Building ${HARNESS_IMAGE}"
docker build \
  -t "${HARNESS_IMAGE}" \
  -f "${REPO_ROOT}/harness/recipe-e2e/Dockerfile" \
  "${REPO_ROOT}"

echo "Running recipe GUI E2E harness"
docker run --rm \
  --network host \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v "${SCREENSHOT_DIR}:/screenshots" \
  -v "${REPORT_DIR}:/report" \
  -e OPENCLAW_IMAGE="${OPENCLAW_IMAGE}" \
  "${HARNESS_IMAGE}"

echo
echo "Screenshots: ${SCREENSHOT_DIR}"
echo "Perf report: ${REPORT_DIR}/perf-report.json"

if [ -f "${REPORT_DIR}/perf-report.json" ]; then
  cat "${REPORT_DIR}/perf-report.json"
fi
