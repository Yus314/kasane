# ADR-034: Display Algebra — From Variant Enum to Composable Primitives

**Status**: Accepted (2026-05-03; proposed and accepted same-day after end-to-end validation through the hybrid bridge in production code paths)

### Context

`DisplayDirective` (`kasane-core/src/display/mod.rs`) is currently a 12-variant enum:
`Hide`, `Fold`, `InsertBefore`, `InsertAfter`, `InsertInline`, `HideInline`,
`StyleInline`, `InlineBox`, `StyleLine`, `Gutter`, `VirtualText`,
`EditableVirtualText`. Multi-plugin composition is handled by `display/resolve.rs`
(780 LOC) plus `display/resolve/tests.rs` (640 LOC), which carries variant-specific
logic for each pairwise interaction (Fold-Hide partial overlap, EditableVirtualText
overlap with InlineBox, priority-tied disambiguation, and so on).

This shape has accreted several costs:

1. **Adding a directive variant is a four-place change**: the enum, `sort_key()`,
   `resolve()`, and the `DisplayMap` projection. Each new variant ships with a
   bespoke composition rule because the resolver is variant-aware rather than
   structural. ADR-030 Level 6 tightened the rules but did not abstract them.
2. **Composition properties are not formal**. `resolve()` is "best-effort
   commutative" — same inputs in different orders produce the same output by
   sort-key construction (`(priority, plugin_id, variant_ordinal, anchor)`),
   but composition itself is not associative across runs because conflict
   resolution is greedy (higher-priority fold accepted first, lower-priority
   overlapping folds dropped). This is the right *outcome* but the *reasoning*
   lives in code rather than in algebra.
3. **Composable Lenses (the foundation of the Phase 0 plan in `roadmap.md`
   §Backlog "External plugin candidates")** want `lens = Display generator` and
   `lens stack = monoidal composition`. The current variant enum forces a lens
   to emit specific variants and the resolver to special-case their interaction.
4. **Variant duplication**: `Hide` is "Replace range with empty content";
   `Fold` is "Replace range with a summary line"; `InsertBefore` is "Insert at
   line.start"; `VirtualText` is "Insert with virtual semantics"; `Gutter` is
   "Insert into a positional anchor lane". These share structure that the type
   does not surface. Five of the twelve variants are projections of two ideas.
5. **Cross-runtime drift**: WIT bindings (`kasane:plugin@2.0.0`) mirror the
   12-variant enum. Each variant change is a host-+-WASM coordinated migration.

This ADR proposes replacing the enum with a small algebra of primitives plus
two composition operators, deriving the existing 12 variants as named smart
constructors over the algebra.

### Decision

Adopt a **five-primitive Display algebra** with two composition operators and a
small set of algebraic laws.

#### 1. Primitives

```rust
pub enum Display {
    /// Identity — produces no change. The unit of `then` and `merge`.
    Identity,

    /// Substitute the content of `range` with `content`. The byte range is
    /// degenerate (start == end) for pure insertions.
    ///
    /// Special cases: `Replace(range, Content::Empty)` is hide;
    /// `Replace(zero_range, content)` is insertion.
    Replace { range: Span, content: Content },

    /// Apply `style` over `range`. Has no positional effect; pure decoration.
    Decorate { range: Span, style: Style },

    /// Attach `content` to a non-text anchor — gutters, ornaments, overlays —
    /// without consuming buffer width.
    Anchor { position: AnchorPosition, content: Content },

    /// Sequential composition: `then(a, b)` evaluates `a`, then `b` sees the
    /// post-`a` document. Non-commutative.
    Then(Box<Display>, Box<Display>),

    /// Parallel composition: `merge(a, b)` evaluates `a` and `b` against the
    /// same input. Commutative when `a ⊥ b` (disjoint supports). Conflict
    /// produces a typed `MergeConflict` carried into the resolved output for
    /// the host to surface.
    Merge(Box<Display>, Box<Display>),
}
```

`Span` is `(line: usize, byte_range: Range<usize>)` — line+inline byte address;
multi-line ranges are expressed as `Then` chains over single-line `Span`s. This
is a deliberate tradeoff: the algebra stays per-line-flat, multi-line semantics
emerge from composition. ADR-030 Level 4 already requires per-line directive
locality; this just makes it the type's responsibility.

`Content` is:
```rust
pub enum Content {
    Empty,
    Text(Vec<Atom>),                // Styled inline content
    Editable(Vec<Atom>, EditSpec),  // ShadowCursor-bound (see ADR-035)
    InlineBox(InlineBoxId),         // Plugin-painted box (Phase 10)
    Reference(SegmentRef),          // Pull from another buffer (cross-file inline)
}
```

