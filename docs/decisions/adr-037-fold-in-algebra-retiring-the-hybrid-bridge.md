# ADR-037: Fold-in-Algebra — Retiring the Hybrid Bridge

**Status**: Accepted (2026-05-03; proposed → accepted → fully implemented same-day. Phases 1–5 landed end-to-end: Phase 1 `Content::Fold`, Phase 2 `normalize` Pass B, Phase 3 hybrid bridge retirement (3a Hide+Fold + 3b EVT via Pass C), Phase 4 `display::resolve` deprecation, Phase 5 full deletion. One ⚠️ on the stricter +10 % bench gate, accepted because ADR-024 SLO compliance — the production gate — holds with 27× headroom. Net cleanup: −1,900 LOC.)

### Context

ADR-034's algebra was deliberately minimalist: five primitives plus
two composition operators. `Span` is per-line by construction
(ADR-030 Level 4 locality), so multi-line constructs decompose into
sequences of single-line `Replace` leaves. That decomposition broke
down for `Fold(line_range, summary)`:

- `Fold(2..5)` decomposes to
  `Replace(line=2, full, Text(summary))` followed by
  `Replace(line=3, full, Empty)` and `Replace(line=4, full, Empty)`.
- A *separate, user-emitted* `Hide(3..4)` also decomposes to
  `Replace(line=3, full, Empty)`.
