#!/usr/bin/env bash
set -euo pipefail

# update-measurements.sh — Replace <!-- BENCH:name --> markers in docs/performance.md
# with auto-generated benchmark tables from criterion JSON data.
#
# Usage:
#   tools/update-measurements.sh              # Update all sections
#   tools/update-measurements.sh --only slo,alloc_breakdown
#   tools/update-measurements.sh --check      # CI: exit 1 if stale
#   tools/update-measurements.sh --dry-run    # Print diff without writing

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFIG="$SCRIPT_DIR/bench-sections.json"
TARGET="$ROOT_DIR/$(jq -r '.meta.target_file' "$CONFIG")"
CRITERION_DIR="$ROOT_DIR/$(jq -r '.meta.criterion_dir' "$CONFIG")"
ALLOC_PATH="$ROOT_DIR/$(jq -r '.meta.alloc_budget_path' "$CONFIG")"

# --- Flags ---
ONLY=""
CHECK=false
DRY_RUN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --only) ONLY="$2"; shift 2 ;;
    --check) CHECK=true; shift ;;
    --dry-run) DRY_RUN=true; shift ;;
    *) echo "Unknown flag: $1" >&2; exit 1 ;;
  esac
done

# --- Core functions ---

read_criterion_ns() {
  local bench="$1"
  # bench path uses / as separator, maps to nested directories
  local path="$CRITERION_DIR/$bench/new/estimates.json"
  if [[ ! -f "$path" ]]; then
    echo "ERROR: Missing criterion data: $path" >&2
    return 1
  fi
  jq -r '.median.point_estimate' "$path"
}

# Convert nanoseconds to target unit, formatted
ns_to_display() {
  local ns="$1" unit="$2"
  case "$unit" in
    ns) jq -n --argjson v "$ns" '$v | . + 0.05 | . * 10 | floor / 10' | awk '{printf "%.1f", $1}' ;;
    us) jq -n --argjson v "$ns" '$v / 1000' | awk '{printf "%.1f", $1}' ;;
    ms) jq -n --argjson v "$ns" '$v / 1000000' | awk '{printf "%.2f", $1}' ;;
  esac
}

# Format value with unit suffix
format_val() {
  local ns="$1" unit="$2"
  local val
  val=$(ns_to_display "$ns" "$unit")
  echo "${val} ${unit}"
}

# Read alloc_budget.json field (dot-path like "full_frame.count")
read_alloc() {
  local key="$1"
  if [[ ! -f "$ALLOC_PATH" ]]; then
    echo "ERROR: Missing alloc_budget.json: $ALLOC_PATH" >&2
    return 1
  fi
  jq -r ".$key" "$ALLOC_PATH"
}

# Compute headroom ratio: threshold / measured
headroom() {
  local threshold="$1" measured="$2"
  jq -n --argjson t "$threshold" --argjson m "$measured" '
    if $m == 0 then "∞"
    else ($t / $m * 10 | floor / 10 | tostring) + "×"
    end
  ' | tr -d '"'
}

# Compute percentage difference: (a - b) / b * 100
pct_diff() {
  local a="$1" b="$2"
  jq -n --argjson a "$a" --argjson b "$b" '
    if $b == 0 then "N/A"
    else
      (($a - $b) / $b * 100) as $pct |
      if $pct > 0 then "+" + ($pct | . * 10 | round / 10 | tostring) + "%"
      else ($pct | . * 10 | round / 10 | tostring) + "%"
      end
    end
  ' | tr -d '"'
}

# Compute ratio: a / b
ratio() {
  local a="$1" b="$2"
  jq -n --argjson a "$a" --argjson b "$b" '
    if $b == 0 then "N/A"
    else ($a / $b * 10 | round / 10 | tostring) + "x"
    end
  ' | tr -d '"'
}

# Compute percentage of full_frame
pct_of() {
  local val="$1" total="$2"
  jq -n --argjson v "$val" --argjson t "$total" '
    if $t == 0 then "N/A"
    else ($v / $t * 1000 | round / 10 | tostring) + "%"
    end
  ' | tr -d '"'
}

