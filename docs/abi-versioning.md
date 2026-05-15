# ABI Versioning Policy

Kasane plugins are versioned on **two independent axes**. Plugin authors
need to track both. The host enforces the wire-level axis at load time;
the crate-level axis follows Rust semver and is enforced by Cargo.

## Two axes

| Axis | Source of truth | Example |
|---|---|---|
| WIT ABI | `kasane-wit/wit/plugin.wit:1` | `package kasane:plugin@6.4.0;` |
| SDK crate semver | `kasane-plugin-sdk/Cargo.toml` | `version = "0.6.0"` |

The two move together for major bumps that change the wire format. The
SDK *can* release patch versions without touching the ABI — a `0.6.1 →
0.6.2` SDK upgrade reuses ABI `4.0.0` and is a pure recompile against
the same generated bindings.

## Host enforcement rule — major.minor exact match

The host enforces *exact major.minor* compatibility, **not** semver-style
major-only:

- Plugin manifest declares `abi_version = "<major>.<minor>.<patch>"`.
- Host's `HOST_ABI_VERSION` reads `kasane:plugin@<major>.<minor>.<patch>`
  from the WIT package line.
- Code locator: `kasane-plugin-package/src/manifest.rs` — see
  `abi_compatible()` and `major_minor()`.

`patch` is ignored; `minor` is part of the breaking surface.

### Why is minor breaking?

WIT `variant` cases are ordered, and the wire encoding depends on that
order. Appending a case to an existing variant shifts the discriminant
of every case after it. A plugin compiled against `4.0.0` cannot safely
decode `5.0.0` records, even though Rust semver would call this a
non-breaking change.

Kasane therefore treats the entire `major.minor` pair as a single wire
identity. The patch field is reserved for host-only fixes that do not
touch the WIT.

## When to bump

| Change | major | minor | patch |
|---|---|---|---|
| Add a case to an existing `variant` | ✓ | | |
| Add a field to a `record` | ✓ | | |
| Remove or rename a function | ✓ | | |
| Reorder existing variant cases | ✓ | | |
| Add a brand-new top-level function | | ✓ | |
| Add a brand-new `record` or `variant` type | | ✓ | |
| Pure SDK helper additions (no WIT change) | | | (SDK-only minor) |
| SDK doc-only changes | | | ✓ |

SDK crate-only changes follow Rust semver: a new public function in the
SDK that uses only existing WIT types is a *crate* minor bump but
**does not** require an ABI bump.

## Plugin author workflow

1. Read `kasane-wit/wit/plugin.wit:1` of the host you are targeting.
2. Set `abi_version` in `kasane-plugin.toml` to *exactly* the value
   after the `@` — including the patch field (the host parser requires
   three components, but compares only major.minor).
3. Pin `kasane-plugin-sdk = "<host-major>.<host-minor>"` in `Cargo.toml`.
   Cargo will pick up the latest matching patch.
4. Rebuild on every host minor bump. The host rejects mismatched
   binaries at load time with a `PluginVersionMismatch` diagnostic.

## Compatibility table

| Kasane host | WIT ABI | SDK crate |
|---|---|---|
| 0.6.x | 3.0.0 | 0.6.x |
| 0.7.x (early) | 5.0.0 | 0.7.x |
| 0.7.x (Phase β-4) | 6.0.0 | 0.7.x |
| 0.7.x | 6.1.0 | 0.7.x |
| 0.7.x | 6.2.0 | 0.7.x |
| 0.7.x | 6.3.0 | 0.7.x |
| 0.7.x (current) | 6.4.0 | 0.7.x |

Future entries land here as releases ship.

ABI 6.4.0 ([#111](https://github.com/Yus314/kasane/issues/111)) adds
`get-display-cells-str` to `host-state`: a cluster-aware batch primitive
that delegates to the host's `line_display_width_str` directly
(segment-wise `Str::width`). Plugins doing column alignment over
arbitrary user content (emoji-bearing Markdown, table cells) must use
the batch form — per-codepoint `get-display-cells` summation diverges
on emoji ZWJ sequences and skin-tone modifiers under `unicode-width`
0.2. Pure additive function; 6.x plugins continue to load.

ABI 6.3.0 ([#106](https://github.com/Yus314/kasane/issues/106)) adds
the `emit-diagnostic` plugin-emitted diagnostic command and the
`plugin-diagnostic` record to the `command` variant, so plugins can
surface soft failures (e.g. "LaTeX expression X failed to render") to
the host's diagnostic panel without triggering ADR-033 quarantine.
Pure additive; 6.x plugins continue to load.

ABI 6.2.0 ([#109](https://github.com/Yus314/kasane/issues/109)) adds a
`cell-metrics` record and three accessors to `host-state` —
`get-default-font-size-px`, `get-cell-metrics`,
`backend-supports-sub-cell-spacing` — so plugins can compute pixel-unit
values for the existing `style.letter-spacing: f32` field. Unblocks UTR
#59 (CJK-Latin auto-spacing) on the GUI backend. TUI returns synthetic
1.0 values and reports `false` from the capability accessor.

ABI 6.1.0 ([#108](https://github.com/Yus314/kasane/issues/108)) adds
`get-display-cells` to `host-state`: a `unicode-width`-backed cell
count primitive for plugins doing column alignment or any computation
that must match the host's `line_display_width_str` ground truth. Pure
additive function; 6.0.0 plugins continue to load.

ABI 6.0.0 (Phase β-4) removes the retired `evaluate-extension` export
(no producers since [ADR-045](decisions/adr-045-retire-the-extension-point-dispatch-path.md);
the WIT declaration was kept under 5.0.0 to preserve binding-table
parity for legacy guests). 5.0.0 plugins are rejected at load time.

ABI 5.0.0 was the [ADR-044](decisions/adr-044-handler-effect-tier-hierarchy.md)
tier-hierarchy split: the five `runtime-effects`-returning exports
(`on-state-changed-effects`, `on-command-error-effects`,
`on-subscription`, `update-effects`, `on-io-event-effects`) now return
their ADR-mapped tier — `kakoune-side-effects` (Tier 1) or
`process-capable-effects` (Tier 2). The `runtime-effects` record and
the transitional B-2 `on-state-changed-tier1-effects` parallel were
removed.

---

## Appendix A: Variant non-exhaustive policy

**Rule**: All WIT `variant` types are treated as **non-exhaustive** from
the plugin-author perspective, even when only one case currently exists.

### Why

Appending a case to a WIT variant is a backward-incompatible wire change
(see "Why is minor breaking?" above), but Rust's pattern matcher only
flags this when the existing pattern was *irrefutable*. Plugins using
`let Foo::Bar(x) = event;` break at every variant extension if not
converted to `match`.

### Implications

1. Migration guides MUST list pattern-fixes whenever a variant gains a
   case.
2. SDK examples and helpers MUST use `match` (not irrefutable `let`)
   for variant destructuring.
3. The SDK provides safe destructure macros (e.g. `process_event!`)
   for the common single-case-of-interest pattern.

## Appendix B: Safe variant destructure macros

| Macro | Variant | Use case |
|---|---|---|
| `process_event!(event => |p| { ... })` | `IoEvent::Process` | I/O event handler that only cares about process events |

Future entries land here as the SDK adds helpers for new variants.
