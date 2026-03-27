#!/usr/bin/env bash
set -euo pipefail

# check-doc-consistency.sh — Validate documentation consistency against source code.
#
# Usage:
#   tools/check-doc-consistency.sh              # Run all checks (same as --full)
#   tools/check-doc-consistency.sh --quick      # ABI version only (for pre-commit hooks)
#   tools/check-doc-consistency.sh --full       # All checks (for CI)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Flags ---
MODE="full"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick) MODE="quick"; shift ;;
    --full) MODE="full"; shift ;;
    *) echo "Unknown flag: $1" >&2; exit 1 ;;
  esac
done

ERRORS=0
WARNINGS=0

error() {
  echo "FAIL: $1" >&2
  ERRORS=$((ERRORS + 1))
}

warn() {
  echo "WARN: $1" >&2
  WARNINGS=$((WARNINGS + 1))
}

ok() {
  echo "  OK: $1" >&2
}

# =============================================================================
# Check 1: ABI version consistency
# =============================================================================
check_abi_version() {
  echo "--- ABI version ---" >&2
  local wit_file="$ROOT_DIR/kasane-wasm/wit/plugin.wit"

  if [[ ! -f "$wit_file" ]]; then
    error "plugin.wit not found at $wit_file"
    return
  fi

  local canonical
  canonical=$(sed -n 's/^package kasane:plugin@\([0-9.]*\);/\1/p' "$wit_file")

  if [[ -z "$canonical" ]]; then
    error "Could not extract ABI version from plugin.wit"
    return
  fi

  local files_to_check=(
    "README.md"
    "docs/using-plugins.md"
    "docs/plugin-development.md"
  )

  for file in "${files_to_check[@]}"; do
    local filepath="$ROOT_DIR/$file"
    if [[ ! -f "$filepath" ]]; then
      warn "$file not found"
      continue
    fi

    # Find all kasane:plugin@X.Y.Z references
    local found
    found=$(grep -oP 'kasane:plugin@\K[0-9.]+' "$filepath" || true)

    if [[ -z "$found" ]]; then
      warn "$file: no kasane:plugin@ reference found"
      continue
    fi

    local all_match=true
    while IFS= read -r version; do
      if [[ "$version" != "$canonical" ]]; then
        error "$file: ABI version $version does not match plugin.wit ($canonical)"
        all_match=false
      fi
    done <<< "$found"

    if $all_match; then
      ok "$file: ABI version $canonical"
    fi
  done
}

# =============================================================================
# Check 2: Config color defaults
# =============================================================================
check_color_defaults() {
  echo "--- Color defaults ---" >&2
  local config_rs="$ROOT_DIR/kasane-core/src/config.rs"
  local config_md="$ROOT_DIR/docs/config.md"

  if [[ ! -f "$config_rs" ]] || [[ ! -f "$config_md" ]]; then
    warn "config.rs or config.md not found"
    return
  fi

  # Extract color defaults from config.rs:  field: "#hexval".to_string()
  # Matches lines like:  white: "#cccccc".to_string(),
  while IFS= read -r line; do
    local field value
    field=$(echo "$line" | sed -n 's/^[[:space:]]*\([a-z_]*\): "#\([0-9a-f]*\)"\.to_string().*/\1/p')
    value=$(echo "$line" | sed -n 's/^[[:space:]]*[a-z_]*: "#\([0-9a-f]*\)"\.to_string().*/\1/p')

    if [[ -z "$field" ]] || [[ -z "$value" ]]; then
      continue
    fi

    # Check config.md for this field's default value in the [colors] table
    # Table format: | `field` | `#hexval` | Description |
    local doc_value
    doc_value=$(grep -P "^\| \`${field}\`" "$config_md" | grep -oP '#[0-9a-fA-F]{6}' | head -1 || true)

    if [[ -z "$doc_value" ]]; then
      # Field might not be in the colors table (could be in a different section)
      continue
    fi

    local code_hex="#${value}"
    if [[ "$doc_value" != "$code_hex" ]]; then
      error "docs/config.md [colors] ${field}: docs say $doc_value, code says $code_hex"
    else
      ok "colors.${field}: $code_hex"
    fi
  done < <(grep -E '^\s+\w+: "#[0-9a-f]+"\.to_string\(\)' "$config_rs")
}