`AnchorPosition` is:
```rust
pub enum AnchorPosition {
    Gutter { line: usize, lane: u8 },     // Numbered gutter columns
    Ornament { line: usize, side: Side },  // Pre/post-line decorations
    Overlay { rect: Rect },                // Floating overlays
}
```

#### 2. Composition operators

`Then` and `Merge` are first-class enum constructors, not external operators,
so the algebra is closed and serializable.

- `Identity` is the unit: `then(Identity, x) == x` and `merge(Identity, x) == x`.
- `Then` is associative: `then(then(a, b), c) == then(a, then(b, c))`.
- `Merge` is associative and commutative under disjoint supports.

#### 3. Algebraic laws (testable)

These are *normative* — proptest fixtures will witness them:

| Law | Statement |
|---|---|
| L1 Identity | `then(I, d) ≡ d` and `merge(I, d) ≡ d` |
| L2 Then-associativity | `then(then(a, b), c) ≡ then(a, then(b, c))` |
| L3 Merge-associativity | `merge(merge(a, b), c) ≡ merge(a, merge(b, c))` |
| L4 Merge-commutativity (disjoint) | `support(a) ∩ support(b) = ∅ ⟹ merge(a, b) ≡ merge(b, a)` |
| L5 Decorate-commutativity | `merge(Decorate(r1, s1), Decorate(r2, s2))` always commutes; conflicts on overlap resolve by tagged-priority style stacking |
| L6 Replace-conflict-determinism | When `merge` would replace overlapping ranges, the result is `MergeConflict { winner: TaggedDirective, displaced: Vec<TaggedDirective> }`, deterministic by `sort_key`. |

`support(d)` is the set of buffer positions touched by `d`. `Decorate` is style-only
and never conflicts with `Replace` over the same range (style applies to whatever
content survives).

#### 4. Derived constructors (compatibility-shaped, but no compat)

The existing 12 variants become named constructors. They are **convenience**, not
the type:

```rust
impl Display {
    pub fn hide(range: Span) -> Self {
        Display::Replace { range, content: Content::Empty }
    }

    pub fn fold(range: Span, summary: Vec<Atom>) -> Self {
        Display::Replace { range, content: Content::Text(summary) }
    }

    pub fn insert_after(line: usize, content: Vec<Atom>) -> Self {
        Display::Replace {
            range: Span::end_of_line(line),
            content: Content::Text(content),
        }
    }

    pub fn gutter(line: usize, lane: u8, content: Vec<Atom>) -> Self {
        Display::Anchor {
            position: AnchorPosition::Gutter { line, lane },
            content: Content::Text(content),
        }
    }

    // ... and so on for the other variants
}
```

Plugin authors keep ergonomic factories. The compiler sees a single type. The
resolver becomes a structural reduction over `Display` rather than a 12-variant
match.

#### 5. Resolution

`display/resolve.rs` is replaced by `display_algebra/normalize.rs`:

```rust
pub fn normalize(displays: Vec<TaggedDisplay>) -> NormalizedDisplay { ... }
```

`normalize` collapses the algebra into a flat per-line representation that the
existing `DisplayMap::build()` consumes (or its replacement). The implementation
is a tree fold whose interesting clause is `Merge` conflict handling. Estimated
size: ~300 LOC, less than half of current `resolve.rs`.

#### 5.1 Canonical normalisation requires a positional tertiary key

**Discovered during implementation (2026-05-03).** L4 (Merge-commutativity on
disjoint supports) is *not* satisfied by sorting on `(priority, plugin_id, seq)`
alone. When two leaves share all three keys (e.g. two plugin-emitted leaves
with the same `(priority, plugin_id, 0)` tag, produced by a single plugin
invocation that emits `merge(a, b)` for disjoint `a` and `b`), a stable sort
preserves emission order — and the emission order *differs* between
`merge(a, b)` and `merge(b, a)`, breaking commutativity.

The fix is to add a positional tertiary key after the L6 tuple:

```text
total_order = (priority, plugin_id, seq, position_key(display))
```

`position_key` is `(line, byte_start, variant_tag)` for `Replace` /
`Decorate` and the analogous tuple for `Anchor` (gutter lane / ornament
side / overlay column). For non-overlapping `Replace`s the renderer
order is irrelevant (their effects are positionally disjoint); for
overlapping `Decorate`s the priority component already orders them, and
the positional component only breaks ties between same-priority decorates
on the same range.

**ADR amendment**: §5 Resolution is hereby extended with this tertiary key
as a normative requirement. The implementation in
`kasane-core/src/display_algebra/normalize.rs::position_key` is the
reference implementation. A proptest fixture (`display_algebra::proptests`)
will witness L4 under random `merge(a, b)` / `merge(b, a)` pairs to guard
against regressions.

