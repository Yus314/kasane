#!/usr/bin/env bash
set -euo pipefail

# vello-trigger-status.sh — Check ADR-032 adoption-gate status against
# crates.io. Phase 1.2 (Trigger監視) automation: replace manual quarterly
# crate polling with a script that reports gate state on demand.
#
# ADR-032 §Decision lists three external triggers for re-opening the
# adoption decision:
#
#   (a) Vello ≥ 1.0 stable release
#   (b) Glifo published to crates.io ≥ 0.2
#   (c) Spike `frame_warm_24_lines` ≤ 70 µs at 80×24
#
# Implementation work (`kasane-vello-spike/src/scene_translate.rs`)
# surfaced a fourth de-facto blocker that does not appear in
# §Decision but blocks runtime W5 even with (a) and (b) clear:
#
#   (d) wgpu version alignment between vello_hybrid and the kasane
#       workspace. Until vello_hybrid bumps to wgpu 29 (or the
#       workspace downgrades), `kasane_gui::gpu::GpuState::device`
#       (wgpu_29::Device) cannot be passed to
#       `vello_hybrid::Renderer::new(device: &wgpu_28::Device, ...)`.
#
# This script automates (a), (b), and (d). Gate (c) requires a GPU
# bench environment and is reported as `spike-required`.
#
# Usage:
#   tools/vello-trigger-status.sh           # human-readable summary
#   tools/vello-trigger-status.sh --json    # machine-readable JSON
#   tools/vello-trigger-status.sh --quiet   # exit code only (for cron)
#
# Exit codes:
#   0  — all automatable gates pass (proceed to spike if (c) also met)
#   1  — at least one automatable gate fails (continue waiting)
#   2  — network failure (cannot reach crates.io)
#
# Designed to run from a cron / GitHub Actions schedule once per week or
# once per release window. Output suitable for piping into a roadmap
# update or a notification webhook.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

FORMAT="human"
USER_AGENT="kasane-trigger-status (https://github.com/Yus314/kasane)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --json) FORMAT="json"; shift ;;
    --quiet) FORMAT="quiet"; shift ;;
    --human) FORMAT="human"; shift ;;
    -h|--help)
      sed -n '5,28p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown flag: $1" >&2; exit 1 ;;
  esac
done

# Pipe-delimited row accessor used throughout the script. Each
# `fetch_crate` row is `<stable>|<max>|<updated>|<status>`; this
# helper extracts a single field by 1-based index.
extract_field() {
  local row="$1"
  local idx="$2"
  printf '%s' "$row" | awk -F'|' -v i="$idx" '{ print $i }'
}

# Look up `crate` via the crates.io API. Echoes
# `<max_stable_version>|<max_version>|<updated_at>|<status>`. Status is
# `ok`, `unpublished` (404 — crate name not registered), or `error`
# (network / parse failure). On `unpublished` and `error`, the version
# fields are empty.
fetch_crate() {
  local crate="$1"
  local resp
  local http_status
  http_status="$(curl -s -A "$USER_AGENT" -o /tmp/kasane-crates-resp -w '%{http_code}' \
    "https://crates.io/api/v1/crates/$crate" 2>/dev/null || echo "000")"
  case "$http_status" in
    200)
      resp="$(cat /tmp/kasane-crates-resp)"
      local stable max updated
      stable="$(printf '%s' "$resp" | jq -r '.crate.max_stable_version // ""' 2>/dev/null || echo "")"
      max="$(printf '%s' "$resp" | jq -r '.crate.max_version // ""' 2>/dev/null || echo "")"
      updated="$(printf '%s' "$resp" | jq -r '.crate.updated_at // ""' 2>/dev/null || echo "")"
      printf '%s|%s|%s|ok\n' "$stable" "$max" "$updated"
      ;;
    404)
      printf '|||unpublished\n'
      ;;
    *)
      printf '|||error\n'
      ;;
  esac
  rm -f /tmp/kasane-crates-resp
}