# =============================================================================
# Check 3: Config scalar defaults (bool, integer, float, string)
# =============================================================================
check_scalar_defaults() {
  echo "--- Scalar defaults ---" >&2
  local config_rs="$ROOT_DIR/kasane-core/src/config.rs"
  local config_md="$ROOT_DIR/docs/config.md"

  if [[ ! -f "$config_rs" ]] || [[ ! -f "$config_md" ]]; then
    return
  fi

  # Known scalar defaults to check: field, expected_value, doc_pattern
  # Format: "field|code_default|doc_grep_pattern"
  local checks=(
    # [ui]
    'shadow|true|`shadow`'
    'padding_char|"~"|`padding_char`'
    'backend|"tui"|`backend`'
    # [scroll]
    'lines_per_scroll|3|`lines_per_scroll`'
    'smooth|false|`smooth`'
    # [menu]
    'max_height|10|`max_height`'
    # [search]
    'dropdown|false|`dropdown`'
    # [clipboard]
    'enabled|true|`enabled`.*clipboard'
    # [mouse]
    'drag_scroll|true|`drag_scroll`'
    # [window]
    'initial_cols|80|`initial_cols`'
    'initial_rows|24|`initial_rows`'
    'fullscreen|false|`fullscreen`'
    'maximized|false|`maximized`'
    # [font]
    'family|"monospace"|`family`'
    'size|14.0|`size`.*Font'
    'line_height|1.2|`line_height`'
    'letter_spacing|0.0|`letter_spacing`'
    # [plugins]
    'auto_discover|true|`auto_discover`'
  )

  for check in "${checks[@]}"; do
    IFS='|' read -r field code_default doc_pattern <<< "$check"

    # Find the table row in config.md matching this field
    local row
    row=$(grep -P "$doc_pattern" "$config_md" | head -1 || true)

    if [[ -z "$row" ]]; then
      # Field not found in docs — skip (might be in a non-table section)
      continue
    fi

    # Extract the default value from the table row
    # Table format: | `key` | type | `default` | description |
    # The default column is the 3rd pipe-delimited field
    local doc_default
    doc_default=$(echo "$row" | awk -F'|' '{print $4}' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | sed 's/`//g')

    # Normalize for comparison
    local code_norm="$code_default"
    local doc_norm="$doc_default"

    # Strip quotes for string comparison
    code_norm=$(echo "$code_norm" | tr -d '"')
    doc_norm=$(echo "$doc_norm" | tr -d '"')

    if [[ "$doc_norm" != "$code_norm" ]]; then
      error "docs/config.md ${field}: docs say '${doc_default}', code says '${code_default}'"
    fi
  done

  ok "scalar defaults checked"
}

# =============================================================================
# Check 4: Capability and authority valid names
# =============================================================================
check_capability_names() {
  echo "--- Capability/authority names ---" >&2
  local config_rs="$ROOT_DIR/kasane-core/src/config.rs"
  local config_md="$ROOT_DIR/docs/config.md"

  if [[ ! -f "$config_rs" ]] || [[ ! -f "$config_md" ]]; then
    return
  fi

  # Extract capability names from config.rs doc comment
  # Line: /// Valid capability names: "filesystem", "environment", "monotonic-clock", "process".
  local code_caps
  code_caps=$(grep 'Valid capability names:' "$config_rs" | grep -oP '"[^"]*"' | tr -d '"' | sort)

  local doc_caps
  doc_caps=$(grep 'Valid capability names:' "$config_md" | grep -oP '`"[^"]*"`' | tr -d '`"' | sort)

  if [[ -n "$code_caps" ]] && [[ -n "$doc_caps" ]]; then
    local missing
    missing=$(comm -23 <(echo "$code_caps") <(echo "$doc_caps") || true)
    if [[ -n "$missing" ]]; then
      error "docs/config.md deny_capabilities: missing capabilities: $missing"
    fi

    local extra
    extra=$(comm -13 <(echo "$code_caps") <(echo "$doc_caps") || true)
    if [[ -n "$extra" ]]; then
      warn "docs/config.md deny_capabilities: extra capabilities not in code: $extra"
    fi

    if [[ -z "$missing" ]] && [[ -z "$extra" ]]; then
      ok "capability names match"
    fi
  fi

  # Extract authority names from config.rs doc comment
  local code_auths
  code_auths=$(grep 'Valid authority names:' "$config_rs" | grep -oP '"[^"]*"' | tr -d '"' | sort)

  local doc_auths
  doc_auths=$(grep 'Valid authority names:' "$config_md" | grep -oP '`"[^"]*"`' | tr -d '`"' | sort)

  if [[ -n "$code_auths" ]] && [[ -n "$doc_auths" ]]; then
    local missing
    missing=$(comm -23 <(echo "$code_auths") <(echo "$doc_auths") || true)
    if [[ -n "$missing" ]]; then
      error "docs/config.md deny_authorities: missing authorities: $missing"
    fi

    local extra
    extra=$(comm -13 <(echo "$code_auths") <(echo "$doc_auths") || true)
    if [[ -n "$extra" ]]; then
      warn "docs/config.md deny_authorities: extra authorities not in code: $extra"
    fi

    if [[ -z "$missing" ]] && [[ -z "$extra" ]]; then
      ok "authority names match"
    fi
  fi
}

