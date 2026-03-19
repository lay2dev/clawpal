#!/usr/bin/env bash

repo_root() {
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
  cd "${script_dir}/.." >/dev/null 2>&1
  pwd -P
}

cd_repo_root() {
  cd "$(repo_root)"
}

require_command() {
  local missing=0
  local cmd
  for cmd in "$@"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      printf "Missing required command: %s\n" "$cmd" >&2
      missing=1
    fi
  done

  if [ "$missing" -ne 0 ]; then
    exit 127
  fi
}

section() {
  printf "\n== %s ==\n" "$1"
}

status_line() {
  local label="$1"
  local message="$2"
  printf "%-18s %s\n" "$label" "$message"
}
