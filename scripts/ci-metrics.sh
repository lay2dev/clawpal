#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/_common.sh"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/clawpal-metrics.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

BUNDLE_RAW_KB="N/A"
BUNDLE_GZIP_KB="N/A"
BUNDLE_INIT_GZIP_KB="N/A"
BUNDLE_LIMIT_KB=350
BUNDLE_STATUS="FAIL"
BUNDLE_NOTE=""
BUNDLE_LOG="$TMP_DIR/bundle.log"
touch "$BUNDLE_LOG"

PERF_STATUS="FAIL"
PERF_EXIT_CODE="N/A"
PERF_NOTE=""
PERF_PASSED="N/A"
PERF_FAILED="N/A"
PERF_RSS_MB="N/A"
PERF_VMS_MB="N/A"
PERF_CMD_P50="N/A"
PERF_CMD_P95="N/A"
PERF_CMD_MAX="N/A"
PERF_UPTIME="N/A"
PERF_LOG="$TMP_DIR/perf_metrics.log"
touch "$PERF_LOG"

CMD_PERF_STATUS="FAIL"
CMD_PERF_EXIT_CODE="N/A"
CMD_PERF_NOTE=""
CMD_PERF_PASSED="N/A"
CMD_PERF_FAILED="N/A"
CMD_PERF_COUNT="N/A"
CMD_PERF_RSS="N/A"
CMD_PERF_LOG="$TMP_DIR/command_perf.log"
touch "$CMD_PERF_LOG"

COMMIT_STATUS="SKIP"
COMMIT_NOTE=""
COMMIT_BASE_REF="N/A"
COMMIT_BASE_SHA="N/A"
COMMIT_TOTAL=0
COMMIT_MAX=0
COMMIT_FAIL_COUNT=0
COMMIT_DETAILS_FILE="$TMP_DIR/commit_details.txt"
touch "$COMMIT_DETAILS_FILE"

LARGE_STATUS="PASS"
LARGE_COUNT=0
LARGE_DETAILS_FILE="$TMP_DIR/large_files.txt"
touch "$LARGE_DETAILS_FILE"

run_capture() {
  local log_file="$1"
  shift

  set +e
  "$@" >"$log_file" 2>&1
  local exit_code=$?
  set -e

  return "$exit_code"
}

extract_metric() {
  local pattern="$1"
  local file="$2"
  if [ ! -f "$file" ]; then
    printf "N/A"
    return
  fi
  local value
  value="$(grep -Eo "$pattern" "$file" | head -n1 | cut -d= -f2 || true)"
  if [ -n "$value" ]; then
    printf "%s" "$value"
  else
    printf "N/A"
  fi
}

print_log_tail() {
  local title="$1"
  local file="$2"
  local lines="${3:-20}"

  if [ ! -s "$file" ]; then
    return
  fi

  printf "\n%s\n" "$title"
  tail -n "$lines" "$file"
}

find_compare_ref() {
  local upstream_ref
  if upstream_ref="$(git rev-parse --abbrev-ref --symbolic-full-name "@{upstream}" 2>/dev/null)"; then
    printf "%s" "$upstream_ref"
    return 0
  fi

  local current_branch
  current_branch="$(git branch --show-current)"
  local candidate
  for candidate in origin/main main origin/develop develop; do
    if [ "$candidate" = "$current_branch" ]; then
      continue
    fi
    if git rev-parse --verify "${candidate}^{commit}" >/dev/null 2>&1; then
      printf "%s" "$candidate"
      return 0
    fi
  done

  return 1
}