# =============================================================================
# Check 5: Documentation file references (lightweight link check)
# =============================================================================
check_doc_links() {
  echo "--- Doc file references ---" >&2

  # Check relative markdown links in docs/
  local doc_dir="$ROOT_DIR/docs"
  local link_errors=0

  for md_file in "$doc_dir"/*.md; do
    [[ -f "$md_file" ]] || continue
    local basename
    basename=$(basename "$md_file")

    # Extract markdown links: [text](path)
    # Only check relative .md links and ../ links
    while IFS= read -r link; do
      # Skip URLs, anchors-only, and empty
      [[ "$link" =~ ^https?:// ]] && continue
      [[ "$link" =~ ^# ]] && continue
      [[ -z "$link" ]] && continue

      # Strip anchor fragment
      local path="${link%%#*}"
      [[ -z "$path" ]] && continue

      # Resolve relative to the doc file's directory
      local resolved
      resolved=$(cd "$(dirname "$md_file")" && realpath -m "$path" 2>/dev/null || echo "")

      if [[ -n "$resolved" ]] && [[ ! -e "$resolved" ]]; then
        error "docs/$basename: broken link to $link"
        link_errors=$((link_errors + 1))
      fi
    done < <(grep -oP '\]\(\K[^)]+' "$md_file" || true)
  done

  # Also check README.md
  local readme="$ROOT_DIR/README.md"
  if [[ -f "$readme" ]]; then
    while IFS= read -r link; do
      [[ "$link" =~ ^https?:// ]] && continue
      [[ "$link" =~ ^# ]] && continue
      [[ -z "$link" ]] && continue

      local path="${link%%#*}"
      [[ -z "$path" ]] && continue

      local resolved
      resolved=$(cd "$ROOT_DIR" && realpath -m "$path" 2>/dev/null || echo "")

      if [[ -n "$resolved" ]] && [[ ! -e "$resolved" ]]; then
        error "README.md: broken link to $link"
        link_errors=$((link_errors + 1))
      fi
    done < <(grep -oP '\]\(\K[^)]+' "$readme" || true)
  fi

  if [[ $link_errors -eq 0 ]]; then
    ok "all doc links valid"
  fi
}

# =============================================================================
# Main
# =============================================================================

echo "=== check-doc-consistency ($MODE) ===" >&2

check_abi_version

if [[ "$MODE" == "full" ]]; then
  check_color_defaults
  check_scalar_defaults
  check_capability_names
  check_doc_links
fi

echo "===" >&2

if [[ $ERRORS -gt 0 ]]; then
  echo "RESULT: $ERRORS error(s), $WARNINGS warning(s)" >&2
  exit 1
fi

if [[ $WARNINGS -gt 0 ]]; then
  echo "RESULT: $WARNINGS warning(s), 0 errors" >&2
fi

echo "RESULT: all checks passed" >&2
exit 0