# Compare a semver string ($1) against `major.minor` ($2). Echoes 1 iff
# version ≥ threshold, else 0. Pre-release suffixes (-alpha, -rc.1, etc.)
# disqualify the version per ADR-032 "Vello ≥ 1.0 stable" wording.
version_ge() {
  local version="$1"
  local threshold="$2"
  if [[ -z "$version" ]]; then echo 0; return; fi
  if [[ "$version" == *-* ]]; then echo 0; return; fi
  local v_major v_minor t_major t_minor
  v_major="${version%%.*}"
  v_minor="${version#*.}"; v_minor="${v_minor%%.*}"
  t_major="${threshold%%.*}"
  t_minor="${threshold#*.}"
  if (( v_major > t_major )); then echo 1; return; fi
  if (( v_major < t_major )); then echo 0; return; fi
  if (( v_minor >= t_minor )); then echo 1; else echo 0; fi
}

# ----------------------------------------------------------------------
# Probe each crate
# ----------------------------------------------------------------------

VELLO="$(fetch_crate vello)"
VELLO_HYBRID="$(fetch_crate vello_hybrid)"
GLIFO="$(fetch_crate glifo)"
PARLEY="$(fetch_crate parley)"
PENIKO="$(fetch_crate peniko)"
# vello_common 0.0.7 hosts `glyph::GlyphCaches` + `peniko::FontData`,
# the load-bearing primitives for `Scene::glyph_run`. Tracked because
# `scene_translate.rs` Finding 3 proposes that gate (b) "Glifo on
# crates.io" can be reframed as "vello_common::glyph compatibility"
# — making this crate a candidate driver of gate (b)'s real verdict.
VELLO_COMMON="$(fetch_crate vello_common)"

# vello_hybrid's wgpu requirement, scraped from the crates.io
# dependencies endpoint for the latest stable. Echoes the major
# version (e.g. "28") or empty on fetch failure / non-stable response.
fetch_vello_hybrid_wgpu_major() {
  local stable
  stable="$(extract_field "$VELLO_HYBRID" 1)"
  [[ -z "$stable" ]] && return
  local req
  req="$(curl -s -A "$USER_AGENT" \
    "https://crates.io/api/v1/crates/vello_hybrid/$stable/dependencies" 2>/dev/null \
    | jq -r '.dependencies[]? | select(.crate_id == "wgpu") | .req' 2>/dev/null \
    | head -n1)"
  # Extract the first integer ("^28.0.0" → 28; "28" → 28).
  printf '%s' "$req" | grep -oE '[0-9]+' | head -n1
}

# Workspace wgpu version, parsed from the repo-root Cargo.toml.
# Echoes the major version (e.g. "29") or empty if the line is
# missing.
fetch_workspace_wgpu_major() {
  local cargo_toml="$ROOT_DIR/Cargo.toml"
  [[ -f "$cargo_toml" ]] || return
  # Match either `wgpu = "29"` or `wgpu = { version = "29", ... }`.
  local line
  line="$(grep -E '^[[:space:]]*wgpu[[:space:]]*=' "$cargo_toml" | head -n1)"
  printf '%s' "$line" | grep -oE '[0-9]+' | head -n1
}

VELLO_HYBRID_WGPU_MAJOR="$(fetch_vello_hybrid_wgpu_major)"
WORKSPACE_WGPU_MAJOR="$(fetch_workspace_wgpu_major)"

# Detect total network failure (every crate returned `error`).
if [[ "${VELLO##*|}" == "error" && "${VELLO_HYBRID##*|}" == "error" \
   && "${GLIFO##*|}" == "error" && "${PARLEY##*|}" == "error" \
   && "${PENIKO##*|}" == "error" ]]; then
  if [[ "$FORMAT" != "quiet" ]]; then
    echo "ERROR: cannot reach crates.io (network failure)" >&2
  fi
  exit 2
fi

# Gate (a): Vello ≥ 1.0 stable.
VELLO_STABLE="${VELLO%%|*}"
GATE_A="$(version_ge "$VELLO_STABLE" "1.0")"

