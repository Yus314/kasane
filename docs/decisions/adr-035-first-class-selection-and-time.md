# ADR-035: First-Class Selection and Time

**Status**: Proposed (2026-05-03)

### Context

Two concepts that should be primitive in Kasane are currently shapes-of-data
rather than first-class types:

1. **Selection**. Kakoune's selection is the editor's defining concept. In
   Kasane it arrives via the protocol as `Vec<SelectionDescriptor>` and
   immediately decomposes into per-cursor `(line, column)` pairs scattered
   across `AppState`. Plugins read it through `AppView` accessors and have no
   way to *transform* selections, *save* a named selection set for later
   recall, *compose* multiple plugin-derived selection sets, or *inspect* the
   selection's algebraic relationships (union, intersection, difference) with
   another set.
2. **Time**. The buffer is a single state value. Undo lives entirely on the
   Kakoune side and is opaque to Kasane plugins. Time-travel features
   (Time-Travel Editing, Pair-Review Replay, "what did this lens produce
   yesterday?") have to be built ad hoc by each feature, with their own
   history backend, their own invalidation rules, and no shared abstraction.

The cost is asymmetric: today, *features* that need composable selections or
time travel must invent their abstractions; tomorrow, the same features could
*use* a shared primitive. This ADR proposes lifting both to the type system.

### Decision

Two coordinated lifts: `SelectionSet` as a first-class algebraic type, and
`Time` as a Salsa input dimension that all relevant queries take as a
parameter (defaulting to `Time::Now` at the call site to preserve readability).

#### 1. `SelectionSet`

```rust
pub struct SelectionSet {
    /// Sorted, non-overlapping selections, anchored to a specific buffer
    /// generation (the `BufferVersion` makes selections survive structural
    /// edits when projected forward).
    selections: Vec<Selection>,
    buffer: BufferId,
    generation: BufferVersion,
}

pub struct Selection {
    pub anchor: BufferPos,
    pub cursor: BufferPos,
    pub direction: Direction,  // Forward | Backward; primary head identity
}
```

**Operations** (closed: `SelectionSet -> SelectionSet`):

```rust
impl SelectionSet {
    // Construction
    pub fn from_kakoune(descriptors: &[SelectionDescriptor]) -> Self;
    pub fn singleton(sel: Selection) -> Self;
    pub fn empty(buffer: BufferId) -> Self;

    // Set-algebraic
    pub fn union(&self, other: &Self) -> Self;
    pub fn intersect(&self, other: &Self) -> Self;
    pub fn difference(&self, other: &Self) -> Self;
    pub fn symmetric_difference(&self, other: &Self) -> Self;

    // Pointwise transformation
    pub fn map<F: Fn(Selection) -> Selection>(&self, f: F) -> Self;
    pub fn filter<F: Fn(&Selection) -> bool>(&self, f: F) -> Self;
    pub fn flat_map<F: Fn(Selection) -> Vec<Selection>>(&self, f: F) -> Self;

    // Pattern-driven (require a SyntaxProvider capability)
    pub fn extend_to_pattern(&self, pat: TreeSitterPattern) -> Self;
    pub fn split_on(&self, pat: TreeSitterPattern) -> Self;

    // Identity / introspection
    pub fn is_disjoint(&self, other: &Self) -> bool;
    pub fn covers(&self, pos: BufferPos) -> bool;
    pub fn primary(&self) -> Option<&Selection>;

    // Persistence (named registers)
    pub fn save(&self, name: &str) -> Result<(), SaveError>;
    pub fn load(name: &str, buffer: BufferId) -> Result<Self, LoadError>;
}
```

**Projection back to Kakoune**: `SelectionSet` produced by Kasane is applied to
Kakoune via the existing `select <ranges>` command. Kakoune remains the source
of truth for the *current* selection; Kasane owns the *operations* on
selections. When a plugin computes `let new = current.union(&saved); new.apply()`,
that invocation issues `select` to Kakoune; on the next protocol echo, the new
selection arrives as the canonical `current`.

**Identity and equality**: `SelectionSet` is structurally compared. Two sets
with the same selections in the same buffer at the same `BufferVersion` are
equal. Set algebra is defined on the same buffer/generation; cross-generation
operations require explicit `project_to(generation)`.

**Plugin-saved sets vs Kakoune registers**: Kakoune already has cursor / mark
registers (`'`, etc). `SelectionSet::save` is **not** the same — it persists
multi-cursor sets with intent metadata (set name, owning plugin, optional TTL).
Kakoune registers stay as-is; Kasane's named-set store is additive.

**Half-open ranges and adjacency.** Selections are half-open `[min, max)`.
`SelectionSet::from_iter` (and therefore `union`) coalesces *adjacent*
selections — `[0, 5)` and `[5, 10)` collapse to `[0, 10)` — in addition to
overlapping ones. Rationale: plugin-piecewise constructions (e.g. a syntax
plugin emitting one selection per token) should yield the coherent range
when the pieces touch. A plugin that needs to preserve the seam can
suppress the merge by emitting a one-position gap, or by reading
`SelectionSet::iter()` before the call that would coalesce. This was
confirmed during implementation (2026-05-03) and is normative.

#### 2. `Time`

```rust
pub enum Time {
    Now,
    At(VersionId),
}

pub struct VersionId(u64);  // Monotonic, opaque

pub trait HistoryBackend: Send + Sync {
    fn snapshot(&self, t: VersionId) -> Option<Snapshot>;
    fn current_version(&self) -> VersionId;
    fn earliest_version(&self) -> VersionId;
    fn iter_range(&self, range: Range<VersionId>) -> Box<dyn Iterator<Item = Snapshot>>;
}
```

`Time::Now` is a constant; `Time::At(v)` requires the configured `HistoryBackend`
to be able to materialise the snapshot. If a query asks for a version the
backend has evicted, the query returns `Err(HistoryError::Evicted)`.

**Backends** (pluggable via `kasane.kdl` `history { backend = ... }`):

| Backend | Trait impl | Use case |
|---|---|---|
| `InMemoryRing` | `kasane-core/src/history/in_memory.rs` | Default, last 256 versions, fixed memory |
| `GitBacked` | `kasane-core/src/history/git.rs` (feature-gated) | Each commit is a `VersionId`; near-infinite range |
| `RocksDb` | `kasane-history-rocksdb` (separate crate, feature-gated) | Long-running session, persistent |

`InMemoryRing` is the default to keep the no-config experience identical to
today. `GitBacked` and `RocksDb` are opt-in.

#### 3. Time-parameterised queries

All Salsa queries that depend on buffer or display state grow a `Time`
parameter:

```rust
// Old
fn buffer_text(file: FileId) -> Arc<str>;
fn lens_directives(file: FileId, lens: LensId) -> Arc<Vec<Display>>;
fn selection_set(buffer: BufferId) -> Arc<SelectionSet>;

// New
fn buffer_text(file: FileId, at: Time) -> Arc<str>;
fn lens_directives(file: FileId, lens: LensId, at: Time) -> Arc<Vec<Display>>;
fn selection_set(buffer: BufferId, at: Time) -> Arc<SelectionSet>;
```

`Time::Now` is the default at the *call site*: most code reads
`state.buffer_text_now(file)` (one-liner that supplies `Time::Now`). Code that
needs explicit time uses the full form.

**Salsa interaction**: `Time` becomes a Salsa input dimension. Queries at
`Time::Now` invalidate exactly when the underlying inputs change (today's
behaviour). Queries at `Time::At(v)` for any `v < current` are *immutable* and
cache forever for that `v`; they only incur cost on first computation per
version. This is the right cache shape for replay / time-travel features.

#### 4. ShadowCursor / EditableSpan reformulation

`ShadowCursor` (`state/shadow_cursor.rs`, 927 LOC) is rewritten on top of the
new primitives:

- The "anchor" of a shadow cursor is a `Selection` in the algebra above.
- The `working_text` lives in a per-version overlay layer that the
  `HistoryBackend` knows about — committing the shadow edit allocates a new
  `VersionId` and projects via the existing `exec -draft` path.
- `EditProjection::Computed { forward, inverse }` (introduced in the Phase 0
  plan) becomes a function pair returning `SelectionSet` deltas, not
  text-byte deltas — the plugin author writes against the algebra.

This is a structural simplification: ShadowCursor's current ad-hoc state
machine collapses into "a selection, a working content, and a version stamp."

### Implementation Status (2026-05-03)

**Status remains Proposed.** The skeleton landed in parallel with
ADR-034 to derisk the type design, but production wiring is pending —
the core requires more invasive surgery than ADR-034 (per-query `Time`
threading touches every Salsa query that reads buffer or selection
state). Acceptance is gated on the wiring step below.

**Landed**:
- `kasane-core/src/state/selection.rs` — `Selection`, `Direction`,
  `BufferPos`, `BufferId`, `BufferVersion` types (165 LOC).
- `kasane-core/src/state/selection_set.rs` — `SelectionSet` set algebra
  (union/intersect/difference/symmetric_difference/map/filter/flat_map)
  plus per-(plugin, name) save/load store. (305 LOC)
- `kasane-core/src/history/{mod.rs, in_memory.rs}` — `Time`,
  `VersionId`, `Snapshot`, `HistoryBackend` trait, `InMemoryRing`
  default backend with FIFO eviction. (320 LOC)
- 35 hand-built tests + 20 proptest fixtures witnessing set-algebra
  laws (idempotency, commutativity, associativity, identity,
  absorption, distributive, difference characterisation, symmetric
  difference, disjointness ↔ intersect-empty), plus 6 InMemoryRing
  unit tests.

**Pending for Accepted status**:
- ✅ **Salsa Time-aware queries — full integration (2026-05-03)**.
  Three Salsa-tracked queries demonstrate the Time-aware pattern
  for distinct payload types, with `HistoryInput` lifted as a
  first-class Salsa input:

  - `text_at_time(db, BufferInput, HistoryInput, Time)
    -> Option<Arc<str>>` — Time::Now projects `BufferInput.lines`
    to plain text; Time::At(v) reads from `history.backend(db)`.
  - `selection_at_time(db, HistoryInput, Time)
    -> Option<SelectionSet>` — Time::Now resolves through
    `history.current_version(db)`; Time::At(v) reads the snapshot's
    selection. Same invalidation guarantees as text_at_time.
  - `display_directives_at_time(db, HistoryInput, Time)
    -> NormalizedDisplay` — ADR-034 + ADR-035 integration query.
    Synthesises Decorate primitives from the SelectionSet at the
    requested Time and runs `algebra_normalize`. Cache key
    `(HistoryInput, Time)`.

  `HistoryInput { backend: Arc<InMemoryRing>, current_version:
  VersionId }` is initialised in `SalsaInputHandles::new` with a
  placeholder ring; `sync_inputs_from_state` swaps the backend to
  `Arc::clone(&state.history)` and pushes
  `state.history.current_version()` to `current_version` every
  frame. Production code paths can now call any of the three
  queries against the same backend the apply auto-commit hook
  writes to.

  Test coverage: 15 unit tests in `salsa_queries::time_query_tests`
  + 4 production-path integration tests in
  `tests/salsa_history_sync.rs`. Cache-shape pattern proved for
  three payload types; bulk per-query migration uses these as the
  template.
- ✅ **Canonical SelectionSet field on `InferenceState` (2026-05-03)**
  — `InferenceState` gains a `pub selection_set: SelectionSet` field
  populated by `AppState::apply` from the heuristic detector's
  output via `selections_to_set`. The projection runs once per
  protocol echo with a `BufferVersion` matching the simultaneous
  history commit, so `AppView::current_selection_set()` (new direct
  accessor) and `AppView::selection_at(Time::Now)` agree on the
  active SelectionSet without history-lookup overhead. `SelectionSet`
  gained `Default::default()` (empty set, unnamed buffer, INITIAL
  generation) to support the field's default value. 3 new
  integration tests pin the empty-state baseline, the apply →
  current consistency, and the clear-on-restyle behaviour. Replaces
  the original §Migration line "Replace state::observed selection
  fields with SelectionSet" — there is no observed selection field
  to replace (Kakoune's `draw` only carries the primary
  `cursor_pos`); the canonical SelectionSet is derived state and
  belongs in `InferenceState`, not `ObservedState`.
- ✅ **`AppState.history` wiring (2026-05-03)** — `Arc<InMemoryRing>`
  field added to `AppState`; `commit_snapshot` and `text_at(Time)`
  methods landed; 9-test integration suite (`tests/history_roundtrip.rs`)
  witnesses round-trip for `Time::At(v)` and `Time::Now`, FIFO
  eviction, Arc-shared history across cloned states, bounded Debug
  output.
- ✅ **External-consumer dogfood (2026-05-03)** —
  `examples/selection-algebra-native/` exercises every `SelectionSet`
  operation from a workspace-external crate; algebraic-law spot
  check (idempotency / commutativity / absorption / distributive)
  passes at runtime.
- Replace `state::observed` selection fields with the new `SelectionSet`
  field per buffer.
- Rewrite `AppView::selection*` accessors as `selection_set(buffer, at)`.
- Add `Time` parameter to every Salsa query reading buffer or selection
  state (`Time::Now` short-circuits to today's behaviour).
- Re-host `state::shadow_cursor` on `Selection` + `Time` primitives.
- WIT 3.0 — coordinated with ADR-034 (already accepted).
- ✅ **`AppState::apply()` auto-commit hook (2026-05-03)** — when a
  protocol message sets `DirtyFlags::BUFFER_CONTENT`, the apply path
  now projects `observed.lines` to plain text via `lines_to_text` and
  calls `commit_snapshot`. `Time::Now` reflects the latest Kakoune
  protocol echo without explicit caller intervention. Lossy by design
  (drops style payloads). 5-test integration suite
  (`tests/history_apply_hook.rs`) covers `Draw` round-trip, multi-version
  monotonicity, empty buffer, `\n`-joined multi-line, and the
  `DrawStatus`-does-not-commit invariant.
- ✅ **`AppView::text_at` / `AppView::history` accessors (2026-05-03)**
  — plugin-facing entry point for time-travel queries. `text_at(Time)`
  delegates to `AppState::text_at`; `history()` exposes the
  `&dyn HistoryBackend` for advanced consumers (version enumeration,
  earliest/current introspection). 5-test integration suite
  (`tests/history_app_view.rs`) covers current-text reads, past-version
  reads, history metadata inspection, version-range iteration, and
  the empty-state None case — all from the read-only `AppView`
  perspective that plugin handlers receive.
- ✅ **`Snapshot.selection` extension + `selection_at(Time)` (2026-05-03)**
  — `Snapshot` now carries a `SelectionSet` alongside `text`;
  `HistoryBackend::commit` and `AppState::commit_snapshot` take the
  selection as a required parameter. New `AppState::selection_at(Time)`
  and `AppView::selection_at(Time)` accessors mirror the text path.
  6-test integration suite (`tests/history_selection.rs`) covers
  per-snapshot round-trip via both `AppState` and `AppView`,
  `Time::Now` returns the latest, text and selection share the same
  `VersionId`, empty-state None.
- ✅ **Protocol-derived selection projection (2026-05-03)** — apply
  auto-commit now projects `inference.selections` (the heuristic
  detector's output) into the canonical `SelectionSet` via
  `selections_to_set` (Coord i32 → BufferPos u32 with negative-clamp;
  per-cursor `is_primary` does not have a direct representation in the
  order-independent set, so the primary surfaces through
  `SelectionSet::primary()` after sort). The
  `auto_commit_apply_with_styled_atoms_projects_selection` test
  witnesses end-to-end: a Draw containing styled selection-bg atoms
  produces a non-empty SelectionSet on `selection_at(Time::Now)`.
  Default-style draws still produce empty sets, pinned by
  `auto_commit_via_apply_pairs_text_with_projected_selection`.
- ✅ **`AppView::selection_set(buffer, at)` accessor (2026-05-03)** —
  the §Migration target accessor. Buffer-filtered, Time-aware
  `Option<SelectionSet>` read; returns `None` when the snapshot at the
  requested time references a different buffer. Five new integration
  tests in `tests/history_selection.rs` (matching-buffer round-trip,
  mismatched-buffer rejection, latest-snapshot via `Time::Now`,
  cross-buffer rejection at `Time::Now`, empty-history None). The
  legacy heuristic-based `AppView::selections()` remains in place
  (returns the older `derived::Selection` type) and is retired in a
  follow-up milestone once the auto-commit projection covers all the
  heuristic's recall cases.
- ✅ **ShadowCursor §Migration Phase 2 — `EditableSpan` field
  consolidation (2026-05-04)** — the per-span `anchor_line: usize`
  + `buffer_byte_range: Range<usize>` pair collapses to a single
  `projection_target: Selection` field. By invariant
  `anchor.line == cursor.line` (Mirror projections target one
  buffer line); `min().column..max().column` recovers the byte
  range. `EditableSpan::projection_target()` (the read-only
  accessor introduced in Phase 1) becomes the field itself; the
  ShadowCursor accessor `buffer_projection_target(spans)` returns
  the field by index. `build_mirror_commit` reads
  `projection_target` directly to compute the `exec -draft`
  command; one Hippocratic check (`col_min == col_max`) replaces
  the previous `Range::is_empty`. Tests collapsed from 6 expanded
  literals to a `mk_span(line, start, end)` test helper; no
  external callsite changes (the field shape is private to
  `state::shadow_cursor`; downstream `display`,
  `display_algebra::primitives`, `display::unit`, and
  `plugin::safe_directive` only carry `EditableSpan` as a payload
  type and never touch the consolidated fields).
- ✅ **ShadowCursor §Migration Phase 3 — algebraic `BufferEdit`
  (2026-05-04)** — the commit pipeline splits into an algebraic
  layer and a serialization layer. `BufferEdit { target:
  Selection, original: String, replacement: String }` is the
  algebraic source of truth; `mirror_edit(shadow, span,
  line_count) -> Option<BufferEdit>` computes it (returning
  `None` for Navigating phase, Hippocratic noops, out-of-range
  anchors, and PluginDefined projections); `edit_to_commands(edit)
  -> Vec<Command>` serializes a `BufferEdit` into the Kakoune
  `exec -draft` substitute / insert command(s). The pre-existing
  `build_mirror_commit` becomes a thin compose of the two,
  preserving the dispatch-side entry point. `BufferEdit` is the
  natural payload for a future plugin commit-intercept hook (a
  plugin reads / transforms / vetoes the edit before it serializes
  to Kakoune). Tests assert structural shape at the
  `BufferEdit` layer (target Selection equality, original /
  replacement text, Hippocratic noop detection) and round-trip
  the keysym-encoded command string through a
  `render_kakoune_command` helper, eliminating Debug-format
  fragility. The keyboard-handling state machine
  (`handle_shadow_cursor_key`) intentionally remains in
  synthetic grapheme space — the original Phase 3 sketch's
  "smaller surface" half does not re-shape onto buffer-space
  `SelectionSet` algebra (cursor_grapheme_offset indexes
  graphemes within working_text, not buffer columns), so that
  half is dropped from the migration plan rather than deferred.
  See ShadowCursor §Migration Phase 4 below for the version-stamp
  follow-up.
- ✅ **ShadowCursor §Migration Phase 4 — `VersionId` activation
  stamp (2026-05-04)** — the active shadow edit now carries the
  history `VersionId` it was authored against.
  `ShadowPhase::Editing` gains a `base_version: VersionId` field
  set at the `Navigating → Editing` transition (the first
  printable keystroke) and preserved across all in-place
  keystroke edits within the span. `handle_shadow_cursor_key`
  takes `current_version: VersionId`, consulted only at the
  activation transition; the production caller in
  `handle_key_pre_dispatch` reads it from
  `app_state.history.current_version()`. `BufferEdit` surfaces
  the stamp; `is_stale_against(current)` returns true when the
  buffer has advanced past the version the edit was authored
  against — a downstream consumer can use this to gate commit,
  prompt the user, or replay the edit on the new base. The stamp
  also lets a consumer compose the edit with `Time::At(v)`
  queries to materialise the buffer state it was authored against
  (e.g. for diff visualisation or three-way conflict resolution).
  4 new tests cover the activation stamp, the in-place
  preservation invariant, the surface through `mirror_edit`, and
  the staleness predicate. Test churn was reduced via an
  `mk_editing(working, original, cursor)` helper that defaults
  `base_version` to `VersionId::INITIAL`; 16 existing test
  constructions migrated. With Phase 4 landed, the in-module
  migration docstring records that the ShadowCursor §Migration is
  complete to the extent the keyboard-handler half permits — no
  further deferred phases remain in this module. The §Migration
  table row "ShadowCursor rewritten on Selection + Time
  primitives; LOC estimated ~400 (vs 927)" is realised in
  spirit (algebraic edit shape + version stamp) but not in LOC
  (the keyboard-handler grapheme arithmetic stays).
- ✅ **WIT 3.0 ABI bump — selection-set + time + history
  (2026-05-04)** — implements the ADR-035 portion of the
  paper-design freeze below. Bumps `kasane:plugin@2.0.0` →
  `@3.0.0`. Adds: `selection-set` value-record + 7 set-algebra
  free functions; `selection-record` + `buffer-pos` +
  `selection-direction` supporting types; `time` variant +
  `version-id` alias; new `history` interface
  (`current-version`, `earliest-version`, `text-at`,
  `selection-at`); `current-selection-set` accessor on
  `host-state`. Removes: legacy heuristic `selection` record
  + `get-selection-count` / `get-selection` /
  `get-all-selections` triplet. Host bindings (`kasane-wasm`)
  serve all new functions over native primitives that landed
  in the prior milestones. The `selection-algebra` example
  plugin migrates from the legacy iterator pattern to a
  single `current-selection-set` call. All 12 example /
  fixture / guest WASM plugins rebuilt against the new ABI
  as `wasm32-wasip2` Components. 25 manifest files + the
  `HOST_ABI_VERSION` constants in `kasane-plugin-package`
  and `kasane/plugin_cmd/templates` bumped to "3.0.0". The
  display-directive → display variant collapse (the
  ADR-034 driver portion of WIT 3.0) is **deferred
  indefinitely** per the post-implementation review
  recorded in §"Drivers reconsidered (2026-05-04)" —
  the host bridge is 72 LOC of straightforward dispatch
  and the collapse moves equivalent code onto the SDK
  side without a net reduction.
- ✅ **`SelectionSet::to_kakoune_command()` projection
  (2026-05-04)** — closes the §Decision "Projection back to
  Kakoune" line which was previously documentation-only. Encodes
  a `SelectionSet` as a Kakoune `:select` command in the
  `<line>.<col>,<line>.<col>` per-range syntax (1-indexed,
  byte-addressed, anchor-then-cursor). Returns `None` for an
  empty set (Kakoune `:select` requires ≥ 1 range). Direction is
  preserved by emitting the anchor position first; multi-line
  selections produce one range whose anchor and cursor sit on
  different lines. With this method landed, the round-trip
  described in the §Decision (`current.union(&saved).apply()` →
  `select` to Kakoune → next protocol echo carries the new
  canonical selection) is wired end-to-end. 5 new integration
  tests cover the empty / singleton / multi-selection /
  direction-preservation / multi-line cases via a
  `render_kakoune_command` helper that decodes
  `KasaneRequest::Keys` back to a readable string.

The §Migration table below remains the target shape; Acceptance signals
the migration is complete.

### Migration (no backward compatibility)

| Site | Action |
|---|---|
| `kasane-core/src/state/observed.rs` (selection fields) | Replaced by `SelectionSet` field per buffer |
| `AppView::selection*` accessors | Replaced by `AppView::selection_set(buffer, at)` |
| `kasane-core/src/state/shadow_cursor.rs` | Rewritten on `Selection` + `Time` primitives; LOC estimated ~400 (vs 927) |
| All Salsa queries reading buffer / selection state | Take `at: Time` |
| `kasane-core/src/history/` | New module — backend trait + in-memory ring impl |
| `Cargo.toml` features | New `history-git`, `history-rocksdb` features |
| WIT contract | Bump to `3.0.0` (coordinated with ADR-034). New `selection-set` value-record + `time` variant + `version-id` alias + 4 history-accessor functions; legacy heuristic `selection` record + `get-selection*` triplet removed. The wire shape is frozen in §"WIT 3.0 Wire Shape (paper design)" below — implementation is one PR. |
| Plugins reading selection | Source rewrite to `SelectionSet` API. |
| Plugin-defined selection extensions (`examples/wasm/selection-algebra`) | Promoted to first-class APIs; the example becomes documentation rather than a workaround. |

### Performance

`SelectionSet` operations run on sorted-disjoint vectors; union / intersect /
difference are O(n + m). Pattern operations defer to tree-sitter cost.
Acceptance criterion: a `SelectionSet::union` of two 1000-cursor sets completes
in under 100 µs.

`Time::Now` queries cost the same as today (Time becomes a constant Salsa key
with a fast path). `Time::At(v)` adds one history backend lookup per query
(O(1) for `InMemoryRing`, O(log n) for `RocksDb`, variable for `GitBacked`).

The history backend is the memory-cost knob:

- `InMemoryRing(256)`: ~10 MB peak for typical buffers (256 × ~40 KB diff
  snapshots).
- `GitBacked`: bounded by repo size; reads pay git object decompression cost.

### Risks

| Risk | Mitigation |
|---|---|
| `Time` parameter pollutes every query signature | Mitigated by `query_now()` convenience wrappers; raw `Time` only appears at time-travel call sites |
| Memory growth with `InMemoryRing` on long sessions | Fixed-size ring with FIFO eviction; documented in `history.md` |
| `SelectionSet::save` namespace collisions | Names are scoped to (plugin_id, name); plugins can't accidentally overwrite each other's saves |
| Kakoune's own undo and Kasane's `Time` divergence | Kakoune is source of truth for buffer history; Kasane's `Time` indexes into a *projection* of Kakoune's history that Kasane has observed. Versions Kakoune undid past are still in Kasane's history (so Kasane can show "the file as it was in this Kasane session even after Kakoune undo'd"). The `history.md` doc spells out the projection rules. |
| Pattern-driven selection ops require SyntaxProvider, may be unavailable | `extend_to_pattern` returns `Result<Self, NoSyntaxProvider>`; ergonomic fallback `extend_to_pattern_or_self` provided |
| Plugin-saved sets persist across editor restarts? | Default: session-scoped (cleared on restart). Opt-in persistence via `SelectionSet::save_persistent(name)` writes to `~/.local/state/kasane/named-selections/`. |

### Out of scope

- **Collaborative SelectionSet merging across users** — this ADR makes the
  type *shape* that supports it (set algebra, named saves) but the network /
  CRDT layer is a future ADR.
- **Time-travel for non-buffer state** — settings, plugin state, etc. are not
  versioned. `Time` indexes buffer + display state only.
- **Undo as a first-class operation in the algebra** — Kasane defers to
  Kakoune's undo. `Time::At(v)` is a *read* primitive; "make this version
  current" is `state.checkout(v)`, which issues an explicit Kakoune restore.

### Implications

- `kasane-core/src/state/selection_set.rs` (new), `selection.rs` (new),
  `history/mod.rs` (new), `history/in_memory.rs` (new).
- `state/observed.rs` selection fields removed.
- `shadow_cursor.rs` rewritten (~50% LOC reduction).
- All plugin-facing APIs take `at: Time`. `define_plugin!` macro generates
  `*_now` convenience methods so plugins reading current state stay terse.
- WIT 3.0.0 introduces `resource selection-set` and `resource time` with the
  full operation surface above.
- `docs/semantics.md` gains a §"Selection Algebra" section and §"Time and
  History" section, both authoritative.
- ADR-030's observed/policy split is preserved: `Time` is observed (it's
  derived from protocol echoes); `HistoryBackend` config is policy.

### WIT 3.0 Wire Shape (paper design, 2026-05-04)

This sub-section freezes the wire-shape decisions for the WIT
`kasane:plugin@3.0.0` ABI bump so the implementation can be one
pass. WIT 3.0 may not introduce features beyond what is listed
here without a follow-up ADR; doing so would re-create the
"two ABI breaks" trap that ADR-031 §動機-5 was written to
prevent. ADR-031's "Phase 10 Wire Shape" sub-section (around
line 2388) is the template; the same discipline applies here.

#### Drivers

WIT 3.0 is driven by ADR-035 only after a 2026-05-04
reconsideration of the ADR-034 driver portion (see "Drivers
reconsidered" below):

| ADR | What it requires from WIT 3.0 |
|---|---|
| ADR-035 | New `selection-set` value-record + `time` variant + history accessor functions; legacy heuristic `selection` record removed |
| ~~ADR-034~~ | ~~`display-directive` → `display` collapse~~ — **deferred indefinitely** post-implementation review; the host bridge is 72 LOC of straightforward dispatch and the collapse moves equivalent code onto the SDK side without a net reduction. See "Drivers reconsidered (2026-05-04)" below. |
| ADR-032 W5 (conditional) | `path`, `brush`, `stroke` types if the Vello adoption decision lands positive — currently **out of scope** for WIT 3.0; if W5 lands later it bumps to WIT 3.1.0 (additive) or WIT 4.0.0 (breaking), per the ADR-032 §SDK-migration analysis |

WIT 3.0 ships ADR-035 only. ADR-032 W5 is decoupled to keep
the bump bounded; the ADR-034 wire-level collapse is deferred
indefinitely.

#### Drivers reconsidered (2026-05-04)

After landing the ADR-035 driver portion of WIT 3.0
(2026-05-04 commit `0e75a54a`), I revisited the ADR-034
driver portion and concluded it should not ship.

**Original argument (paper design)**: collapse the legacy
`display-directive` (11-case variant) onto the `display`
algebra (4-leaf variant) on the wire so plugins emit raw
algebra leaves, eliminating the host-side
`bridge::directive_to_display` translator and giving the
codebase "one representation" rather than two.

**Re-examination**:

1. The host translator is **72 LOC** of straightforward
   per-variant dispatch over 13 `derived::*` constructors
   (`hide_lines`, `fold`, `style_line`, etc.) — not a deep
   plumbing tail, just a thin sugar layer.
2. Removing it doesn't eliminate equivalent code; it moves
   it to the **guest SDK side**, because plugin authors
   need the same ergonomic helpers (`hide(line_range)`,
   `fold(range, summary)`, etc.) — emitting raw `Replace
   { span, content }` leaves directly is verbose and
   error-prone (Span construction alone is non-trivial).
3. The "one representation" benefit is **theoretical
   aesthetic**, not load-bearing. Both representations
   carry the same information; the bridge is a pure
   bijection (modulo the documented lossy metadata fields
   the rustdoc already enumerates).
4. The migration cost is **real and concrete**: every
   plugin author pays a forced rewrite (the `display-
   directive` constructors they currently use disappear)
   plus a forced recompile against the new SDK.
5. No concrete capability is unlocked. The original ADR
   text mentioned `then` / `merge` "as record-level
   constructors" but the paper design itself walked that
   back, declaring them host-side normalisation operators.
   With composition staying host-side, the wire shape
   gains nothing actionable.

**Conclusion**: Defer the `display-directive` →
`display` wire collapse indefinitely. WIT 3.0 ships
without it. The host bridge stays as a stable internal
sugar layer; plugins continue to emit `list<display-
directive>`; the algebra normalize / Pass A/B/C
pipeline continues to consume the bridge's output.

A future ABI break (WIT 4.0 or later) may revisit if a
concrete capability emerges that the wire-level collapse
unlocks (e.g. plugin-emitted Then / Merge composition,
which would require host changes anyway). Until that
capability surfaces, the collapse is pure churn.

This reconsideration also updates the "Decision summary"
table below: row 3 ("`display`") moves from
**plugin-visible** to **deferred** status; rows 1 / 2 /
4 / 5 ship as planned.

#### Decision summary

| Feature | Plugin visibility | WIT additions | WIT removals | Host plumbing |
|---|---|---|---|---|
| 1. `selection-set` (canonical multi-cursor algebra) | plugin-visible | new `selection-set` record + 8 free functions | legacy `selection` record + `get-selection*` triplet | All landed (`state::selection_set`) |
| 2. `time` (history coordinate) | plugin-visible | new `time` variant + `version-id` alias + 4 free functions | none | All landed (`history::Time`, `salsa_queries::*_at_time`) |
| 3. ~~`display` (algebra leaves)~~ | **deferred** | ~~new `display` variant (4 leaves) + supporting records~~ | ~~legacy `display-directive` variant (12 cases)~~ | Stays internal — see "Drivers reconsidered" |
| 4. `buffer-edit` (algebraic shadow-cursor commit) | plugin-visible | new `buffer-edit` record | none | All landed (`shadow_cursor::BufferEdit`) |
| 5. `current-selection-set` host accessor | plugin-visible | one new `host-state` function | none | All landed (`AppView::current_selection_set`) |

Every host-side primitive is **already implemented** as native
Rust code (the ADR-035 / ADR-034 / ADR-037 work that landed
2026-05-03 / 04). WIT 3.0 is the wire-shape promotion of work
that already exists internally — *not* a design phase that
discovers new requirements.

#### 1. `selection-set` value-record

Following the existing WIT idiom of records + free functions
(no `resource` types appear in the current `kasane:plugin@2.0.0`
WIT), `selection-set` is a value-record with set-algebra
operations as free functions. Resources were considered but
rejected: the host-tracked-handle ergonomics they enable do not
benefit set algebra (which is naturally value-typed), and they
would introduce the only `resource` in the entire WIT, raising
the SDK guest-binding complexity floor.

```wit
record buffer-pos {
    line: u32,
    column: u32,
}

enum selection-direction {
    forward,
    backward,
}

record selection-record {
    anchor: buffer-pos,
    cursor: buffer-pos,
    direction: selection-direction,
}

record selection-set {
    /// Sorted by `min()`, disjoint, all anchored to the same
    /// buffer + generation by construction.
    selections: list<selection-record>,
    /// Buffer this set is anchored to. Must match the buffer
    /// Kakoune is currently focused on for projection back via
    /// `selection-set-to-kakoune-command`; otherwise the
    /// projection lands positions in the wrong buffer.
    buffer: string,
    /// Buffer-version this set is anchored to. Set algebra is
    /// defined on the same generation; cross-generation
    /// operations require explicit `selection-set-project-to`
    /// (deferred to a follow-up).
    generation: u64,
}

variant set-algebra-error {
    /// `union` / `intersect` / `difference` / `symmetric-difference`
    /// applied to two sets anchored to different buffers.
    buffer-mismatch(tuple<string, string>),
    /// Cross-generation operation without explicit `project-to`.
    generation-mismatch(tuple<u64, u64>),
}

variant save-error {
    /// The supplied name is empty or contains a `:` (reserved for
    /// scoping by `(plugin-id, name)`).
    invalid-name,
}

variant load-error {
    not-found,
    buffer-mismatch(tuple<string, string>),
}
```

Free functions on `selection-set`:

```wit
selection-set-empty: func(buffer: string, generation: u64) -> selection-set;
selection-set-singleton: func(sel: selection-record, buffer: string, generation: u64) -> selection-set;
selection-set-union: func(a: selection-set, b: selection-set) -> result<selection-set, set-algebra-error>;
selection-set-intersect: func(a: selection-set, b: selection-set) -> result<selection-set, set-algebra-error>;
selection-set-difference: func(a: selection-set, b: selection-set) -> result<selection-set, set-algebra-error>;
selection-set-symmetric-difference: func(a: selection-set, b: selection-set) -> result<selection-set, set-algebra-error>;
selection-set-save: func(set: selection-set, name: string) -> result<_, save-error>;
selection-set-load: func(name: string, buffer: string) -> result<selection-set, load-error>;
selection-set-to-kakoune-command: func(set: selection-set) -> option<string>;
```

`map` / `filter` / `flat-map` are deliberately omitted from
WIT 3.0 — they require host-side closures, which WIT does not
express ergonomically. Plugins reach into `selections` directly
and rebuild via `selection-set-empty` + repeated union.

#### 2. `time` variant

```wit
type version-id = u64;

variant time {
    %now,
    at(version-id),
}
```

Free functions:

```wit
/// Current history version (the `Time::Now` materialisation).
history-current-version: func() -> version-id;
/// Earliest version still in the backend's window.
history-earliest-version: func() -> version-id;
/// Materialise a snapshot's text. Returns none for evicted versions.
history-text-at: func(at: time) -> option<string>;
/// Materialise a snapshot's selection-set. Returns none for evicted versions.
history-selection-at: func(at: time) -> option<selection-set>;
```

The four `history-*` functions live in the new `history`
interface (parallel to `host-state`) so plugins that don't
need history don't pull the symbols.

#### 3. `display` variant (algebra leaves) — DEFERRED

> **Update (2026-05-04, post-implementation review)**: this
> sub-section's collapse is **deferred indefinitely**. See
> "Drivers reconsidered (2026-05-04)" above for the
> reasoning. The shapes below are retained in the paper
> design as a record of what *would* land if a future ABI
> break revisits this — but no implementation work is
> scheduled.

The legacy `display-directive` variant (12 cases — `Hide`,
`Fold`, `StyleLine`, `StyleInline`, `InsertBefore`,
`InsertAfter`, `InsertInline`, `Gutter`, `VirtualText`,
`EditableVirtualText`, `InlineBox`, `HideInline`) collapses to
4 algebra leaves matching `display_algebra::primitives::Display`:

```wit
record byte-range {
    start: u32,
    end: u32,
}

enum side {
    left,
    right,
    full,
}

variant content {
    empty,
    text(list<atom>),
    fold(tuple<u32, u32, list<atom>>),  // (line-range-start, end, summary)
    hide(tuple<u32, u32>),               // (line-range-start, end)
    editable(tuple<list<atom>, list<editable-span>, edit-spec>),
    inline-box(inline-box-id),
    gutter(element),
}

record span {
    line: u32,
    side: side,
    byte-range: byte-range,
}

variant display {
    identity,
    replace(tuple<span, content>),
    decorate(tuple<span, style>),
    anchor(tuple<span, content, anchor-position>),
}
```

Plugins emit `list<display>`; the host runs the existing
`algebra_normalize` + Pass A/B/C pipeline (`display_algebra`).
`then` / `merge` are *normalisation operators on the host*, not
wire constructors — plugins do not express composition; they
emit independent leaves and the host composes.

#### 4. `shadow-edit` record (renamed from `buffer-edit`)

> **Update (2026-05-04 follow-up)**: shipped under the name
> `shadow-edit` to avoid the shape conflict with the WIT 2.0
> `buffer-edit` (used by the `edit-buffer` command effect, with
> a `(start-line, start-column, end-line, end-column,
> replacement)` shape). The two records now coexist:
> `buffer-edit` keeps its programmatic-edit role; `shadow-edit`
> carries the richer Phase 3 / 4 algebraic shape exclusively
> for the commit-intercept hook surface. Plugins that don't
> override `intercept-buffer-edit` get a default impl returning
> `pass-through` from `kasane-plugin-sdk-macros::defaults`.

The Phase 3 / 4 algebraic shape of a shadow-cursor commit:

```wit
record buffer-edit {
    target: selection-record,
    original: string,
    replacement: string,
    base-version: version-id,
}
```

The naming choice landed as `shadow-edit` for clarity
about the record's origin (shadow-cursor commits) and to
keep `buffer-edit` reserved for the programmatic
`edit-buffer` command. No free functions are exposed;
the record is read-only from the plugin perspective.
The `intercept-buffer-edit` plugin-api export consumes /
produces it.

#### 5. `current-selection-set` accessor in `host-state`

Replaces the legacy heuristic `get-selection*` triplet:

```wit
// In host-state interface:

/// Get the canonical SelectionSet for the focused buffer at
/// the current Time. Returns the empty set when no protocol
/// echo has populated the canonical set yet.
current-selection-set: func() -> selection-set;
```

#### Removals from WIT 2.0

| Symbol | Reason |
|---|---|
| `record selection { anchor, cursor, is-primary }` | Replaced by `selection-record` (carries direction, no ad-hoc primary flag — primary inferred from sorted ordering) |
| `host-state.get-selection-count` | Replaced by `selection-set.selections.len` (plugin reads the list directly) |
| `host-state.get-selection(index)` | Replaced by `selection-set.selections[index]` |
| `host-state.get-all-selections` | Replaced by `selection-set.selections` |
| `display-directive` variant (12 cases) | Replaced by `display` (4 algebra leaves) |

Removals are aggregated in this single bump precisely to avoid
the two-ABI-breaks trap; plugins migrate once.

#### Implementation gating

A WIT 3.0 implementation lands as **one PR** that:

1. Bumps `package kasane:plugin@2.0.0;` → `@3.0.0` in
   `kasane-wasm/wit/plugin.wit`,
   `kasane-plugin-sdk/wit/plugin.wit`, and
   `kasane-plugin-sdk-macros/wit/plugin.wit` (kept in sync).
2. Adds the new types / functions per the shape above.
3. Removes the legacy types / functions per the removals
   table.
4. Updates `kasane-wasm/src/host.rs` host-side bindings to
   serve the new functions from existing native primitives
   (no new logic — only wire bridging).
5. Updates `kasane-plugin-sdk/src/lib.rs` guest-side bindings
   and helpers (`define_plugin!` macro adjustments).
6. Recompiles the ~10 bundled / example WASM plugins
   (`examples/wasm/`) against `@3.0.0`. Any plugin using
   `get-selection*` or `display-directive` migrates to the
   new API; the §Decision examples in this ADR plus the
   `examples/wasm/selection-algebra/` external example
   document the migration pattern.
7. ABI version check at host load time rejects `@2.0.0` WASM
   binaries (matching the ADR-031 Phase 4 closure pattern;
   no parallel-ABI support, no deprecation cycle inside the
   binary).

#### Risks

| Risk | Mitigation |
|---|---|
| External plugin authors block on the migration | CHANGELOG entry + migration cookbook in `docs/plugin-development.md`; the migration is mechanical (rename + restructure, no semantic surprises) |
| `display` variant's `content::editable` payload contains `editable-span` which itself contains a `selection-record` — circular type dependency in WIT | Topological ordering of types in the WIT file (the existing 1728-line file already orders this way for `style` / `atom`) |
| `buffer-edit` record size grows with `original` / `replacement` strings | Acceptable — shadow-cursor commits are inherently bounded in size by the editable virtual-text span; no risk of unbounded `original` |
| WIT 3.0 lands while ADR-032 W5 is in flight | W5 is currently undecided; if W5 lands positive after WIT 3.0, that's a WIT 3.1 or 4.0 follow-up. Not a blocker |

#### Out of scope (deferred to later ADRs)

- ADR-032 W5 path / brush / stroke types — gated on the Vello
  adoption decision (see ADR-032 §SDK-migration analysis).
- Plugin commit-intercept hook (consumes `buffer-edit`,
  produces transformed `buffer-edit`) — additively landable
  on top of WIT 3.0; no further ABI break needed.
- `selection-set::map` / `::filter` / `::flat-map` (require
  host-side closures, deferred indefinitely; plugins use the
  workaround via direct `selections` list access).
- Cross-generation `selection-set-project-to` — deferred to a
  follow-up ADR; the current `set-algebra-error::generation-mismatch`
  variant reserves the wire shape for it.