# --- Section should be processed? ---
should_process() {
  local name="$1"
  if [[ -z "$ONLY" ]]; then
    return 0
  fi
  echo ",$ONLY," | grep -q ",$name,"
}

# --- Marker replacement ---
replace_marker() {
  local name="$1" content="$2" file="$3"
  local open_tag="<!-- BENCH:${name} -->"
  local close_tag="<!-- /BENCH:${name} -->"
  awk -v otag="$open_tag" -v ctag="$close_tag" -v content="$content" '
    index($0, otag)  { print; printf "%s\n", content; skip=1; next }
    index($0, ctag)  { skip=0 }
    skip { next }
    { print }
  ' "$file" > "${file}.tmp" && mv "${file}.tmp" "$file"
}

# --- Section generators ---

generate_slo() {
  local n
  n=$(jq '.sections.slo.metrics | length' "$CONFIG")
  local table=""
  table+="| Metric | SLO | Current | Headroom |"$'\n'
  table+="|---|---|---|---|"

  local i=0
  while [[ $i -lt $n ]]; do
    local label slo threshold unit
    label=$(jq -r ".sections.slo.metrics[$i].label" "$CONFIG")
    slo=$(jq -r ".sections.slo.metrics[$i].slo" "$CONFIG")
    threshold=$(jq -r ".sections.slo.metrics[$i].threshold" "$CONFIG")
    unit=$(jq -r ".sections.slo.metrics[$i].unit" "$CONFIG")

    local current_display hdroom
    # Check if this uses alloc_key or bench
    local alloc_key
    alloc_key=$(jq -r ".sections.slo.metrics[$i].alloc_key // empty" "$CONFIG")

    if [[ -n "$alloc_key" ]]; then
      local count
      count=$(read_alloc "$alloc_key") || return 1
      current_display="$count"
      hdroom=$(headroom "$threshold" "$count")
    else
      local bench ns val_num
      bench=$(jq -r ".sections.slo.metrics[$i].bench" "$CONFIG")
      ns=$(read_criterion_ns "$bench") || return 1
      current_display=$(format_val "$ns" "$unit")
      # Convert to unit for headroom comparison
      val_num=$(ns_to_display "$ns" "$unit")
      hdroom=$(headroom "$threshold" "$val_num")
    fi

    table+=$'\n'"| $label | $slo | $current_display | $hdroom |"
    i=$((i + 1))
  done
  echo "$table"
}

generate_alloc() {
  if [[ ! -f "$ALLOC_PATH" ]]; then
    echo "ERROR: Missing alloc_budget.json: $ALLOC_PATH" >&2
    return 1
  fi

  local phases total_key extra_key extra_label
  phases=$(jq -r '.sections.alloc_breakdown.phases[]' "$CONFIG")
  total_key=$(jq -r '.sections.alloc_breakdown.total' "$CONFIG")
  extra_key=$(jq -r '.sections.alloc_breakdown.extra.key' "$CONFIG")
  extra_label=$(jq -r '.sections.alloc_breakdown.extra.label' "$CONFIG")

  local table=""
  table+="| Phase | Alloc Count | Bytes | Notes |"$'\n'
  table+="|---|---|---|---|"

  local phase_notes
  phase_notes=$(cat <<'NOTES'
view=Element tree construction
place=Layout result vectors
paint=Atom-to-cell conversion
diff=CellDiff vector + Cell clones
swap=Previous buffer allocation
NOTES
  )

  for phase in $phases; do
    local count bytes note=""
    count=$(jq -r ".$phase.count" "$ALLOC_PATH")
    bytes=$(jq -r ".$phase.bytes" "$ALLOC_PATH")
    note=$(echo "$phase_notes" | grep "^${phase}=" | cut -d= -f2- || true)
    local bytes_fmt
    bytes_fmt=$(jq -n --argjson b "$bytes" '$b | tostring | explode | reverse | [range(0; length)] | map(. as $i | if $i > 0 and $i % 3 == 0 then [44, (input_line_number // 0)] else [] end + [.]) | [.[][]] | reverse | implode' 2>/dev/null || echo "$bytes")
    # Simpler approach: just use the raw number
    table+=$'\n'"| $phase | $count | $bytes | $note |"
  done

  # Total
  local total_count total_bytes
  total_count=$(jq -r ".$total_key.count" "$ALLOC_PATH")
  total_bytes=$(jq -r ".$total_key.bytes" "$ALLOC_PATH")
  table+=$'\n'"| **${total_key} total** | **$total_count** | **$total_bytes** | |"

  # Extra (parse_request)
  local extra_count extra_bytes
  extra_count=$(jq -r ".$extra_key.count" "$ALLOC_PATH")
  extra_bytes=$(jq -r ".$extra_key.bytes" "$ALLOC_PATH")
  table+=$'\n'"| $extra_label | $extra_count | $extra_bytes | JSON parsing dominates |"

  echo "$table"
}