# Gate (b): Glifo on crates.io with max_stable ≥ 0.2.
GLIFO_STATUS="${GLIFO##*|}"
GLIFO_STABLE="${GLIFO%%|*}"
if [[ "$GLIFO_STATUS" == "ok" ]]; then
  GATE_B="$(version_ge "$GLIFO_STABLE" "0.2")"
else
  GATE_B=0
fi

# Gate (c): spike-only, never automatable.
GATE_C="spike-required"

# Gate (d): wgpu version alignment between vello_hybrid and the
# kasane workspace. PASS iff both major versions are non-empty and
# match. Reported as UNKNOWN if either fetch failed (network /
# missing Cargo.toml).
if [[ -n "$VELLO_HYBRID_WGPU_MAJOR" && -n "$WORKSPACE_WGPU_MAJOR" ]]; then
  if [[ "$VELLO_HYBRID_WGPU_MAJOR" == "$WORKSPACE_WGPU_MAJOR" ]]; then
    GATE_D=1
  else
    GATE_D=0
  fi
else
  GATE_D=-1
fi

# Overall status: pass iff (a) AND (b) AND (d). (c) cannot be
# auto-passed and is reported as `spike-required`. Exit 0 means
# "ready to schedule the spike" (or run it if (a) and (b) clear and
# (d) aligns at adoption time); the spike still runs before adoption
# is committed.
if (( GATE_A == 1 && GATE_B == 1 && GATE_D == 1 )); then
  OVERALL=0
else
  OVERALL=1
fi

# ----------------------------------------------------------------------
# Output
# ----------------------------------------------------------------------

emit_human() {
  echo "ADR-032 adoption-gate status (probed: $(date -u +'%Y-%m-%dT%H:%M:%SZ'))"
  echo
  printf '  %-14s %-12s %-10s %s\n' "crate" "stable" "max" "updated"
  printf '  %-14s %-12s %-10s %s\n' "----" "----" "---" "-------"
  for row_pair in \
    "vello|$VELLO" \
    "vello_hybrid|$VELLO_HYBRID" \
    "vello_common|$VELLO_COMMON" \
    "glifo|$GLIFO" \
    "parley|$PARLEY" \
    "peniko|$PENIKO"; do
    local name="${row_pair%%|*}"
    local row="${row_pair#*|}"
    local stable max updated status
    stable="$(extract_field "$row" 1)"; max="$(extract_field "$row" 2)"
    updated="$(extract_field "$row" 3)"; status="$(extract_field "$row" 4)"
    case "$status" in
      ok)          printf '  %-14s %-12s %-10s %s\n' "$name" "${stable:-—}" "${max:-—}" "${updated%%T*}" ;;
      unpublished) printf '  %-14s %-12s %-10s %s\n' "$name" "(404)" "—" "not on crates.io" ;;
      error)       printf '  %-14s %-12s %-10s %s\n' "$name" "(error)" "—" "fetch failed" ;;
    esac
  done
  echo
  echo "Adoption gates:"
  if (( GATE_A == 1 )); then
    echo "  (a) Vello ≥ 1.0 stable          PASS  (current: $VELLO_STABLE)"
  else
    echo "  (a) Vello ≥ 1.0 stable          WAIT  (current: ${VELLO_STABLE:-none})"
  fi
  if (( GATE_B == 1 )); then
    echo "  (b) Glifo ≥ 0.2 on crates.io    PASS  (current: $GLIFO_STABLE)"
  else
    case "$GLIFO_STATUS" in
      unpublished) echo "  (b) Glifo ≥ 0.2 on crates.io    WAIT  (not yet published)" ;;
      ok)          echo "  (b) Glifo ≥ 0.2 on crates.io    WAIT  (current: ${GLIFO_STABLE:-pre-release})" ;;
      *)           echo "  (b) Glifo ≥ 0.2 on crates.io    WAIT  (fetch failed)" ;;
    esac
  fi
  echo "  (c) spike frame_warm ≤ 70 µs    SPIKE (run W5 once (a)+(b)+(d) pass)"
  case "$GATE_D" in
    1)  echo "  (d) wgpu version alignment      PASS  (vello_hybrid wgpu=$VELLO_HYBRID_WGPU_MAJOR, workspace wgpu=$WORKSPACE_WGPU_MAJOR)" ;;
    0)  echo "  (d) wgpu version alignment      WAIT  (vello_hybrid wgpu=$VELLO_HYBRID_WGPU_MAJOR, workspace wgpu=$WORKSPACE_WGPU_MAJOR — Finding 1)" ;;
    -1) echo "  (d) wgpu version alignment      UNKNOWN (could not detect one or both versions)" ;;
  esac
  echo
  if (( OVERALL == 0 )); then
    echo "Status: schedule W5 spike (gates (a), (b), (d) clear)"
  else
    echo "Status: continue waiting"
  fi
}

