#!/usr/bin/env bash
set -euo pipefail

# gpu-bugfix-rate.sh — Classify commits touching the GPU layer and report
# bug-fix percentage per quarter. Feeds ADR-032 §Context's "16 of 25
# GPU-layer commits were bug fixes" claim into a continuous measurement
# rather than a one-shot snapshot.
#
# A sustained or rising bug-fix percentage strengthens the maintenance-cost
# argument for Vello adoption (decisions.md ADR-032 §Context). A declining
# percentage weakens it. The metric is intentionally crude: conventional-
# commit prefix (`fix:`, `feat:`, etc.) is the sole classifier; merges and
# unconventional messages fall into `other`.
#
# Usage:
#   tools/gpu-bugfix-rate.sh                    # since 2024-07-01, default path
#   tools/gpu-bugfix-rate.sh --since 2025-01-01
#   tools/gpu-bugfix-rate.sh --path kasane-gui/src/gpu/  # override scope
#   tools/gpu-bugfix-rate.sh --csv              # CSV instead of markdown
#
# Output: a markdown table (default) or CSV with one row per calendar
# quarter, listing commit counts by classification and the bug-fix
# percentage.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

SINCE="2024-07-01"
SCOPE_PATH="kasane-gui/src/gpu/"
FORMAT="markdown"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --since) SINCE="$2"; shift 2 ;;
    --path) SCOPE_PATH="$2"; shift 2 ;;
    --csv) FORMAT="csv"; shift ;;
    --markdown) FORMAT="markdown"; shift ;;
    -h|--help)
      sed -n '5,22p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown flag: $1" >&2; exit 1 ;;
  esac
done

# Map an ISO-8601 date (YYYY-MM-DD) to a YYYY-Q[1-4] bucket.
quarter_of() {
  local date="$1"
  local year="${date:0:4}"
  local month="${date:5:2}"
  # Strip leading zero so arithmetic comparison works.
  month=$((10#$month))
  local q
  if (( month <= 3 )); then q=1
  elif (( month <= 6 )); then q=2
  elif (( month <= 9 )); then q=3
  else q=4
  fi
  printf '%s-Q%s' "$year" "$q"
}

# Classify a commit subject line into one bucket. Uses the
# conventional-commit prefix; anything before the first `:` is
# normalised to lowercase and stripped of the `(scope)` suffix.
classify() {
  local subject="$1"
  local prefix
  prefix="$(printf '%s' "$subject" | sed -E 's/^([a-zA-Z]+)(\([^)]*\))?:.*/\1/' | tr '[:upper:]' '[:lower:]')"
  case "$prefix" in
    fix|bug) echo "fix" ;;
    feat|feature) echo "feat" ;;
    refactor) echo "refactor" ;;
    perf) echo "perf" ;;
    test|tests) echo "test" ;;
    docs|doc) echo "docs" ;;
    chore|build|ci|style) echo "chore" ;;
    *) echo "other" ;;
  esac
}

declare -A FIX FEAT REFACTOR PERF TEST DOCS CHORE OTHER TOTAL
QUARTERS=()

# Skip merge commits (`--no-merges`) — they conflate authorship and
# would otherwise inflate `other`.
while IFS='|' read -r _hash date subject; do
  [[ -z "${date:-}" ]] && continue
  q="$(quarter_of "$date")"
  if [[ -z "${TOTAL[$q]:-}" ]]; then
    QUARTERS+=("$q")
    FIX[$q]=0; FEAT[$q]=0; REFACTOR[$q]=0; PERF[$q]=0
    TEST[$q]=0; DOCS[$q]=0; CHORE[$q]=0; OTHER[$q]=0
    TOTAL[$q]=0
  fi
  klass="$(classify "$subject")"
  case "$klass" in
    fix) FIX[$q]=$((FIX[$q]+1)) ;;
    feat) FEAT[$q]=$((FEAT[$q]+1)) ;;
    refactor) REFACTOR[$q]=$((REFACTOR[$q]+1)) ;;
    perf) PERF[$q]=$((PERF[$q]+1)) ;;
    test) TEST[$q]=$((TEST[$q]+1)) ;;
    docs) DOCS[$q]=$((DOCS[$q]+1)) ;;
    chore) CHORE[$q]=$((CHORE[$q]+1)) ;;
    other) OTHER[$q]=$((OTHER[$q]+1)) ;;
  esac
  TOTAL[$q]=$((TOTAL[$q]+1))
done < <(git log --since="$SINCE" --no-merges --pretty=format:'%h|%ai|%s' -- "$SCOPE_PATH")

# Sort quarters chronologically. The YYYY-QN format sorts
# lexicographically as long as we keep the leading year.
SORTED_QUARTERS=()
while IFS= read -r q; do SORTED_QUARTERS+=("$q"); done < <(printf '%s\n' "${QUARTERS[@]}" | sort -u)

if [[ ${#SORTED_QUARTERS[@]} -eq 0 ]]; then
  echo "No commits in scope (since=$SINCE, path=$SCOPE_PATH)." >&2
  exit 0
fi

# ----------------------------------------------------------------------
# Emit
# ----------------------------------------------------------------------

emit_markdown() {
  echo "GPU-layer commit classification (path=\`$SCOPE_PATH\`, since=$SINCE)."
  echo
  echo "| Quarter | fix | feat | refactor | perf | test | docs | chore | other | total | fix% |"
  echo "|---|---|---|---|---|---|---|---|---|---|---|"
  for q in "${SORTED_QUARTERS[@]}"; do
    local total="${TOTAL[$q]}"
    local fix="${FIX[$q]}"
    local pct
    if (( total > 0 )); then
      pct="$(awk "BEGIN { printf \"%.0f\", 100 * $fix / $total }")"
    else
      pct=0
    fi
    printf '| %s | %d | %d | %d | %d | %d | %d | %d | %d | %d | %d%% |\n' \
      "$q" "$fix" "${FEAT[$q]}" "${REFACTOR[$q]}" "${PERF[$q]}" \
      "${TEST[$q]}" "${DOCS[$q]}" "${CHORE[$q]}" "${OTHER[$q]}" "$total" "$pct"
  done
}

emit_csv() {
  echo "quarter,fix,feat,refactor,perf,test,docs,chore,other,total,fix_pct"
  for q in "${SORTED_QUARTERS[@]}"; do
    local total="${TOTAL[$q]}"
    local fix="${FIX[$q]}"
    local pct
    if (( total > 0 )); then
      pct="$(awk "BEGIN { printf \"%.2f\", 100 * $fix / $total }")"
    else
      pct=0
    fi
    printf '%s,%d,%d,%d,%d,%d,%d,%d,%d,%d,%s\n' \
      "$q" "$fix" "${FEAT[$q]}" "${REFACTOR[$q]}" "${PERF[$q]}" \
      "${TEST[$q]}" "${DOCS[$q]}" "${CHORE[$q]}" "${OTHER[$q]}" "$total" "$pct"
  done
}

case "$FORMAT" in
  markdown) emit_markdown ;;
  csv) emit_csv ;;
esac