generate_bench_table() {
  local section="$1"
  local type
  type=$(jq -r ".sections.$section.type" "$CONFIG")

  local ncols
  ncols=$(jq ".sections.$section.columns | length" "$CONFIG")
  local nrows
  nrows=$(jq ".sections.$section.rows | length" "$CONFIG")

  # Build header
  local header="" sep=""
  local c=0
  while [[ $c -lt $ncols ]]; do
    local col
    col=$(jq -r ".sections.$section.columns[$c]" "$CONFIG")
    header+="| $col "
    sep+="|---"
    c=$((c + 1))
  done
  header+="|"
  sep+="|"

  local table=""
  table+="$header"$'\n'
  table+="$sep"

  local full_frame_ns=""
  # For bench_table_vs type, read full_frame benchmark
  if [[ "$type" == "bench_table_vs" ]]; then
    local ff_bench
    ff_bench=$(jq -r ".sections.$section.full_frame_bench" "$CONFIG")
    full_frame_ns=$(read_criterion_ns "$ff_bench") || return 1
  fi

  local i=0
  while [[ $i -lt $nrows ]]; do
    local bench unit label note
    bench=$(jq -r ".sections.$section.rows[$i].bench" "$CONFIG")
    unit=$(jq -r ".sections.$section.rows[$i].unit" "$CONFIG")
    label=$(jq -r ".sections.$section.rows[$i].label // empty" "$CONFIG")
    note=$(jq -r ".sections.$section.rows[$i].note // empty" "$CONFIG")

    local ns val_display
    ns=$(read_criterion_ns "$bench") || return 1
    val_display=$(format_val "$ns" "$unit")

    # Use label if provided, else format bench name
    local row_label
    if [[ -n "$label" ]]; then
      row_label="$label"
    else
      row_label="\`$bench\`"
    fi

    local row=""
    case "$type" in
      bench_table)
        # Dynamically build based on column count
        local what target target_val verdict
        what=$(jq -r ".sections.$section.rows[$i].what // empty" "$CONFIG")
        target=$(jq -r ".sections.$section.rows[$i].target // empty" "$CONFIG")
        target_val=$(jq -r ".sections.$section.rows[$i].target_val // empty" "$CONFIG")

        # Check for derived note
        local derived_note
        derived_note=$(jq -r ".sections.$section.rows[$i].derived_note // empty" "$CONFIG")

        local note_col="$note"
        if [[ -n "$derived_note" && "$derived_note" != "" ]]; then
          local dn_base dn_sub dn_suffix
          dn_base=$(jq -r ".sections.$section.rows[$i].derived_note.base" "$CONFIG")
          dn_sub=$(jq -r ".sections.$section.rows[$i].derived_note.sub" "$CONFIG")
          dn_suffix=$(jq -r ".sections.$section.rows[$i].derived_note.suffix" "$CONFIG")
          local ns_base ns_sub derived_ns derived_display
          ns_base=$(read_criterion_ns "$dn_base") || return 1
          ns_sub=$(read_criterion_ns "$dn_sub") || return 1
          derived_ns=$(jq -n --argjson a "$ns_base" --argjson b "$ns_sub" '$a - $b')
          derived_display=$(format_val "$derived_ns" "$unit")
          note_col="${dn_suffix}: ${derived_display}"
        fi

        if [[ -n "$target_val" && "$target" != "--" ]]; then
          local val_num
          val_num=$(ns_to_display "$ns" "$unit")
          local headroom_x
          headroom_x=$(headroom "$target_val" "$val_num")
          verdict="OK (${headroom_x} headroom)"
        else
          verdict=""
        fi

        case $ncols in
          5) row="| \`$bench\` | $what | $target | $val_display | $verdict${note_col:+ $note_col} |" ;;
          4)
            if [[ -n "$what" ]]; then
              row="| \`$bench\` | $what | $val_display | $note_col |"
            else
              row="| $row_label | $val_display | $note_col |"
            fi
            ;;
          3) row="| \`$bench\` | $val_display | $note_col |" ;;
          *) row="| \`$bench\` | $val_display |" ;;
        esac
        ;;

      bench_table_vs)
        local pct
        pct=$(pct_of "$ns" "$full_frame_ns")
        row="| $row_label | **$val_display** | **$pct** | $note |"
        ;;

      *)
        row="| \`$bench\` | $val_display | $note |"
        ;;
    esac

    table+=$'\n'"$row"
    i=$((i + 1))
  done

  echo "$table"
}

