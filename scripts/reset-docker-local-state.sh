#!/usr/bin/env bash
set -euo pipefail

# Reset Docker Local install state used by ClawPal install onboarding.
# Usage:
#   scripts/reset-docker-local-state.sh
#   scripts/reset-docker-local-state.sh --yes

YES=0
if [[ "${1:-}" == "--yes" ]]; then
  YES=1
fi

HOME_DIR="${HOME:-}"
if [[ -z "${HOME_DIR}" ]]; then
  echo "HOME is not set; aborting." >&2
  exit 1
fi

INSTALL_REPO_DIR="${HOME_DIR}/.clawpal/install/openclaw-docker"
DOCKER_LOCAL_DIR="${HOME_DIR}/.clawpal/docker-local"
AAD_DIR="${HOME_DIR}/.clawpal/access-discovery"
AAD_PROFILE_FILE="${AAD_DIR}/docker:local.json"
AAD_EXPERIENCE_FILE="${AAD_DIR}/docker:local.experiences.json"
IMAGE_NAME="openclaw:local"

echo "This will reset Docker Local state:"
echo "- ${INSTALL_REPO_DIR}"
echo "- ${DOCKER_LOCAL_DIR}"
echo "- ${AAD_PROFILE_FILE}"
echo "- ${AAD_EXPERIENCE_FILE}"
echo "- Docker image ${IMAGE_NAME}"

if [[ "${YES}" -ne 1 ]]; then
  echo
  read -r -p "Continue? [y/N] " answer
  case "${answer}" in
    y|Y|yes|YES) ;;
    *)
      echo "Cancelled."
      exit 0
      ;;
  esac
fi

echo
echo "[1/5] docker compose down (if repo exists)..."
if [[ -d "${INSTALL_REPO_DIR}" ]]; then
  (
    cd "${INSTALL_REPO_DIR}"
    docker compose down -v --remove-orphans || true
  )
else
  echo "  skipped (repo not found)"
fi

echo "[2/5] remove docker-local state dir..."
rm -rf "${DOCKER_LOCAL_DIR}"

echo "[3/5] remove install repo cache..."
rm -rf "${INSTALL_REPO_DIR}"

echo "[4/5] remove access-discovery files..."
rm -f "${AAD_PROFILE_FILE}" "${AAD_EXPERIENCE_FILE}"

echo "[5/5] remove local docker image (${IMAGE_NAME})..."
docker rmi -f "${IMAGE_NAME}" >/dev/null 2>&1 || true

echo
echo "Done. Docker Local state has been reset."