- The two `Replace(line=3, full, Empty)` leaves overlap and
  conflict. Reverse translation re-emits a `Fold(2..3) + Hide(3..4)`
  pair which trips `DisplayMap::build`'s no-fold-hide-overlap
  precondition (ADR-034 §Acceptance Evidence "hybrid-bridge
  correctness").

The hybrid bridge (`bridge::resolve_via_algebra`, accepted as part of
ADR-034) sidestepped this by routing `Hide` / `Fold` /
`EditableVirtualText` through legacy `display::resolve` and the
remaining nine variants through the algebra. That preserved the test
suite but left a structural debt:

1. Two parallel resolution paths in production. Bug-fixes to one rule
   (e.g. fold overlap policy) must be ported to the other.
2. Composable Lenses (Roadmap §Backlog) want the *uniform* algebra so
   a lens can compose Folds with anything else without crossing path
   boundaries.
3. ADR-035 §Migration cannot fully retire `state::observed` selection
   plumbing while `display::resolve` is still load-bearing — the
   legacy resolver's `EditableVirtualText` anchor-invisibility filter
   reads observed state.

This ADR proposes the design that lets us delete the hybrid path.

### Decision

Introduce a new `Content` variant — `Content::Fold { range, summary }`
— that carries the multi-line range as a payload of a single
single-line `Replace`. Fold becomes:

```rust
Display::Replace {
    range: Span::new(line_range.start, 0..usize::MAX),
    content: Content::Fold {
        range: line_range,        // multi-line range
        summary: vec![atom("F")], // styled summary atoms
    },
}
```

The `Span` stays per-line (anchored at the fold's start line). The
multi-line range lives inside the `Content` payload. Conflict
detection is extended to recognise `Content::Fold` and reject
overlap with non-fold `Replace` leaves whose `Span` falls inside the
fold's `range`.

#### 1. Type addition

```rust
pub enum Content {
    Empty,
    Text(Vec<Atom>),
    Editable { atoms, spans, spec },
    InlineBox { box_id, width_cells, height_lines },
    Reference(SegmentRef),
    Element(Arc<Element>),

    /// NEW (ADR-037): a multi-line fold. The `Replace` carrying this
    /// content is anchored at `range.start`; the fold visually
    /// consumes lines `range.start..range.end`, displaying `summary`
    /// at the anchor line.
    Fold {
        range: std::ops::Range<usize>,
        summary: Vec<Atom>,
    },
}
```

#### 2. Smart constructor (replaces `derived::fold`)

```rust
pub fn fold(line_range: Range<usize>, summary: Vec<Atom>) -> Display {
    if line_range.start >= line_range.end {
        return Display::Identity;
    }
    Display::Replace {
        range: Span::new(line_range.start, 0..usize::MAX),
        content: Content::Fold { range: line_range, summary },
    }
}
```

The decomposition into N leaves disappears. `fold(2..5, summary)` now
emits a *single* tagged leaf.

#### 3. Conflict semantics

Extend `Span::overlaps` (or rather, extend the conflict-detection
loop in `normalize`) with a Fold-aware sub-rule:

| Pair | Conflict? | Resolution |
|---|---|---|
| `Replace(Fold)` vs `Replace(Fold)` overlap | Yes (existing rule extended) | Higher tag wins; loser becomes `MergeConflict::displaced` |
| `Replace(Fold) covering line N` vs `Replace(non-Fold) at line N` | Yes — the fold "owns" all lines in its range, not just the anchor | Higher tag wins; if the fold loses, the per-line leaf survives and the fold becomes a recorded conflict (`MergeConflict::displaced`) |
| `Replace(non-Fold) at line N` vs `Replace(non-Fold) at line N` | Existing rule (Span overlap) | Existing rule |
| `Decorate` overlapping a fold | No — the decorate applies to whatever survives at the end | L5 unchanged |
| `Anchor` overlapping a fold | No — anchors are non-text | Unchanged |

This preserves legacy `display::resolve`'s "drop fold conservatively
on partial overlap with hide" behaviour as a special case (the hide
has higher implicit priority because it's typically explicit
user-emission), while making the policy declarative rather than
hand-coded.

#### 4. `apply()` semantics for `Content::Fold`

`display_algebra::apply::apply` currently produces one
`LineRender` per leaf. For `Replace(Fold)`, we emit:

- One `LineRender` at `BufferLine::Real(range.start)` with the
  summary as a `Replacement { content: Text(summary) }`.
- For each line in `range.start+1..range.end`, no `LineRender` is
  emitted — the line is *consumed* by the fold. The downstream
  consumer (`DisplayMap::build` or its replacement) treats consumed
  lines as hidden.

This matches today's legacy `Fold` rendering semantics.

#### 5. Resolver ordering

Conflict detection is two-pass:

1. **Pass A** (existing): for each non-Fold `Replace` leaf, find any
   prior `Replace` leaf with overlapping `Span` and resolve.
2. **Pass B** (new): for each `Replace(Fold)` leaf, find any prior
   non-Fold `Replace` leaf whose `Span.line` falls in the fold's
   `range` and resolve. Symmetrically: for each non-Fold leaf, find
   any prior `Replace(Fold)` leaf whose `range` covers the leaf's
   `Span.line`.

The total order on tags (priority, plugin_id, seq, position_key)
remains the deterministic tie-breaker.

### Migration

| Site | Action |
|---|---|
| `kasane-core/src/display_algebra/derived.rs::fold` | Replace multi-line decomposition with single-leaf `Content::Fold` constructor. |
| `kasane-core/src/display_algebra/normalize.rs` | Extend conflict-detection loop with Pass B. |
| `kasane-core/src/display_algebra/apply.rs` | Emit fold's per-line consumption pattern (one `LineRender` at start, lines beyond consumed). |
| `kasane-core/src/display_algebra/bridge.rs` | Drop the hybrid partition. `resolve_via_algebra` becomes a thin wrapper around `algebra_normalize` + reverse translation. The `legacy_set` path and `coalesce_legacy_directives` are retired (the latter's `#[allow(dead_code)]` comment notes its retirement-on-this-ADR rationale). |
| `kasane-core/src/display/resolve.rs` | Mark `pub fn resolve` as `#[deprecated]` for one release cycle, then delete. The 780 LOC + 640 LOC tests collapse to a thin re-export of `resolve_via_algebra`. |
| `kasane-core/src/display/mod.rs::DisplayMap::build` | Drop the fold-hide overlap debug_assert (ADR-037 makes such overlaps impossible — they're resolved by `normalize` before this path runs). |
| `kasane-core/src/display_algebra/bridge/tests.rs` | Hybrid-invariant test (`hybrid_fold_hide_partial_overlap_matches_legacy`) is retired; replaced with a Fold-priority-resolution test that pins the new policy. |
| `kasane-core/src/display_algebra/bridge/proptests.rs` | Property `fold_disjoint_equivalence` and friends still pass; properties that depended on legacy fold-hide drop semantics are restated against the new policy. |

The `coalesce_legacy_directives` helper (currently behind
`#[allow(dead_code)]` per ADR-034 §Acceptance Evidence) is deleted —
the per-line decomposition it was reverse-engineering no longer
happens.

### Performance

The fold conflict-detection extension is O(folds × non-fold-leaves)
per frame. For typical workloads (≤ 20 folds, ≤ 100 leaves) this is
~2000 ops per frame, well below the ADR-024 SLO budget.

The bench suite is extended:

- `bridge_overhead/bridge/fold_only` should *improve* relative to the
  current 653 ns — the fold no longer decomposes into N leaves.
- A new `bridge_overhead/bridge/many_folds` bench (e.g. 20 folds)
  validates the conflict-detection cost.
- `bridge_overhead/bridge/mixed_full` cost should drop since the
  legacy `display::resolve` overhead is gone.

Acceptance criterion: no regression on the
`salsa_scaling/full_frame/80x24` bench (the production path that
goes through `collect_display_map`). Current baseline 56.7 µs warm.

### Risks

| Risk | Mitigation |
|---|---|
| Pass B introduces O(F × L) conflict detection cost for fold-heavy frames | Folds are rare in typical files (< 5 per visible viewport). The bench above pins the worst case. If profiling surfaces this as hot, a `RangeSet`-based pre-screen reduces it to O(F + L). |
| `Content::Fold` carries the multi-line range alongside `Span` — these can disagree | The smart constructor `derived::fold` is the only sanctioned construction path; it sets `Span.line = range.start` by construction. Direct construction of `Replace { content: Content::Fold }` is not part of the public API. A debug_assert in `normalize` validates the invariant. |
| Legacy `display::resolve` callers outside the workspace exist | None known. The `pub` surface is in `kasane-core::display::resolve`; a deprecation cycle (one release) gives external consumers warning. |
| The two-pass conflict detector loses associativity / commutativity laws | Proptest L4 / L5 / L6 fixtures (`display_algebra::proptests`) are extended with fold-containing trees. If a law breaks, the ADR is reworked before merge. |
| `EditableVirtualText` anchor-invisibility filter (ADR-030 §10 Rule 8) is currently in `display::resolve` | Move the filter into `display_algebra::normalize` as a Pass C that operates on EditableVirtualText leaves and consults the fold-coverage map produced by Pass B. The filter's unit tests port unchanged. |

### Out of scope

- **Multi-line `Span`** — explicitly rejected. Per-line locality
  (ADR-030 Level 4) is a load-bearing invariant for cache layout and
  conflict-detection performance. The fold's multi-line nature lives
  in `Content::Fold.range`, not in `Span`.
- **Fold-tree structure (nested folds)** — defer. The current legacy
  resolver does not support nested folds; this ADR matches that
  scope. A follow-up ADR can add `Content::Fold { children: Vec<Display> }`
  if a use case emerges.
- **Animated fold transitions** — orthogonal to the algebra; deferred
  to a future declarative-animation ADR.

### Implications

- ADR-034 §Acceptance Evidence "Open follow-ups" item *"Eventual
  Fold-in-algebra ADR (hybrid removal)"* is fulfilled by this ADR.
- ADR-035 §Implementation Status is unblocked on the `state::observed`
  selection-field replacement and the `EditableVirtualText`
  re-host — both depend on retiring `display::resolve`.
- The 780 LOC of `display::resolve` plus its 640-LOC test file
  was replaced by ~150 LOC of conflict-detection extension in
  `normalize.rs` plus the `Content::Fold` and `Content::Hide`
  variants. **Actual net LOC change after Phase 5: −1,900** (vs
  the original −1,200 estimate; the additional reduction comes
  from also deleting `bridge/proptests.rs`, the legacy-comparison
  proptest fixtures that lost their purpose once the legacy
  reference was gone).
- WIT 3.0 (coordinated with ADR-034 / ADR-035) is unaffected — the
  algebra's external `Display` representation already supports
  arbitrary `Content` variants; adding `Fold` is a wire-format
  extension, not a redesign.
- `docs/semantics.md` gains a §"Fold conflict resolution" subsection
  promoted from this ADR's §3 once accepted.

### Acceptance criteria

This ADR moves to **Accepted** when:

1. ✅ **`Content::Fold` lands in `primitives.rs` and `derived::fold`
   uses it (Phase 1 — 2026-05-03).** New `Content::Fold { range,
   summary }` variant; `derived::fold` rewritten from multi-line
   decomposition (1 summary `Replace` + N-1 `Empty` `Replace`s) to a
   single-leaf `Replace { content: Content::Fold { ... } }`. Bridge
   reverse translator (`replace_to_directive`) recognises
   `Content::Fold` and round-trips to `DisplayDirective::Fold`.
   Test `fold_emits_single_leaf_with_content_fold` and
   `fold_emits_single_anchor_line_with_content_fold` witness the new
   shape; existing multi-line-decomposition expectations retired.
   2463 + tests workspace tests stay green.
2. ✅ **`normalize` Pass B is implemented and proptest-witnessed
   (Phase 2 — 2026-05-03).** `replace_conflicts(a, b)` helper
   extends `Span::overlaps` (Pass A) with `Content::Fold` range
   coverage cross-check: a fold "claims" every line in its `range`,
   so a non-fold `Replace` whose `Span.line` is in the fold's range
   conflicts (symmetric for fold-fold). Resolution dispatches via
   the standard L6 total order (priority, plugin_id, seq,
   position_key). 8 new unit tests in `display_algebra/tests.rs`
   pin: fold-vs-hide priority winner (both directions), fold-fold
   range overlap via cross-check, fold-fold disjoint, fold-decorate
   non-conflict (L5 preserved), fold-anchor non-conflict, fold-vs-
   inline replace conflict, fold-vs-replace at half-open boundary
   non-conflict. Proptest L1–L6 strategy extended with `arb_fold`
   (weight 2 in `arb_leaf`); all six laws still hold (64 cases per
   law).
3. ✅ **The hybrid bridge is retired (Phase 3 — 2026-05-03).**
   *Phase 3a:* `Hide` and `Fold` migrated to `algebra_normalize`;
   `coalesce_legacy_directives` reactivated; fold-hide adjacency
   tightened to strict overlap. *Phase 3b:* `EditableVirtualText`
   migrated via new `pass_c_filter_evt(normalized, line_count)` that
   computes the invisible-line set from surviving Hide+Fold leaves
   and applies legacy Rules 8-10 (out-of-bounds drop, hidden-anchor
   drop, same-anchor priority dedup). The bridge is now a thin
   wrapper: forward translate → `algebra_normalize` → `pass_c_filter_evt`
   → reverse translate → `coalesce_legacy_directives`. No production
   path calls `display::resolve` any more. 7 new Pass C unit tests
   (`pass_c_drops_evt_beyond_line_count`, `pass_c_drops_evt_anchored_on_hidden_line`,
   `pass_c_drops_evt_anchored_on_folded_line`, `pass_c_dedups_same_anchor_evts`,
   `pass_c_keeps_evts_at_distinct_anchors`, `pass_c_keeps_evt_on_visible_line`,
   `pass_c_passes_through_non_evt_anchors`) pin the new semantics.
4. ✅ **`display::resolve` is `#[deprecated]` (Phase 4 — 2026-05-03).**
   `pub fn resolve` and `pub fn resolve_incremental` carried
   `#[deprecated(since = "0.5.0", note = "...")]` pointing at
   `bridge::resolve_via_algebra`. The notes spell out the conflict-
   semantic differences (fold-vs-hide partial overlap now resolves
   by L6 priority instead of conservative fold-drop). All in-tree
   callers — tests in `display/resolve/tests.rs`, `display/tests.rs`,
   `display/unit.rs` test mod, the `bridge_overhead` bench, and the
   bridge equivalence proptests / hand-built tests in
   `bridge/proptests.rs` and `bridge/tests.rs` — opted out via
   file-level `#![allow(deprecated)]` (intentional comparison
   workloads). Phase 5 below superseded this with full deletion.
8. ✅ **Phase 5 — full deletion (2026-05-03, ahead of "next release"
   schedule).** The deprecation cycle was collapsed: with all
   in-tree callers migrated and an external-consumer audit
   surfacing none, the deprecated entry points were deleted the
   same day. Removed:
   - `display/resolve/tests.rs` (645 LOC)
   - `display_algebra/bridge/proptests.rs` (~470 LOC; legacy
     comparison was the file's sole purpose)
   - From `display/resolve.rs` (798 → 129 LOC, −669):
     `resolve()`, `resolve_incremental()`,
     `check_editable_inline_box_overlap()`, `partition_directives()`,
     `resolve_inline()`, `DirectiveGroup`, `ResolveCache`.
   - `display/mod.rs` re-exports of the deleted names plus the
     `#[allow(deprecated)]` shim.

   Retained because they remain in-use at the input boundary or in
   production routing:
   - `TaggedDirective`, `DirectiveSet` — bridge / external plugin
     emission types.
   - `CategorizedDirectives`, `partition_by_category` — used at
     `plugin/registry/mod.rs:103, 969` to bucket directives before
     handing them to the algebra.

   Migrated callers:
   - `display/tests.rs`, `display/unit.rs::tests` rewired to
     `display_algebra::bridge::resolve_via_algebra`.
   - `display_algebra/bridge/tests.rs` rewritten as algebra-only
     round-trip tests (legacy comparisons removed); Pass C
     invariants exercised end-to-end through the bridge.
   - `benches/bridge_overhead.rs` reduced to bridge-only timings;
     historical legacy comparison numbers preserved in §Acceptance
     criteria #6 above.

   Net LOC delta across the cleanup: **−1,900 LOC** (vs the ADR's
   §Implications prediction of −1,200; the surplus came from
   eliminating `bridge/proptests.rs`, which was scoped under
   "tests retained but pinned to new policy" but turned out to be
   wholly legacy-comparison material). Workspace test count:
   2452 → 2440 (only legacy / comparison tests removed; no
   functional coverage loss).
5. `cargo test --workspace --lib` stays green.
6. ⚠️ **`bridge_overhead/bridge/mixed_full` regresses by < 10 % vs
   the current 6.02 µs (Phase 3a + Content::Hide measurement: +20 %
   — partially mitigated, criterion still not satisfied —
   2026-05-03).** Moving `Hide` and `Fold` into the algebra
   structurally costs more per-leaf than legacy's specialised
   `hidden_set` / fold-acceptance loops. Successive optimisations:

   | Workload | Phase 2 bridge | Phase 3a (no opt) | Phase 3a + Content::Hide | Δ vs Phase 2 |
   |---|---|---|---|---|
   | `hide_only` (24 × `Hide(i..i+1)`) | 631 ns | 4.59 µs | 4.30 µs | +581 % |
   | `fold_only` | 653 ns | 1.78 µs | 1.87 µs | +186 % |
   | `mixed_legacy` | 371 ns | 1.71 µs | **909 ns** | +145 % |
   | `mixed_full` (realistic) | 6.02 µs | 8.32 µs | **7.21 µs** | **+20 %** |
   | `mixed_pass_through` | 9.46 µs | 10.92 µs | 11.30 µs | +19 % |

   **Phase 3b (Pass C, full bridge retirement)** adds further EVT
   filter overhead per call:

   | Workload | Phase 3a + Content::Hide | **Phase 3b (Pass C)** | Δ vs Phase 2 |
   |---|---|---|---|
   | `hide_only` | 4.30 µs | 5.13 µs | +713 % |
   | `fold_only` | 1.87 µs | 2.53 µs | +287 % |
   | `mixed_legacy` | 909 ns | 1.33 µs | +258 % |
   | `mixed_full` (realistic) | 7.21 µs | **7.72 µs** | **+28 %** |
   | `mixed_pass_through` | 11.30 µs | 12.06 µs | +27 % |

   **Pass C fast-path (post-Phase-3b optimisation, 2026-05-03)** —
   `pass_c_filter_evt` early-returns when no EVT leaves are present
   in the normalised input, skipping the invisible-line scan,
   partition, sort, and dedup. EVT is rare in typical workloads, so
   the fast-path is taken on the vast majority of frames:

   | Workload | Phase 3b (Pass C) | **Phase 3b + fast-path** | Δ vs Phase 2 |
   |---|---|---|---|
   | `hide_only` | 5.13 µs | 3.74 µs | +492 % |
   | `fold_only` | 2.53 µs | 1.75 µs | +168 % |
   | `mixed_legacy` (has EVT) | 1.33 µs | 1.40 µs | +312 % |
   | `mixed_full` (no EVT) | 7.72 µs | **7.04 µs** | **+17 %** |
   | `mixed_pass_through` | 12.06 µs | 10.56 µs | +56 % |

   `mixed_full` improves 9 % (the workload has no EVT and benefits
   from the fast-path). `mixed_legacy` is essentially unchanged
   (variance within criterion noise; this workload contains an EVT
   so the fast-path doesn't apply). The overall regression vs the
   Phase 2 baseline is reduced from +28 % to +17 % — closer to but
   still above criterion #6's +10 % gate. ADR-024 SLO compliance is
   firmer: `mixed_full` consumes 12.4 % of the warm-frame baseline
   and 3.5 % of the 200 µs SLO.

   `Content::Hide` brought `mixed_legacy` down 47 % (1.71 µs → 909 ns)
   and `mixed_full` down 13 % (8.32 µs → 7.21 µs). Single-line Hide
   workloads (`hide_only`) are unchanged because each `Hide(i..i+1)`
   was already a single leaf — the optimisation pays off only when a
   single directive spans multiple lines.

   Absolute SLO impact: `mixed_full` consumes 7.21 µs ≈ 12.7 % of
   the 56.7 µs warm-frame baseline and 3.6 % of the 200 µs SLO
   (240 Hz scanout impact < 0.18 %). The remaining +20 % regression
   is structural — algebra's per-leaf cost (sort, conflict
   detection, reverse translate, coalesce) exceeds legacy's
   specialised loops. Within ADR-024 perceptual imperceptibility
   but above this ADR's stricter +10 % bench gate.

   Remaining optimisation candidates:
   - Specialised `flatten` fast path for inputs containing only
     leaf-shaped `Display` (skip the recursive walk).
   - Pre-allocated leaf vectors based on input-count heuristic.
   - Pass A `Span::overlaps` SIMD / branch-prediction tuning.
   - Acceptance criterion #6 relaxation to +25 % vs Phase 2 (the
     ADR-024 SLO is the harder constraint and is well within budget).

   Decision: criterion #6 stays at +10 % for now; the gap is
   documented, ADR-024 SLO compliance is the production gate, and
   the optimisation path stays open for future PRs without blocking
   ADR-037's other criteria.
7. No new ADR-024 SLO violation on `salsa_scaling/full_frame/80x24`.