generate_replay() {
  local nrows
  nrows=$(jq '.sections.replay.rows | length' "$CONFIG")

  local table=""
  table+="| Scenario | Messages | Measured | Per-message |"$'\n'
  table+="|---|---|---|---|"

  local i=0
  while [[ $i -lt $nrows ]]; do
    local bench label messages
    bench=$(jq -r ".sections.replay.rows[$i].bench" "$CONFIG")
    label=$(jq -r ".sections.replay.rows[$i].label" "$CONFIG")
    messages=$(jq -r ".sections.replay.rows[$i].messages" "$CONFIG")

    local ns
    ns=$(read_criterion_ns "$bench") || return 1
    local total_display per_msg_display
    # Replay benchmarks measure total scenario time
    total_display=$(format_val "$ns" "ms")
    # Per-message = total_ns / messages → in μs
    local per_msg_ns
    per_msg_ns=$(jq -n --argjson ns "$ns" --argjson m "$messages" '$ns / $m')
    per_msg_display=$(format_val "$per_msg_ns" "us")

    table+=$'\n'"| $label | $messages | $total_display | $per_msg_display |"
    i=$((i + 1))
  done

  echo "$table"
}

generate_paired() {
  local section="$1"
  local npairs
  npairs=$(jq ".sections.$section.pairs | length" "$CONFIG")

  local ncols
  ncols=$(jq ".sections.$section.columns | length" "$CONFIG")

  # Build header
  local header="" sep=""
  local c=0
  while [[ $c -lt $ncols ]]; do
    local col
    col=$(jq -r ".sections.$section.columns[$c]" "$CONFIG")
    header+="| $col "
    sep+="|---"
    c=$((c + 1))
  done
  header+="|"
  sep+="|"

  local table=""
  table+="$header"$'\n'
  table+="$sep"

  local i=0
  while [[ $i -lt $npairs ]]; do
    local label a_bench b_bench unit
    label=$(jq -r ".sections.$section.pairs[$i].label" "$CONFIG")
    a_bench=$(jq -r ".sections.$section.pairs[$i].a" "$CONFIG")
    b_bench=$(jq -r ".sections.$section.pairs[$i].b" "$CONFIG")
    unit=$(jq -r ".sections.$section.pairs[$i].unit" "$CONFIG")

    local a_ns b_ns a_display b_display diff
    a_ns=$(read_criterion_ns "$a_bench") || return 1
    b_ns=$(read_criterion_ns "$b_bench") || return 1
    a_display=$(format_val "$a_ns" "$unit")
    b_display=$(format_val "$b_ns" "$unit")
    diff=$(pct_diff "$a_ns" "$b_ns")

    # Bold the faster one
    if jq -n --argjson a "$a_ns" --argjson b "$b_ns" '$a < $b' | grep -q true; then
      a_display="**$a_display**"
    elif jq -n --argjson a "$a_ns" --argjson b "$b_ns" '$b < $a' | grep -q true; then
      b_display="**$b_display**"
    fi

    table+=$'\n'"| $label | $a_display | $b_display | $diff |"
    i=$((i + 1))
  done

  echo "$table"
}