### Acceptance Evidence (2026-05-03)

The proposal-to-accepted transition is justified by the following
landed artifacts and measurements:

**Implementation**:
- `kasane-core/src/display_algebra/` — primitives (Display, Span, Content,
  AnchorPosition, Side, Style, EditSpec), derived smart constructors,
  normalize, apply (per-line render plan), and the bridge that hybrid-
  routes Hide/Fold/EditableVT through legacy `display::resolve` and the
  remaining 9 variants through `algebra_normalize`. ~2,200 LOC.
- `kasane-core/src/plugin/registry/collection.rs:852, 893` — both
  production callsites switched from `display::resolve` to
  `bridge::resolve_via_algebra`. The Salsa-backed display map now flows
  through the new algebra.

**Test coverage**:
- 1789 `kasane-core` lib tests green; 2437 workspace lib tests green.
- 23 hand-built unit tests for primitives + smart constructors.
- 7 proptest fixtures witnessing L1–L6 over randomised `Display` trees
  (64 cases per law).
- 22 bridge tests (17 hand-built + 4 proptest equivalence properties +
  1 hybrid-invariant case).
- `cargo clippy -p kasane-core --tests -- -D warnings` clean for the
  new modules.

**Performance** (per `bridge_overhead` bench, criterion 50-sample
median, post zero-clone optimisation 2026-05-03):

| Workload | Legacy | Bridge | Δ abs | Δ vs `frame_warm_24_lines` (56.7 µs) | Δ vs SLO (200 µs) |
|---|---|---|---|---|---|
| `hide_only` | 635 ns | 631 ns | −4 ns | −0.0 % | −0.0 % |
| `fold_only` | 684 ns | 653 ns | −31 ns | −0.1 % | −0.0 % |
| `mixed_legacy` | 340 ns | 371 ns | +31 ns | +0.1 % | +0.0 % |
| `mixed_full` (realistic) | 209 ns | 6.02 µs | +5.81 µs | +10.2 % | +2.9 % |
| `mixed_pass_through` (extreme) | 68 ns | 9.46 µs | +9.39 µs | +16.6 % | +4.7 % |

Within ADR-024 perceptual imperceptibility budget; the 240 Hz scanout
budget (4170 µs) is impacted by < 0.25 %.

The zero-clone optimisation (passing the full `DirectiveSet` to legacy
`resolve()`, which already filters by variant internally, instead of
rebuilding a partitioned subset) reduced the legacy-heavy workloads
from 1.87 µs → 631 ns (`hide_only`, −66 %), 1.36 µs → 653 ns
(`fold_only`, −52 %), and 714 ns → 371 ns (`mixed_legacy`, −48 %).
Pass-through-dominated workloads are unchanged (the algebra
normalisation cost is the bottleneck, not the partition).

**Hybrid-bridge correctness**:
- Strict superset of legacy: every `Hide` / `Fold` / `EditableVT` legacy
  emits is still emitted (legacy path), plus pass-through variants are
  resolved through the algebra.
- Bridge proptest properties:
  - `hide_only_coverage_equivalence`: covered-line set equals legacy.
  - `fold_disjoint_equivalence`: identical Fold signatures for gap-disjoint folds.
  - `fold_touching_coverage_equivalence`: same hidden-line set for touching folds.
  - `pass_through_legacy_emits_none`: legacy emits zero of these; bridge preserves them.
  - `fold_and_inline_coexist_under_hybrid_bridge`: both directives survive on the same line.

**Open follow-ups** (not blockers for Accepted status):
- ShadowCursor / Editable virtual text re-host on the algebra (still
  routed through the legacy path in the bridge).
- Eventual Fold-in-algebra ADR (hybrid removal); requires `Span` to
  represent multi-line ranges or a new `Content::Fold` variant.
- partition zero-clone optimisation in the bridge (estimated ~50 % cost
  reduction on the legacy-only path).

### Migration (no backward compatibility)

This ADR explicitly opts out of backward compatibility per the project's
2026-05-03 directive that backward compatibility is no longer a constraint
during the foundation redesign.