emit_json() {
  jq -n \
    --arg probed "$(date -u +'%Y-%m-%dT%H:%M:%SZ')" \
    --arg vello_stable "$(extract_field "$VELLO" 1)" \
    --arg vello_max "$(extract_field "$VELLO" 2)" \
    --arg vello_updated "$(extract_field "$VELLO" 3)" \
    --arg vello_hybrid_stable "$(extract_field "$VELLO_HYBRID" 1)" \
    --arg vello_hybrid_max "$(extract_field "$VELLO_HYBRID" 2)" \
    --arg vello_hybrid_updated "$(extract_field "$VELLO_HYBRID" 3)" \
    --arg glifo_stable "$(extract_field "$GLIFO" 1)" \
    --arg glifo_status "$(extract_field "$GLIFO" 4)" \
    --arg parley_stable "$(extract_field "$PARLEY" 1)" \
    --arg parley_updated "$(extract_field "$PARLEY" 3)" \
    --arg peniko_stable "$(extract_field "$PENIKO" 1)" \
    --arg peniko_updated "$(extract_field "$PENIKO" 3)" \
    --arg vello_common_stable "$(extract_field "$VELLO_COMMON" 1)" \
    --arg vello_common_status "$(extract_field "$VELLO_COMMON" 4)" \
    --arg vh_wgpu_major "${VELLO_HYBRID_WGPU_MAJOR:-}" \
    --arg ws_wgpu_major "${WORKSPACE_WGPU_MAJOR:-}" \
    --argjson gate_a "$GATE_A" \
    --argjson gate_b "$GATE_B" \
    --arg gate_c "$GATE_C" \
    --argjson gate_d "$GATE_D" \
    --argjson overall_pass "$(if (( OVERALL == 0 )); then echo true; else echo false; fi)" \
    '{
      probed: $probed,
      crates: {
        vello: { stable: $vello_stable, max: $vello_max, updated: $vello_updated },
        vello_hybrid: { stable: $vello_hybrid_stable, max: $vello_hybrid_max, updated: $vello_hybrid_updated },
        vello_common: { stable: $vello_common_stable, status: $vello_common_status },
        glifo: { stable: $glifo_stable, status: $glifo_status },
        parley: { stable: $parley_stable, updated: $parley_updated },
        peniko: { stable: $peniko_stable, updated: $peniko_updated }
      },
      wgpu_alignment: {
        vello_hybrid_major: $vh_wgpu_major,
        workspace_major: $ws_wgpu_major
      },
      gates: {
        a_vello_stable_ge_1_0: ($gate_a == 1),
        b_glifo_published_ge_0_2: ($gate_b == 1),
        c_spike_frame_warm_le_70us: $gate_c,
        d_wgpu_version_aligned: (if $gate_d == 1 then true elif $gate_d == 0 then false else "unknown" end)
      },
      overall_automatable_pass: $overall_pass
    }'
}

case "$FORMAT" in
  human) emit_human ;;
  json) emit_json ;;
  quiet) ;;
esac

exit "$OVERALL"