run_bundle_check() {
  if ! command -v bun >/dev/null 2>&1; then
    BUNDLE_NOTE="bun is not installed"
    return
  fi
  if ! command -v gzip >/dev/null 2>&1; then
    BUNDLE_NOTE="gzip is not installed"
    return
  fi

  : >"$BUNDLE_LOG"
  {
    echo "\$ bun install --frozen-lockfile"
    bun install --frozen-lockfile
    echo
    echo "\$ bun run build"
    bun run build
  } >"$BUNDLE_LOG" 2>&1 || {
    BUNDLE_NOTE="frontend install/build failed"
    return
  }

  local js_files=()
  mapfile -t js_files < <(find dist/assets -maxdepth 1 -type f -name "*.js" | sort)
  if [ "${#js_files[@]}" -eq 0 ]; then
    BUNDLE_NOTE="no built JavaScript assets found under dist/assets"
    return
  fi

  local raw_bytes=0
  local gzip_bytes=0
  local init_gzip_bytes=0
  local file
  for file in "${js_files[@]}"; do
    local size
    size="$(wc -c <"$file" | tr -d ' ')"
    raw_bytes=$((raw_bytes + size))

    local gz_size
    gz_size="$(gzip -c "$file" | wc -c | tr -d ' ')"
    gzip_bytes=$((gzip_bytes + gz_size))

    case "$(basename "$file")" in
      index-*|vendor-react-*|vendor-ui-*|vendor-i18n-*|vendor-icons-*)
        init_gzip_bytes=$((init_gzip_bytes + gz_size))
        ;;
    esac
  done

  BUNDLE_RAW_KB=$((raw_bytes / 1024))
  BUNDLE_GZIP_KB=$((gzip_bytes / 1024))
  BUNDLE_INIT_GZIP_KB=$((init_gzip_bytes / 1024))

  if [ "$BUNDLE_GZIP_KB" -le "$BUNDLE_LIMIT_KB" ]; then
    BUNDLE_STATUS="PASS"
    BUNDLE_NOTE="gzip bundle is within limit"
  else
    BUNDLE_NOTE="gzip bundle exceeds ${BUNDLE_LIMIT_KB} KB"
  fi
}

run_perf_metrics_check() {
  if ! command -v cargo >/dev/null 2>&1; then
    PERF_NOTE="cargo is not installed"
    PERF_STATUS="SKIP"
    return
  fi

  if run_capture "$PERF_LOG" cargo test --manifest-path Cargo.toml -p clawpal --test perf_metrics -- --nocapture; then
    PERF_EXIT_CODE=0
    PERF_STATUS="PASS"
    PERF_NOTE="perf_metrics passed"
  else
    PERF_EXIT_CODE=$?
    PERF_NOTE="perf_metrics failed"
  fi

  PERF_PASSED="$(grep -Eo '[0-9]+ passed' "$PERF_LOG" | tail -n1 | awk '{print $1}' || true)"
  PERF_FAILED="$(grep -Eo '[0-9]+ failed' "$PERF_LOG" | tail -n1 | awk '{print $1}' || true)"
  PERF_PASSED="${PERF_PASSED:-0}"
  PERF_FAILED="${PERF_FAILED:-0}"
  PERF_RSS_MB="$(extract_metric 'METRIC:rss_mb=[0-9.]+' "$PERF_LOG")"
  PERF_VMS_MB="$(extract_metric 'METRIC:vms_mb=[0-9.]+' "$PERF_LOG")"
  PERF_CMD_P50="$(extract_metric 'METRIC:cmd_p50_us=[0-9.]+' "$PERF_LOG")"
  PERF_CMD_P95="$(extract_metric 'METRIC:cmd_p95_us=[0-9.]+' "$PERF_LOG")"
  PERF_CMD_MAX="$(extract_metric 'METRIC:cmd_max_us=[0-9.]+' "$PERF_LOG")"
  PERF_UPTIME="$(extract_metric 'METRIC:uptime_secs=[0-9.]+' "$PERF_LOG")"
}

run_command_perf_check() {
  if ! command -v cargo >/dev/null 2>&1; then
    CMD_PERF_NOTE="cargo is not installed"
    CMD_PERF_STATUS="SKIP"
    PERF_STATUS="SKIP"
    return
  fi

  if run_capture "$CMD_PERF_LOG" cargo test --manifest-path Cargo.toml -p clawpal --test command_perf_e2e -- --nocapture; then
    CMD_PERF_EXIT_CODE=0
    CMD_PERF_STATUS="PASS"
    CMD_PERF_NOTE="command_perf_e2e passed"
  else
    CMD_PERF_EXIT_CODE=$?
    CMD_PERF_NOTE="command_perf_e2e failed"
  fi

  CMD_PERF_PASSED="$(grep -Eo '[0-9]+ passed' "$CMD_PERF_LOG" | tail -n1 | awk '{print $1}' || true)"
  CMD_PERF_FAILED="$(grep -Eo '[0-9]+ failed' "$CMD_PERF_LOG" | tail -n1 | awk '{print $1}' || true)"
  CMD_PERF_PASSED="${CMD_PERF_PASSED:-0}"
  CMD_PERF_FAILED="${CMD_PERF_FAILED:-0}"
  CMD_PERF_COUNT="$(grep -c '^LOCAL_CMD:' "$CMD_PERF_LOG" || true)"
  CMD_PERF_RSS="$(extract_metric 'PROCESS:rss_mb=[0-9.]+' "$CMD_PERF_LOG")"
}

