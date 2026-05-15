# WIT 2.x — Bundled Text Metrics Surface (deferred)

**Status:** deferred — gated on a layout-pipeline Parley-accurate ADR.

This file tracks a family of text-metric exports that share a single
architectural prerequisite. Bundling them into one major bump avoids the
"three consecutive minor breaks" trap ADR-031 §動機-5 was written to
prevent.

## Constituent items

| Item | Source | Notes |
|---|---|---|
| `get-display-cells-str(s: string) -> u32` | #108 follow-up | Cluster-aware cell-grid width matching host `line_display_width_str`. Independent of the Parley-accurate ADR — ships any time as a minor bump. Drives the divergence noted in [§Why deferred](#why-deferred). |
| `get-cluster-advance-em(text, byte-index) -> f32` | [#105](https://github.com/Yus314/kasane/issues/105) (closed) | Parley `Cluster::visual_offset`-derived, per-cluster, RTL/ligature-aware |
| `get-string-advance-em(text) -> f32` | #105 (closed) | Sum-with-kerning over a string |
| `letter-spacing` per-cluster writeback | ADR-031 §"WIT 2.0 Wire Shape" candidate list | Currently per-style only (`style.letter-spacing: f32`) |
| `face-id` parameter on metrics queries | #105 Alternative C | Lets plugins query non-default faces |
| Font-variation axis metadata | ADR-031 WIT 2.0 candidate | Per-axis name/min/max for variable fonts |
| `bidi-override` per-atom | ADR-031 WIT 2.0 candidate | Currently inferred from text content |

## Why deferred

`kasane-core/src/render/pipeline_salsa.rs`, `salsa_views/{menu,info,
status}`, `kasane-gui/src/gpu/scene_renderer/{mod,draw_commands}`,
`kasane-gui/src/ime.rs` — the entire layout pipeline computes column
positions via `line_display_width_str` (unicode-width cell-grid). Parley
advances are consulted only at hit-test (`kasane-gui/src/gpu/text/
hit_test.rs::byte_to_advance`) and per-glyph paint.

Exposing Parley cluster-advances to plugins **without** first migrating
the layout pipeline gives plugins coordinate data they cannot reconcile
with the host's column model. Plugins would compute a "true" px position
that doesn't match where the host actually lays the next atom — silent
misalignment.

The cell-grid alternatives partially cover the cases that motivated #105:

- **GFM table border alignment**: partially solved by [#108](https://github.com/Yus314/kasane/issues/108)
  `get-display-cells` for ASCII / pure-CJK / BMP content. The primitive
  is per-codepoint (`Char::width`); the host's `line_display_width_str`
  uses `Str::width`. These agree on non-cluster strings but diverge on
  emoji ZWJ sequences (`👨‍👩‍👧`: 2 vs 6) and skin-tone modifiers
  (`👍🏽`: 2 vs 4) under `unicode-width` 0.2. Cluster-aware totals over
  arbitrary user content need `get-display-cells-str` (below) —
  promoted from "deferred for performance" to "candidate for correctness".
- **UTR #59 ¼em CJK-Latin spacing**: solved by
  [#109](https://github.com/Yus314/kasane/issues/109)
  `get-default-font-size-px` plus the existing `style.letter-spacing:
  f32` — plugin computes `0.25 × font-size-px`, emits as letter-spacing
  on the boundary atom.

## Precursor ADR (not yet written)

The bundle becomes implementable once an ADR commits the host's layout
pipeline to Parley-accurate column math. That ADR must address:

- Per-atom layout latency: variable-pitch column resolution is more
  expensive than cell-grid; see ADR-024 perceptual-imperceptibility
  envelope
- TUI/GUI semantic split: does TUI continue with cell-grid while only
  GUI moves to Parley-accurate? Or does TUI compute integer-px-rounded
  positions from the same Parley pipeline?
- Cache invariants: `LayoutCache` keys today are shape-affecting style
  fields (font, weight, letter_spacing, etc.). A cluster-advance query
  API needs a corresponding cache layer with compatible invalidation
- `salsa_views` migration: each widget that computes column widths
  needs a Parley-aware salsa input

## Signature reservations (pre-paper-design)

These shapes are placeholder — the precursor ADR may revise them:

```wit
record cluster-advance {
    byte-start: u32,
    byte-end: u32,
    advance-px: f32,
    is-rtl: bool,
}

interface host-state {
    /// Layout `text` under the active text face at byte-index `byte`;
    /// return the cluster covering that byte position.
    get-cluster-at: func(text: string, byte: u32) -> option<cluster-advance>;

    /// Full cluster layout for a string under the active text face.
    /// Costs ~O(text.len()) Parley shape; cache aggressively.
    layout-clusters: func(text: string) -> list<cluster-advance>;
}
```

## How to track progress

When the precursor ADR lands, file a tracker issue and link it from
this file. Do not ship items piecemeal — the major bump cost should
buy a coherent, complete surface that plugins can adopt in one
migration step.

## Related

- Parent RFC (closed): [#105](https://github.com/Yus314/kasane/issues/105)
- Sibling minor bumps that ship today:
  - [#108](https://github.com/Yus314/kasane/issues/108) get-display-cells
  - [#109](https://github.com/Yus314/kasane/issues/109) font/cell metrics
- ADR-031 WIT 2.0 candidate list: `docs/decisions/adr-031-text-stack-migration-cosmic-text-parley-swash-with-protocol-style-redesign.md`
  §"WIT 2.0 Wire Shape" (line 562)
- Existing cluster advance reader (host-internal): `kasane-gui/src/gpu/text/hit_test.rs:72`