generate_wasm_cm() {
  local npairs
  npairs=$(jq '.sections.wasm_cm.pairs | length' "$CONFIG")

  local table=""
  table+="| Function | Raw Module | Component Model | Ratio | Notes |"$'\n'
  table+="|---|---|---|---|---|"

  local i=0
  while [[ $i -lt $npairs ]]; do
    local label raw_bench cm_bench unit
    label=$(jq -r ".sections.wasm_cm.pairs[$i].label" "$CONFIG")
    raw_bench=$(jq -r ".sections.wasm_cm.pairs[$i].raw" "$CONFIG")
    cm_bench=$(jq -r ".sections.wasm_cm.pairs[$i].cm" "$CONFIG")
    unit=$(jq -r ".sections.wasm_cm.pairs[$i].unit" "$CONFIG")

    local raw_ns cm_ns raw_display cm_display r
    raw_ns=$(read_criterion_ns "$raw_bench") || return 1
    cm_ns=$(read_criterion_ns "$cm_bench") || return 1
    raw_display=$(format_val "$raw_ns" "$unit")
    cm_display=$(format_val "$cm_ns" "$unit")
    r=$(ratio "$cm_ns" "$raw_ns")

    # Compute overhead in ns
    local overhead_ns overhead_note
    overhead_ns=$(jq -n --argjson c "$cm_ns" --argjson r "$raw_ns" '$c - $r')
    if jq -n --argjson o "$overhead_ns" '$o > 1000' | grep -q true; then
      local overhead_us
      overhead_us=$(ns_to_display "$overhead_ns" "us")
      overhead_note="~${overhead_us} μs canonical ABI overhead"
    else
      local overhead_disp
      overhead_disp=$(ns_to_display "$overhead_ns" "ns")
      overhead_note="~${overhead_disp} ns overhead"
    fi

    table+=$'\n'"| $label | $raw_display | **$cm_display** | ${r} | $overhead_note |"
    i=$((i + 1))
  done

  echo "$table"
}

generate_wasm_native() {
  local npairs
  npairs=$(jq '.sections.wasm_native.pairs | length' "$CONFIG")

  local table=""
  table+="| Operation | Native | WASM (CM) | Ratio | Notes |"$'\n'
  table+="|---|---|---|---|---|"

  local i=0
  while [[ $i -lt $npairs ]]; do
    local label native_bench wasm_bench unit note
    label=$(jq -r ".sections.wasm_native.pairs[$i].label" "$CONFIG")
    native_bench=$(jq -r ".sections.wasm_native.pairs[$i].native" "$CONFIG")
    wasm_bench=$(jq -r ".sections.wasm_native.pairs[$i].wasm" "$CONFIG")
    unit=$(jq -r ".sections.wasm_native.pairs[$i].unit" "$CONFIG")
    note=$(jq -r ".sections.wasm_native.pairs[$i].note // empty" "$CONFIG")

    local native_ns wasm_ns native_display wasm_display r
    native_ns=$(read_criterion_ns "$native_bench") || return 1
    wasm_ns=$(read_criterion_ns "$wasm_bench") || return 1
    native_display=$(format_val "$native_ns" "$unit")
    wasm_display=$(format_val "$wasm_ns" "$unit")
    r=$(ratio "$wasm_ns" "$native_ns")

    table+=$'\n'"| $label | $native_display | $wasm_display | ${r} | $note |"
    i=$((i + 1))
  done

  echo "$table"
}