run_commit_size_check() {
  local compare_ref
  if ! compare_ref="$(find_compare_ref)"; then
    COMMIT_STATUS="SKIP"
    COMMIT_NOTE="no upstream, main, or develop ref available for comparison"
    return
  fi

  local merge_base
  merge_base="$(git merge-base HEAD "$compare_ref")"
  COMMIT_BASE_REF="$compare_ref"
  COMMIT_BASE_SHA="$(git rev-parse --short "$merge_base")"

  mapfile -t commits < <(git rev-list "${merge_base}..HEAD")
  if [ "${#commits[@]}" -eq 0 ]; then
    COMMIT_STATUS="PASS"
    COMMIT_NOTE="no commits ahead of ${compare_ref}"
    return
  fi

  local commit
  for commit in "${commits[@]}"; do
    local parent_words
    parent_words="$(git rev-list --parents -1 "$commit" | wc -w | tr -d ' ')"
    if [ "$parent_words" -gt 2 ]; then
      continue
    fi

    local subject
    subject="$(git log --format=%s -1 "$commit")"
    if printf "%s" "$subject" | grep -qiE '^style(\(|:)'; then
      continue
    fi

    local short_sha
    short_sha="$(git rev-parse --short "$commit")"
    local stat
    stat="$(git show --format= --shortstat "$commit" 2>/dev/null || true)"
    local adds=0
    local dels=0
    local total=0

    if printf "%s" "$stat" | grep -Eq '[0-9]+ insertion'; then
      adds="$(printf "%s" "$stat" | grep -Eo '[0-9]+ insertion' | awk '{print $1}')"
    fi
    if printf "%s" "$stat" | grep -Eq '[0-9]+ deletion'; then
      dels="$(printf "%s" "$stat" | grep -Eo '[0-9]+ deletion' | awk '{print $1}')"
    fi
    total=$((adds + dels))

    COMMIT_TOTAL=$((COMMIT_TOTAL + 1))
    if [ "$total" -gt "$COMMIT_MAX" ]; then
      COMMIT_MAX="$total"
    fi

    if [ "$total" -gt 500 ]; then
      COMMIT_FAIL_COUNT=$((COMMIT_FAIL_COUNT + 1))
      printf "WARN  %s  %4d lines  %s\n" "$short_sha" "$total" "$subject" >>"$COMMIT_DETAILS_FILE"
    else
      printf "PASS  %s  %4d lines  %s\n" "$short_sha" "$total" "$subject" >>"$COMMIT_DETAILS_FILE"
    fi
  done

  if [ "$COMMIT_TOTAL" -eq 0 ]; then
    COMMIT_STATUS="SKIP"
    COMMIT_NOTE="only merge/style commits found since ${compare_ref}"
  elif [ "$COMMIT_FAIL_COUNT" -gt 0 ]; then
    COMMIT_STATUS="WARN"
    COMMIT_NOTE="${COMMIT_FAIL_COUNT} commit(s) exceed 500 changed lines"
  else
    COMMIT_STATUS="PASS"
    COMMIT_NOTE="all checked commits are within 500 changed lines"
  fi
}