| Site | Action |
|---|---|
| `kasane-core/src/display/mod.rs` | Delete `DisplayDirective` enum. Replace with `Display` algebra in new `kasane-core/src/display_algebra/`. |
| `kasane-core/src/display/resolve.rs` | Delete. Replace with `display_algebra/normalize.rs`. |
| `kasane-core/src/display/resolve/tests.rs` | Rewrite as proptest L1–L6 witnesses + scenario tests in `display_algebra/tests/`. |
| Plugin handler signatures | Change `Vec<DisplayDirective>` → `Display`. |
| WIT contract | Reconsidered (2026-05-04). The `display-directive` → `display` variant collapse was originally folded into the `kasane:plugin@3.0.0` bump, but on closer inspection (post-ADR-035-WIT-3.0 implementation) the collapse provides ~zero net benefit. The host's `display_algebra::bridge::directive_to_display` is 72 LOC of straightforward dispatch over 13 algebra constructors (`derived::*`); collapsing it onto the wire would move equivalent code to the guest SDK (the ergonomic helper layer plugin authors expect) without a net LOC reduction. Meanwhile every plugin author pays a forced rewrite + recompile for what is, in practice, a representation rename. The display-directive variant therefore remains on the wire under WIT 3.0; the algebra normalize / Pass A/B/C pipeline (`display_algebra`) continues to consume the bridge's output. A future bump may revisit if a concrete capability emerges that the wire-level collapse unlocks (e.g. plugin-emitted Then / Merge composition). |
| 10 bundled / fixture WASM plugins | Rebuild against 3.0.0. The `define_plugin!` macro is updated so plugins that already use the helper constructors compile with minimal source change. |
| `EditableVirtualText` (ADR-030 Level 5) | Becomes `Display::Replace { content: Content::Editable(...) }`. ADR-035 covers the time / selection facets that interlock with this. |
| ADR-030 Level 4 per-line locality invariant | Re-witnessed as a property of `Span` (single-line by construction). |

The migration **does not preserve plugin source compatibility** even where
helper constructors retain the old names — error types, return shapes, and the
WIT ABI all change in coordinated ways. Bundled plugins are rewritten in lock
step with the host change.

### Performance

Two effects pull in opposite directions:

- **Win**: tree fold over `Display` replaces the 12-variant `match` cascade in
  `resolve()`; cache locality improves; conflict-detection is `support()` set
  arithmetic, suitable for `RangeSet`-based pre-screening.
- **Loss**: `Box<Display>` allocations for `Then` / `Merge` add per-frame heap
  traffic. Mitigation: arena-allocate `Display` per frame (bumpalo or
  hand-rolled vec-with-indices), so traversal is pointer arithmetic and the
  whole tree is freed in one drop.

Target: `frame_warm_24_lines` ≤ 70 µs at 80×24 (matches current SLO; ADR-024).
Acceptance criterion: no regression vs the post-Scratch baseline of 56.7 µs at
the L1 cache hit ratio measured in `parley_pipeline/warm`.

### Risks

| Risk | Mitigation |
|---|---|
| Algebraic laws (L1–L6) sound but incomplete — real-world plugin combinations expose missing law | Proptest grammar generates random `Display` trees from a weighted distribution over primitives + composition; L4–L6 witnesses run for ≥10⁵ cases per CI run |
| `Merge` conflict surfacing breaks ADR-030 Level 6 transparency | `MergeConflict` carries the full set of displaced directives so a recovery handler can reconstruct what was suppressed; this is *strictly more information* than the current "lower-priority dropped silently" path |
| Per-frame arena allocator increases peak memory | Bench `peak_rss_during_frame` on `salsa_scaling/full_frame/200x60`; cap at 2× current peak |
| Plugin authors confused by `Then` vs `Merge` semantics | `define_plugin!` macro hides composition behind ergonomic syntax; raw algebra is a fallback for advanced authors |
| WIT 3.0 ABI churn | Acceptable per project directive; bundled plugins are the only consumer pinned to a version; external plugin authors are notified via `CHANGELOG.md` and a migration cookbook |

### Out of scope

- **Cross-buffer composition** — `Content::Reference(SegmentRef)` is reserved
  for ADR-036 (Cross-File Inlining). This ADR ships the type slot; the
  resolver treats `Reference` as opaque and forwards it.
- **Animation primitives** — `Display` is a static description of a frame.
  Transitions between frames are an orthogonal concern (future ADR on
  declarative animation).
- **AST-level edits** — operations on the editable graph (F3.1 in the
  innovative-features plan) are not display algebra. They live above the
  algebra and *generate* `Display` trees as one of their outputs.

### Implications

- `display/` directory restructured into `display_algebra/` (primitives,
  normalize, derived, conflict) and `display/` (DisplayMap projection, only).
- All 5945 lines of `display/*.rs` and `display/resolve/*.rs` are touched;
  net LOC change estimated at −1500 (denser, more structural code).
- WIT 3.0.0 — coordinated with ADR-035 to ship as a single ABI bump.
- `lens-development.md` (new doc) treats the lens as a `Display` generator,
  with the algebra exposed as the lens author's working surface.
- Salsa input shape: `display_for_line(file_id, line, lens_stack) -> Display`
  becomes the unit of caching; a single-lens toggle invalidates one entry per
  line, not the entire frame.