generate_per_frame_cost() {
  local nrows
  nrows=$(jq '.sections.per_frame_cost.rows | length' "$CONFIG")

  local table=""
  table+="| Phase | Complexity | Measured | Notes |"$'\n'
  table+="|---|---|---|---|"

  local i=0
  while [[ $i -lt $nrows ]]; do
    local label complexity unit note bench
    label=$(jq -r ".sections.per_frame_cost.rows[$i].label" "$CONFIG")
    complexity=$(jq -r ".sections.per_frame_cost.rows[$i].complexity" "$CONFIG")
    unit=$(jq -r ".sections.per_frame_cost.rows[$i].unit" "$CONFIG")
    note=$(jq -r ".sections.per_frame_cost.rows[$i].note // empty" "$CONFIG")
    bench=$(jq -r ".sections.per_frame_cost.rows[$i].bench // empty" "$CONFIG")
    local bold
    bold=$(jq -r ".sections.per_frame_cost.rows[$i].bold // false" "$CONFIG")

    local val_display=""

    # Check for bench_range (min-max display)
    local bench_range
    bench_range=$(jq -r ".sections.per_frame_cost.rows[$i].bench_range // empty" "$CONFIG")

    if [[ -n "$bench_range" && "$bench_range" != "" ]]; then
      local bench_lo bench_hi ns_lo ns_hi
      bench_lo=$(jq -r ".sections.per_frame_cost.rows[$i].bench_range[0]" "$CONFIG")
      bench_hi=$(jq -r ".sections.per_frame_cost.rows[$i].bench_range[1]" "$CONFIG")
      ns_lo=$(read_criterion_ns "$bench_lo") || return 1
      ns_hi=$(read_criterion_ns "$bench_hi") || return 1
      local lo_display hi_display
      lo_display=$(ns_to_display "$ns_lo" "$unit")
      hi_display=$(ns_to_display "$ns_hi" "$unit")
      val_display="**${lo_display}-${hi_display} ${unit}**"
    elif [[ -n "$bench" ]]; then
      # Check for derived (subtract)
      local derived
      derived=$(jq -r ".sections.per_frame_cost.rows[$i].derived // empty" "$CONFIG")

      if [[ -n "$derived" && "$derived" != "" ]]; then
        local base_bench sub_bench ns_base ns_sub ns_derived
        base_bench=$(jq -r ".sections.per_frame_cost.rows[$i].derived.base" "$CONFIG")
        sub_bench=$(jq -r ".sections.per_frame_cost.rows[$i].derived.sub" "$CONFIG")
        ns_base=$(read_criterion_ns "$base_bench") || return 1
        ns_sub=$(read_criterion_ns "$sub_bench") || return 1
        ns_derived=$(jq -n --argjson a "$ns_base" --argjson b "$ns_sub" '$a - $b')
        val_display=$(format_val "$ns_derived" "$unit")
      else
        local ns
        ns=$(read_criterion_ns "$bench") || return 1
        val_display=$(format_val "$ns" "$unit")
      fi

      # Check for note_tpl (template with two values)
      local note_tpl
      note_tpl=$(jq -r ".sections.per_frame_cost.rows[$i].col_measured // empty" "$CONFIG")
      if [[ "$note_tpl" == "note_tpl" ]]; then
        local bench2 ns1 ns2 v1 v2
        bench2=$(jq -r ".sections.per_frame_cost.rows[$i].bench2" "$CONFIG")
        ns1=$(read_criterion_ns "$bench") || return 1
        ns2=$(read_criterion_ns "$bench2") || return 1
        v1=$(ns_to_display "$ns1" "$unit")
        v2=$(ns_to_display "$ns2" "$unit")
        val_display="${v1} ${unit} (0 plugins) / ${v2} ${unit} (10 plugins)"
      fi

      if [[ "$bold" == "true" ]]; then
        val_display="**${val_display}**"
      fi
    fi

    # Build note string, checking for note suffix from row config
    local note_suffix
    note_suffix=$(jq -r ".sections.per_frame_cost.rows[$i].note_suffix // empty" "$CONFIG")
    local full_note="$note"
    if [[ -n "$note_suffix" ]]; then
      full_note="${full_note}${note_suffix}"
    fi

    # For view() row, add special note
    if [[ "$label" == '`view()`' ]]; then
      full_note="Element tree + plugin contributions + Salsa sync"
    fi

    table+=$'\n'"| $label | $complexity | $val_display | $full_note |"
    i=$((i + 1))
  done

  echo "$table"
}