run_large_file_check() {
  local tracked_files=()
  mapfile -t tracked_files < <(git ls-files "*.rs" "*.ts" "*.tsx")

  local file
  local lines
  local found=0
  for file in "${tracked_files[@]}"; do
    case "$file" in
      src/*|clawpal-core/*|clawpal-cli/*|src-tauri/*)
        ;;
      *)
        continue
        ;;
    esac

    [ -f "$file" ] || continue
    lines="$(wc -l <"$file" | tr -d ' ')"
    if [ "$lines" -gt 500 ]; then
      printf "%5d  %s\n" "$lines" "$file" >>"$LARGE_DETAILS_FILE"
      LARGE_COUNT=$((LARGE_COUNT + 1))
      found=1
    fi
  done

  if [ "$found" -eq 0 ]; then
    LARGE_STATUS="PASS"
  else
    LARGE_STATUS="WARN"
    sort -nr "$LARGE_DETAILS_FILE" -o "$LARGE_DETAILS_FILE"
  fi
}

print_report() {
  section "Local Metrics Report"
  status_line "Repo root" "$(pwd)"

  section "Hard Gates"
  status_line "Bundle gzip" "${BUNDLE_STATUS} (${BUNDLE_GZIP_KB} KB / ${BUNDLE_LIMIT_KB} KB)"
  status_line "" "raw=${BUNDLE_RAW_KB} KB init-load=${BUNDLE_INIT_GZIP_KB} KB"
  status_line "" "$BUNDLE_NOTE"

  status_line "perf_metrics" "${PERF_STATUS} (exit=${PERF_EXIT_CODE} passed=${PERF_PASSED} failed=${PERF_FAILED})"
  status_line "" "rss=${PERF_RSS_MB} MB vms=${PERF_VMS_MB} MB uptime=${PERF_UPTIME}s"
  status_line "" "cmd_p50=${PERF_CMD_P50}ms cmd_p95=${PERF_CMD_P95}ms cmd_max=${PERF_CMD_MAX}ms"
  status_line "" "$PERF_NOTE"

  status_line "command_perf_e2e" "${CMD_PERF_STATUS} (exit=${CMD_PERF_EXIT_CODE} passed=${CMD_PERF_PASSED} failed=${CMD_PERF_FAILED})"
  status_line "" "local_cmds=${CMD_PERF_COUNT} process_rss=${CMD_PERF_RSS} MB"
  status_line "" "$CMD_PERF_NOTE"

  section "Soft Gates"
  status_line "Commit size" "${COMMIT_STATUS} (${COMMIT_NOTE})"
  status_line "" "base=${COMMIT_BASE_REF} merge-base=${COMMIT_BASE_SHA} checked=${COMMIT_TOTAL} max=${COMMIT_MAX}"
  if [ -s "$COMMIT_DETAILS_FILE" ]; then
    sed 's/^/  /' "$COMMIT_DETAILS_FILE"
  fi

  status_line "Large files" "${LARGE_STATUS} (${LARGE_COUNT} file(s) over 500 lines)"
  if [ -s "$LARGE_DETAILS_FILE" ]; then
    sed 's/^/  /' "$LARGE_DETAILS_FILE"
  fi
}

cd_repo_root
require_command git

run_bundle_check
run_perf_metrics_check
run_command_perf_check
run_commit_size_check
run_large_file_check
print_report

hard_failures=()
if [ "$BUNDLE_STATUS" != "PASS" ]; then
  hard_failures+=("bundle gzip")
fi
if [ "$BUNDLE_INIT_GZIP_KB" != "N/A" ] && [ "$BUNDLE_INIT_GZIP_KB" -gt 180 ] 2>/dev/null; then
  hard_failures+=("initial-load gzip exceeds 180 KB (got ${BUNDLE_INIT_GZIP_KB} KB)")
fi
if [ "$PERF_STATUS" != "PASS" ] && [ "$PERF_STATUS" != "SKIP" ]; then
  hard_failures+=("perf_metrics")
fi
if [ "$PERF_CMD_P50" != "N/A" ] && [ "$PERF_CMD_P50" -gt 1000 ] 2>/dev/null; then
  hard_failures+=("cmd_p50 exceeds 1000 us (got ${PERF_CMD_P50} us)")
fi
if [ "$CMD_PERF_STATUS" != "PASS" ] && [ "$CMD_PERF_STATUS" != "SKIP" ]; then
  hard_failures+=("command_perf_e2e")
fi

if [ "${#hard_failures[@]}" -gt 0 ]; then
  print_log_tail "Bundle log tail" "$BUNDLE_LOG"
  print_log_tail "perf_metrics log tail" "$PERF_LOG"
  print_log_tail "command_perf_e2e log tail" "$CMD_PERF_LOG"
  printf "\nHard gate failure(s): %s\n" "${hard_failures[*]}" >&2
  exit 1
fi

echo
echo "All hard metrics gates passed."