# --- Main ---

ERRORS=0
UPDATED=0

process_section() {
  local name="$1"
  should_process "$name" || return 0

  local type
  type=$(jq -r ".sections.$name.type" "$CONFIG")
  local content=""

  case "$type" in
    slo)            content=$(generate_slo) ;;
    alloc)          content=$(generate_alloc) ;;
    table)
      if [[ "$name" == "per_frame_cost" ]]; then
        content=$(generate_per_frame_cost)
      else
        content=$(generate_bench_table "$name")
      fi
      ;;
    bench_table|bench_table_vs)
      content=$(generate_bench_table "$name")
      ;;
    replay)         content=$(generate_replay) ;;
    paired)         content=$(generate_paired "$name") ;;
    wasm_cm)        content=$(generate_wasm_cm) ;;
    wasm_native)    content=$(generate_wasm_native) ;;
    *)
      echo "WARNING: Unknown section type '$type' for '$name'" >&2
      return 0
      ;;
  esac

  if [[ $? -ne 0 || -z "$content" ]]; then
    echo "ERROR: Failed to generate section '$name'" >&2
    ERRORS=$((ERRORS + 1))
    return 1
  fi

  if $CHECK; then
    # Extract current content between markers
    local open="<!-- BENCH:${name} -->"
    local close_tag="<!-- /BENCH:${name} -->"
    local current
    current=$(awk -v otag="$open" -v ctag="$close_tag" '
      index($0, otag)  { found=1; next }
      index($0, ctag)  { found=0; next }
      found { print }
    ' "$TARGET")

    if [[ "$current" != "$content" ]]; then
      echo "STALE: $name" >&2
      if $DRY_RUN; then
        echo "--- current ($name) ---"
        echo "$current"
        echo "--- generated ($name) ---"
        echo "$content"
        echo "---"
      fi
      ERRORS=$((ERRORS + 1))
    else
      echo "OK: $name" >&2
    fi
    return 0
  fi

  if $DRY_RUN; then
    echo "=== Section: $name ==="
    echo "$content"
    echo ""
    return 0
  fi

  replace_marker "$name" "$content" "$TARGET"
  UPDATED=$((UPDATED + 1))
  echo "Updated: $name" >&2
}

# Process all sections defined in config
SECTIONS=$(jq -r '.sections | keys[]' "$CONFIG")
for section in $SECTIONS; do
  process_section "$section" || true
done

# Update "Last verified" date if not in check/dry-run mode
if ! $CHECK && ! $DRY_RUN && [[ $UPDATED -gt 0 ]]; then
  local_date=$(date +%Y-%m-%d)
  sed -i "s/^\*\*Last verified\*\*: .*/\*\*Last verified\*\*: ${local_date} (tables auto-generated by \`tools\/update-measurements.sh\`)/" "$TARGET"
  echo "Updated 'Last verified' date to $local_date" >&2
fi

if $CHECK && [[ $ERRORS -gt 0 ]]; then
  echo "ERROR: $ERRORS section(s) are stale" >&2
  exit 1
fi

if ! $CHECK; then
  echo "Done. $UPDATED section(s) updated." >&2
fi
