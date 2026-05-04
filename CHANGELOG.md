# Changelog

## [Unreleased]

### Added — Composable Lenses MVP (Roadmap §Backlog) (2026-05-04)

A `Lens` is a named, individually-toggleable source of
`DisplayDirective`s, registered on a `LensRegistry` held on
`AppState`. The dispatch path queries the registry alongside
plugin display handlers; enabled lenses contribute their
directives to the same `DirectiveSet` plugin handlers produce,
so the existing display algebra (`Then` / `Merge` via
`bridge::resolve_via_algebra`) handles composition uniformly.

**Why a separate abstraction from plugins**: plugin display
handlers can already emit directives, but enable / disable
granularity is plugin-wide. A plugin bundling 5 visualisations
gets all-or-nothing today. Lenses give per-contribution toggle
without unloading the plugin, addressable by stable
`(plugin_id, name)` identity from a UI / CLI.

**Composition**: enabled lenses' directives push onto the same
`DirectiveSet` and resolve via the existing algebra —
order-independent for non-conflicting leaves; conflicts resolve
by the existing `(priority, plugin_id, ...)` sort key. A lens
can supply a `priority()` to control its placement in the
conflict ordering.

- (core) `kasane_core::lens` — new module:
  - `Lens` trait — `id() -> LensId`, `label() -> String`
    (default = name), `priority() -> i16` (default 0),
    `display(view) -> Vec<DisplayDirective>`.
  - `LensId { plugin: PluginId, name: String }` — namespaced
    identity.
  - `LensRegistry` — `register` / `unregister` / `enable` /
    `disable` / `toggle` / `is_enabled` / `is_registered` /
    `registered_ids` / `enabled_ids` / `len` /
    `enabled_count` / `collect_directives(view)`. Holds
    lenses as `Arc<dyn Lens>` so cloning is cheap; `Default`
    is empty.
- (core) `AppState.lens_registry: LensRegistry` — new field;
  default empty. Survives `clone()` since `LensRegistry` is
  `Clone`.
- (core) `PluginRuntime::view().collect_display_directives`
  now also pulls from `AppView::as_app_state().lens_registry`
  via `collect_tagged_display_directives`. The early-return
  guard relaxes from "any DISPLAY_TRANSFORM plugin" to "any
  DISPLAY_TRANSFORM plugin OR any enabled lens" so a
  lens-only buffer still produces directives.
- (test) 15 new tests in `lens::tests` (lib 2467 → 2482):
  - **Registry mechanics** (8 tests): empty registry collects
    nothing; register inserts but starts disabled;
    enable / disable toggles emission; toggle returns new
    state and flips; enable / toggle of unregistered lens are
    no-ops; unregister also removes from enabled set;
    re-register replaces existing lens preserving enable
    state.
  - **Composition** (2 tests): multiple enabled lenses compose
    in `LensId` sort order; priority lifts onto the emitted
    tuple.
  - **Identity & introspection** (2 tests): `registered_ids`
    are sorted; equality compares enabled-set + registered-id
    set (lens trait objects compared by id).
  - **AppState integration** (2 tests): `AppState::default`
    carries empty registry; mutating `state.lens_registry`
    takes effect on the next `collect_directives` call.
  - **End-to-end pipeline** (1 test): a lens emitting
    `DisplayDirective::Hide { 3..4 }` flows through
    `PluginRuntime::view().collect_display_directives`
    against an empty plugin set; disabling the lens removes
    the directive on the next call.

The MVP is **native-only and runs every frame** (no Salsa
caching layer yet). The future Salsa integration described in
ADR-035 (caching key
`(file_id, line, lens_stack) -> Display`) is the natural
follow-up — the registry's stable `LensId` set + enabled set
is already the right cache key shape. WIT-level surface
(plugins registering lenses from WASM) is also deferred;
native plugins (crates linking `kasane-core` directly) can
register lenses today.

Validation: 2482 workspace lib tests pass; 2739 workspace
integration tests pass; clippy + fmt clean across
`gui,syntax`.

### Added — WIT surface for commit-intercept hook (`shadow-edit` + `intercept-buffer-edit`) (2026-05-04)

WASM plugins can now register a commit-intercept handler — the
native-only capability that landed in commit `6f88744f` is now
fully wire-exposed. Resolves the `buffer-edit` WIT shape
conflict noted in the WIT 3.0 paper-design reconsideration by
naming the new record `shadow-edit` (distinct from the
WIT 2.0 `buffer-edit` used by the `edit-buffer` command effect).

This is an additive change within `kasane:plugin@3.0.0`; the
ABI version stays at 3.0.0, no plugin recompilation is
required for plugins that don't override the new export
(SDK provides a `pass-through` default).

#### WIT contract

- (wit) `interface types`:
  - `record shadow-edit { target: selection-record, original:
    string, replacement: string, base-version: version-id }` —
    the algebraic shape of a shadow-cursor commit, surfaced
    to intercept handlers before serialisation.
  - `variant shadow-edit-verdict { pass-through, replace(shadow-edit), veto }`
    — handler return type. Folded by the dispatcher in slot
    order; `replace` substitutes the running edit, `veto`
    short-circuits.
- (wit) `interface plugin-api`:
  - New export `intercept-buffer-edit: func(edit: shadow-edit)
    -> shadow-edit-verdict` — host calls this on every
    registered plugin after the builtin shadow cursor
    surfaces a pending commit; `pass-through` is the default
    verdict.
  - `use` clause extended to import the two new types.

The legacy `buffer-edit` record (different shape, used by
`edit-buffer` command) coexists unchanged.

#### SDK

- (sdk-macros) `kasane-plugin-sdk-macros::defaults` — new
  `intercept_buffer_edit` default returning
  `ShadowEditVerdict::PassThrough`. Plugins that don't
  override the export inherit this, matching the native-side
  `BufferEditVerdict::PassThrough` default behaviour.

#### Host bindings (`kasane-wasm`)

- (core) `convert::buffer_edit_to_wit` /
  `convert::wit_shadow_edit_to_buffer_edit` /
  `convert::wit_shadow_edit_verdict_to_native` — wire ↔
  native conversion helpers. Direction is preserved across
  the boundary (Backward selections retain their direction
  rather than being re-inferred from anchor / cursor
  ordering).
- (core) `WasmPlugin::intercept_buffer_edit`
  (`PluginBackend` impl) — converts native `BufferEdit` →
  wire `ShadowEdit`, calls
  `api.call_intercept_buffer_edit(...)`, converts wire
  `ShadowEditVerdict` → native `BufferEditVerdict`. Uses
  `call_synced` whose `R::default()` fallback (now
  `BufferEditVerdict::PassThrough` via the new `Default`
  derive) means a plugin call failure degrades safely to
  pass-through.
- (core) `BufferEditVerdict` gains `#[derive(Default)]` with
  `#[default]` on `PassThrough`. Used by host bindings to
  fail safely.

#### WASM blob rebuild

- All 12 example / fixture / guest plugins rebuilt against
  the new ABI surface. The default `intercept-buffer-edit`
  export is auto-supplied by the SDK macro; no plugin source
  changes required for the rebuild.

#### Validation

- 2467 workspace lib tests pass (unchanged from prior
  commit; no new tests added — the existing
  `intercept_tests` already exercise the verdict algebra).
- 2724 workspace integration tests pass.
- 45 SDK lib tests pass.
- clippy + fmt clean across `gui,syntax`.

ADR-035 §"WIT 3.0 Wire Shape (paper design)" updated:
sub-section "4. `buffer-edit` record" renamed to
"4. `shadow-edit` record" with a follow-up note recording
the rename rationale.

The native-only intercept hook from commit `6f88744f` is now
the implementation for both native plugins (which can register
via `HandlerRegistry::on_buffer_edit_intercept` directly) and
WASM plugins (which export `intercept-buffer-edit` per the
WIT). Same dispatch chain, same verdict algebra, same
Hippocratic noop short-circuit.

### Added — Plugin commit-intercept hook for shadow-cursor edits (2026-05-04)

Plugins can now observe / transform / veto a shadow-cursor
buffer edit before it's serialized to Kakoune `exec -draft`
commands. The intercept runs after `mirror_edit` produces a
`BufferEdit` and before `edit_to_commands` serializes it —
exactly the slot ADR-035 ShadowCursor Phase 3 / 4 prepared
the algebraic shape for.

- (core) `state::shadow_cursor::BufferEditVerdict` — new enum
  with three cases:
  - `PassThrough` — observe without changing the edit
    (default; equivalent to a plugin not registering an
    intercept).
  - `Replace(BufferEdit)` — substitute a transformed edit.
  - `Veto` — drop the commit; no Kakoune commands emit
    (the shadow cursor still deactivates).
- (core) `plugin::handler_registry::HandlerRegistry::on_buffer_edit_intercept`
  — registration method. Handler signature:
  `Fn(&S, &BufferEdit, &AppView) -> (S, BufferEditVerdict)`.
- (core) `plugin::handler_table::ErasedBufferEditInterceptHandler`
  + `HandlerTable.buffer_edit_intercept_handler` field.
- (core) `plugin::traits::PluginBackend::intercept_buffer_edit`
  — trait method with `BufferEditVerdict::PassThrough`
  default. Implemented on `PluginBridge` to dispatch through
  the table.
- (core) `plugin::registry::input_dispatch::fold_intercept_chain`
  — pure-function verdict folder. Iterates plugin backends in
  slot order; `PassThrough` keeps the running edit,
  `Replace(new)` substitutes, `Veto` short-circuits to None.
  Public so out-of-tree consumers (and tests) can exercise
  the algebra directly.
- (core) `plugin::traits::KeyPreDispatchResult::Consumed.pending_buffer_edit`
  — new `Option<BufferEdit>` field. Producers
  (`BuiltinShadowCursorPlugin`) populate it instead of
  pre-serialized commands; the dispatch loop folds intercepts
  and serializes the final edit.
- (core) `state::shadow_cursor::BuiltinShadowCursorPlugin::handle_key_pre_dispatch`
  — Commit branch refactored: instead of calling
  `build_mirror_commit` to pre-serialize, calls `mirror_edit`
  to surface the typed BufferEdit on
  `pending_buffer_edit`. The dispatcher
  (`PluginRuntime::dispatch_key_pre_dispatch`) takes
  responsibility for folding intercepts and serializing.
- (core) `plugin::registry::input_dispatch::PluginRuntime::dispatch_key_pre_dispatch`
  — recognizes the new `pending_buffer_edit` field on
  `Consumed`. After the consumer wins, runs
  `fold_intercept_chain` over all plugins, applies the
  Hippocratic noop check on the final edit, and serializes
  via `state::shadow_cursor::edit_to_commands` into
  `commands` before returning.
- (test) 7 new `intercept_tests` covering the verdict
  algebra (lib 2460 → 2467):
  - `empty_chain_returns_initial_edit` — no plugins
    registered: identity.
  - `pass_through_chain_returns_initial_edit` — multiple
    PassThrough plugins compose to identity.
  - `replace_substitutes_running_edit` — Replace lands.
  - `replace_then_replace_chains_in_order` — last
    Replace wins (slot order).
  - `veto_short_circuits_returning_none` — Veto first
    drops the commit.
  - `replace_then_veto_returns_none` — Veto after
    Replace still drops.
  - `veto_does_not_invoke_subsequent_handlers` —
    short-circuit confirmed via a CountingBackend that
    asserts zero invocations after a Veto.
- (test) `bridge::tests::exhaustive_handler_dispatch_coverage`
  updated: `EXPECTED_HANDLER_NAMES` gains
  `"buffer_edit_intercept"`; `AllHandlersPlugin::register`
  registers a passthrough intercept handler; the test body
  invokes `bridge.intercept_buffer_edit(&probe_edit, &app)`
  to exercise the dispatch path.

The hook lands as a **native-only** capability for now — it
does not yet appear in the WIT contract. The buffer-edit
WIT shape conflict noted in the WIT 3.0 paper-design
reconsideration (the existing `buffer-edit` record under WIT
2.0 backs `edit-buffer` and has a different shape) blocks
the WIT-level surface; surfacing the intercept handler to
WASM plugins waits for that resolution. Native plugins
(crates that link directly to `kasane-core`) can register
the handler today.

Validation: 2467 workspace lib tests pass. 2724 workspace
integration tests pass. clippy + fmt clean across
`gui,syntax`.

### Changed — WIT 3.0 paper design reconsidered: display-directive collapse deferred indefinitely (2026-05-04)

After landing the ADR-035 driver portion of WIT 3.0 (commit
`0e75a54a`), I revisited the deferred ADR-034 driver portion
(the `display-directive` → `display` algebra-leaf collapse)
and concluded it should not ship — neither now nor as a WIT
3.x follow-up.

The original paper-design argument was "one wire
representation eliminates the host translator." On
re-examination after seeing the actual code:

- The host translator (`display_algebra::bridge::directive_to_display`)
  is **72 LOC** of straightforward per-variant dispatch over
  13 `derived::*` constructors — not the deep plumbing tail
  the prior commit's deferral note implied.
- Removing it doesn't eliminate equivalent code; it moves it
  to the **guest SDK side** because plugin authors need the
  same ergonomic helpers (`hide(line_range)`, `fold(range,
  summary)`, etc.) — emitting raw `Replace { span, content }`
  leaves directly is verbose and error-prone.
- The "single representation" benefit is **theoretical
  aesthetic**, not load-bearing — both representations carry
  the same information, the bridge is a pure bijection
  modulo documented lossy metadata.
- Migration cost is **real and concrete**: every plugin
  author pays a forced rewrite plus a forced recompile.
- No concrete capability is unlocked — the original ADR text
  mentioned `then` / `merge` "as record-level constructors"
  but the paper design itself walked that back, declaring
  them host-side normalisation operators.

A future ABI break (WIT 4.0+) may revisit if a concrete
capability emerges that the wire-level collapse unlocks
(e.g. plugin-emitted Then / Merge composition, which would
require host changes anyway). Until that capability
surfaces, the collapse is pure churn.

- (docs) `docs/decisions.md` ADR-035 §"WIT 3.0 Wire Shape
  (paper design)" — new sub-section "Drivers reconsidered
  (2026-05-04)" with the analysis; "Drivers" table struck
  through the ADR-034 row; "Decision summary" table struck
  through row 3; sub-section "3. `display` variant" gains a
  DEFERRED banner; sub-section "4. `buffer-edit` record"
  gains a DEFERRED banner (the pre-existing WIT 2.0
  `buffer-edit` shape conflict was missed in the original
  paper design — addressing it requires a separate naming
  pass).
- (docs) ADR-034 §Migration WIT-contract row updated to
  point at the reconsidered decision rather than the frozen
  shape.
- (docs) The previous commit's milestone entry note
  ("intentionally deferred to a follow-up commit") replaced
  with a clear "deferred indefinitely" marker.

This commit changes only documentation. No code, no tests,
no plugins — the WIT 3.0 ABI bump that landed in `0e75a54a`
ships ADR-035 only and is the final shape of WIT 3.0
unless / until a concrete capability surfaces that justifies
revisiting.

### Changed — WIT 3.0 ABI bump (selection-set + time + history) (2026-05-04)

Implements the ADR-035 portion of the WIT 3.0 paper-design
freeze. Bumps `kasane:plugin@2.0.0` → `@3.0.0`.

The display-directive → display variant collapse (the
ADR-034 driver portion of WIT 3.0) is intentionally deferred
to a follow-up commit — the 11-case `display-directive`
variant has a deep host plumbing tail (color-preview +
selection-algebra both emit it) that warrants its own
focused PR. The version is bumped to 3.0.0 in this commit;
the deferred follow-up will land additively under the same
major version.

#### WIT contract (single canonical file, symlinked into 3 crates)

- (wit) `package kasane:plugin@2.0.0` → `@3.0.0` with a
  comment block recording the ADR-035 driver scope.
- (wit) Adds (in `interface types`):
  - `record buffer-pos { line: u32, column: u32 }`
  - `enum selection-direction { forward, backward }`
  - `record selection-record { anchor, cursor, direction }`
  - `record selection-set { selections, buffer, generation }`
  - `variant set-save-error { invalid-name }`
  - `variant set-load-error { not-found, buffer-mismatch }`
  - `type version-id = u64`
  - `variant time { now, at(version-id) }`
- (wit) Adds (in `interface host-state`):
  - `current-selection-set: func() -> selection-set`
  - `selection-set-union` / `-intersect` / `-difference` /
    `-symmetric-difference` (free functions on
    `selection-set`)
  - `selection-set-save` / `-load` (returning
    `result<_, set-save-error>` / `result<selection-set,
    set-load-error>`)
  - `selection-set-to-kakoune-command: func(set) -> option<string>`
- (wit) Adds new `interface history` with
  `current-version`, `earliest-version`, `text-at`,
  `selection-at`; world `kasane-plugin` imports it
  alongside `host-state` / `element-builder` / `host-log`.
- (wit) Removes:
  - `record selection { anchor, cursor, is-primary }`
  - `host-state.get-selection-count` / `get-selection` /
    `get-all-selections`

#### Host bindings (`kasane-wasm`)

- (core) `host::HostState.selection_set:
  state::selection_set::SelectionSet` — replaces
  `selections: Vec<derived::Selection>`. Mirrored from
  `state.inference.selection_set` in the BUFFER_CURSOR sync
  block.
- (core) `host::HostState.history:
  Option<Arc<InMemoryRing>>` — cloned from `state.history`
  in the BUFFER_CONTENT sync block. Backs the new
  `history` interface.
- (core) `host::HostState`'s `host_state::Host` impl gains
  `current_selection_set` + 7 set-algebra wrappers
  (`selection_set_union` / `_intersect` / `_difference` /
  `_symmetric_difference` / `_save` / `_load` /
  `_to_kakoune_command`). Each wrapper converts WIT
  `SelectionSet` → native `SelectionSet`, calls the
  algebraic method, converts the result back. The
  `_to_kakoune_command` wrapper extracts the command
  string from the native return value's keysym-encoded
  `KasaneRequest::Keys` form (decoded back to the bare
  `select <ranges>` string plugins expect).
- (core) `host::HostState`'s `history::Host` impl —
  `current_version`, `earliest_version`, `text_at(time)`,
  `selection_at(time)`. Each delegates to the held
  `Arc<InMemoryRing>` and converts `Time::At(VersionId)`
  via the existing `HistoryBackend::snapshot` interface.
- (core) `selection_set_to_wit` / `selection_set_from_wit`
  free helpers — wire ↔ native conversion. Direction is
  preserved across the boundary (backward selections
  retain `Direction::Backward` rather than being
  re-inferred from anchor / cursor ordering).
- (core) `lib.rs::add_to_linker` — wires the new
  `history` interface into the wasmtime linker.

#### SDK bindings (`kasane-plugin-sdk`)

- (sdk) `test::MockHostState.selection_count` field +
  `set_selection_count` / `get_selection_count` mock
  functions removed. The mock surface is intentionally
  narrowed; plugins that need a SelectionSet mock should
  add a per-plugin mock rather than rely on the SDK's
  built-in.
- (sdk) `lib.rs::tests::harness_set_selection_count` test
  removed (asserted on the deleted mock).

#### Example plugins

- (examples) `examples/wasm/selection-algebra/src/lib.rs`
  — set-operation hot path migrated from
  `host_state::get_selection_count()` + per-index
  `host_state::get_selection(i)` loop to a single
  `host_state::current_selection_set()` call returning a
  `SelectionSet` whose `selections` field iterates
  directly. Coordinate types adjusted from `i32` to `u32`
  to match the new `BufferPos` shape.
- (examples) All 10 plugins under `examples/wasm/`
  recompiled against the new ABI as `wasm32-wasip2`
  Components and copied to
  `kasane-wasm/fixtures/<plugin>.wasm` (and to
  `kasane-wasm/bundled/<plugin>.wasm` for the 6 bundled
  ones). The 2 host-test guest plugins under
  `kasane-wasm/guests/` (`instantiate-trap`,
  `surface-probe`) likewise recompiled.

#### ABI version constants

- (kasane-plugin-package) `manifest::HOST_ABI_VERSION`
  bumped from `"2.0.0"` to `"3.0.0"`.
- (kasane) `plugin_cmd::templates::HOST_ABI_VERSION`
  bumped to `"3.0.0"` (templates emit `abi_version =
  "3.0.0"` for newly-generated plugins).
- (manifests) 25 `kasane-plugin.toml` / `*.toml` files
  across `kasane-wasm/{fixtures,bundled}/` and
  `examples/wasm/*/` bumped to `abi_version = "3.0.0"`.
- (test fixtures) 10 hardcoded `abi_version = "2.0.0"`
  manifest strings inside `kasane/src/{plugin_lock,
  plugin_store, locked_wasm_provider, plugin_cmd/*}.rs`
  + `kasane-plugin-package/src/{manifest,package}.rs`
  test bodies bumped.
- (test assertions) `kasane-wasm/src/tests/discovery.rs`
  asserts `pane_manager.abi_version == "3.0.0"`;
  `kasane-wasm/src/tests/mod.rs` test fixture manifest
  bumped.

#### Build pipeline

- The previously-shipped WASM blobs were built against
  the `wasm32-wasip2` target (which produces Component
  format directly); the rebuild used the same target via
  the Nix-flake-provided toolchain. No `wit-component`
  invocation needed — the wasip2 target produces
  Components natively.

#### Validation

- 2460 workspace lib tests pass.
- 2717 workspace integration tests pass.
- 45 SDK lib tests pass.
- `cargo clippy --workspace --features gui,syntax -- -D
  warnings`: clean.
- `cargo fmt --check`: clean.

ADR-035 §Implementation Status updated with the WIT 3.0
milestone entry.

### Added — WIT 3.0 Wire Shape paper design (2026-05-04)

Freezes the wire-shape decisions for the `kasane:plugin@3.0.0`
ABI bump so the implementation can be one PR. Following the
ADR-031 "Phase 10 Wire Shape" template — the same discipline
that prevented the two-ABI-breaks trap there applies here.

- (docs) `docs/decisions.md` ADR-035 §"WIT 3.0 Wire Shape (paper
  design, 2026-05-04)" — ~250 lines covering:
  - Drivers table (ADR-034 + ADR-035 only; ADR-032 W5
    path/brush/stroke decoupled to keep the bump bounded).
  - Decision summary (5 features, all backed by already-landed
    native primitives — wire promotion, not new design).
  - Concrete WIT shapes for `selection-set` value-record +
    `selection-record` + `buffer-pos` + `selection-direction`
    + 8 free functions for set-algebra operations + `save` /
    `load` / `to-kakoune-command` projections.
  - `time` variant + `version-id` alias + 4 history-accessor
    free functions (`history-current-version`,
    `history-earliest-version`, `history-text-at`,
    `history-selection-at`) in a new `history` interface.
  - `display` variant (4 algebra leaves) replacing the legacy
    12-case `display-directive`. `then` / `merge` are
    *normalisation operators on the host*, not wire
    constructors — plugins emit `list<display>` and the host
    composes via `algebra_normalize`.
  - `buffer-edit` record (target / original / replacement /
    base-version) — the Phase 3 / 4 algebraic shadow-cursor
    commit shape, frozen now so a future plugin commit-intercept
    hook can land additively.
  - `current-selection-set` accessor in `host-state`,
    replacing the legacy heuristic `get-selection*` triplet.
  - Removals table (legacy `selection` record, `get-selection*`
    triplet, 12-case `display-directive` variant).
  - Implementation gating (one PR; touches plugin.wit ×3,
    host.rs, sdk lib.rs, ~10 example WASM plugins; ABI version
    check rejects `@2.0.0` binaries at host load — same single
    -ABI-break strategy as ADR-031 Phase 4 closure).
  - Risks / out-of-scope (plugin-author migration cost
    mitigated by mechanical migration cookbook;
    `selection-set::map` / `::filter` / `::flat-map` deferred
    indefinitely because WIT does not express host-side
    closures ergonomically).
- (docs) ADR-034 §Migration WIT-contract row updated to point
  at the frozen wire shape.
- (docs) ADR-035 §Migration WIT-contract row sharpened with
  the concrete additions / removals.

Resources were considered for `selection-set` but rejected:
the host-tracked-handle ergonomics they enable do not benefit
set algebra (which is naturally value-typed), and they would
introduce the only `resource` in the entire 1728-line WIT,
raising the SDK guest-binding complexity floor unnecessarily.

Every host-side primitive WIT 3.0 surfaces is **already
implemented** as native Rust code (the ADR-035 / ADR-034 /
ADR-037 work that landed 2026-05-03 / 04). WIT 3.0 is the
wire-shape promotion of work that already exists internally —
not a design phase that discovers new requirements.

No code change in this entry. The implementation PR is gated
on user greenlight: the bump touches host bindings, SDK,
~10 example WASM plugins, and the ABI version check.

### Added — ADR-035 SelectionSet → Kakoune projection (2026-05-04)

Closes the ADR-035 §Decision "Projection back to Kakoune" line
which was previously documentation-only. The round-trip the ADR
describes (`current.union(&saved).apply()` → `select` to Kakoune
→ next protocol echo carries the new canonical selection) is now
wired end-to-end.

- (core) `state::selection_set::SelectionSet::to_kakoune_command(&self)
  -> Option<Command>` — encodes the set as a Kakoune `:select`
  command in the `<line>.<col>,<line>.<col>` per-range syntax
  (1-indexed, byte-addressed, anchor-then-cursor). Returns
  `None` for an empty set (Kakoune `:select` requires ≥ 1
  range). Direction is preserved by emitting the anchor
  position first; multi-line selections produce one range whose
  anchor and cursor sit on different lines.
- (test) 5 new tests in `state::selection_set_tests` (lib 1783
  → 1788) covering empty / singleton / multi-selection /
  direction-preservation (Backward selection emits anchor
  before cursor) / multi-line anchor / cursor cases. The
  assertions decode `KasaneRequest::Keys` via a
  `render_kakoune_command` helper to compare against the
  readable command form rather than the keysym vector.

The set's `BufferId` and `BufferVersion` are not consulted —
the caller is responsible for ensuring the set is anchored to
the buffer Kakoune is currently focused on; otherwise the
projection lands positions in the wrong buffer and Kakoune
silently mis-selects.

### Added — ADR-035 ShadowCursor §Migration Phase 4 (2026-05-04)

The active shadow edit now carries the history `VersionId` it was
authored against. Combined with Phase 3's algebraic `BufferEdit`,
this gives a downstream consumer the full algebraic shape of an
edit-in-flight: `target` (where), `original` / `replacement`
(what), and `base_version` (when). The §Migration table row
"ShadowCursor rewritten on Selection + Time primitives" is
realised in spirit (algebraic edit shape + version stamp) but not
in LOC (the keyboard-handler grapheme arithmetic stays — it does
not re-shape onto buffer-space `SelectionSet` algebra).

- (core) `state::shadow_cursor::ShadowPhase::Editing` — gains a
  `base_version: VersionId` field set at the
  `Navigating → Editing` transition (the first printable
  keystroke) and preserved across all in-place keystroke edits.
- (core) `state::shadow_cursor::handle_shadow_cursor_key` —
  takes `current_version: VersionId`, consulted only at the
  activation transition. The production caller in
  `handle_key_pre_dispatch` reads it from
  `app_state.history.current_version()`.
- (core) `state::shadow_cursor::BufferEdit` — gains
  `base_version: VersionId` plus
  `is_stale_against(current) -> bool` returning true when
  `current > base_version`. Lets a downstream consumer gate
  commit, prompt the user, or replay the edit on a newer base
  — and lets a consumer compose the edit with `Time::At(v)`
  queries to materialise the buffer state it was authored
  against.
- (test) 4 new shadow_cursor tests (1779 → 1783 lib): activation
  stamp from `Char` keystroke; in-place edit preserves the
  activation stamp even when a later `current_version` is
  supplied (the buffer advanced underneath the active edit but
  the user kept typing); `mirror_edit` surfaces the stamp on
  `BufferEdit`; `is_stale_against` predicate semantics
  (same / older / newer current).
- (test) 16 existing `ShadowPhase::Editing` constructions
  collapsed onto a new `mk_editing(working, original, cursor)`
  helper that defaults `base_version` to `VersionId::INITIAL`;
  3 `BufferEdit` literals in Phase 3 tests collapsed onto a new
  `mk_buffer_edit(line, start, end, original, replacement)`
  helper that does the same. Net test-code reduction.

ADR-035 §Implementation Status updated with the Phase 4
milestone entry; the in-module migration docstring records that
the ShadowCursor §Migration is now complete to the extent the
keyboard-handler half permits — no further deferred phases
remain in this module.

### Added — ADR-035 ShadowCursor §Migration Phase 3 (2026-05-04)

The shadow cursor commit pipeline gains an algebraic representation:
`BufferEdit { target: Selection, original: String, replacement:
String }`. Production splits into `mirror_edit(shadow, span,
line_count) -> Option<BufferEdit>` (the algebra) and
`edit_to_commands(edit) -> Vec<Command>` (the Kakoune
`exec -draft` serializer). `build_mirror_commit` survives as a
thin compose of the two — dispatch-side callers do not change.

The motivation: `BufferEdit` is the natural payload for a future
plugin commit-intercept hook (a plugin reads / transforms / vetoes
the edit before it serializes to Kakoune), and tests can assert
structural shape against a typed value rather than parsing
keysym-encoded `exec -draft` strings.

- (core) `state::shadow_cursor::BufferEdit` — new public type
  carrying the algebraic shape of a buffer edit (target Selection,
  pre-edit text, post-edit text). `is_hippocratic_noop()` returns
  true when the replacement equals the original.
- (core) `state::shadow_cursor::mirror_edit(shadow, span,
  line_count) -> Option<BufferEdit>` — computes the BufferEdit for
  a Mirror-projection commit. Returns None for Navigating phase
  (nothing to commit), Hippocratic noops (working_text ==
  original_text), out-of-range anchor lines, and PluginDefined
  projections (handled separately by `on_virtual_edit`).
- (core) `state::shadow_cursor::edit_to_commands(edit)
  -> Vec<Command>` — serializes a BufferEdit into Kakoune
  `exec -draft` commands. Empty target ranges produce the insert
  form (`{line}g {col}li{text}<esc>`); non-empty ranges produce
  the substitute form (`{line}g {start}l{end}lsc{text}<esc>`).
- (core) `state::shadow_cursor::build_mirror_commit` — refactored
  to `mirror_edit().as_ref().map(edit_to_commands).unwrap_or_default()`.
  Behavior unchanged for existing callers.
- (test) 9 new `shadow_cursor` tests (1770 → 1779 lib tests):
  five `mirror_edit_*` tests covering the four None branches plus
  the Some-shape happy path; two `edit_to_commands_*` tests
  asserting substitute / insert command forms via a
  `render_kakoune_command` helper that decodes
  `KasaneRequest::Keys` back to a readable string;
  `build_mirror_commit_matches_compose_of_mirror_edit_and_edit_to_commands`
  pins the equivalence; `buffer_edit_hippocratic_noop_detects_equal_strings`
  pins the noop-detection helper.

The keyboard-handling state machine (`handle_shadow_cursor_key`)
intentionally remains in synthetic grapheme space — the original
Phase 3 sketch's "smaller surface" half does not re-shape onto
buffer-space `SelectionSet` algebra (cursor_grapheme_offset
indexes graphemes within working_text, not buffer columns), so
that half is dropped from the migration plan rather than
deferred. Phase 4 (`BufferVersion` stamp on `working_text`)
remains deferred.

ADR-035 §Implementation Status updated with the Phase 3
milestone entry; the in-module migration docstring renumbers
the deferred phases (Phase 4 is the only remainder).

### Changed — ADR-035 ShadowCursor §Migration Phase 2 (2026-05-04)

`EditableSpan`'s per-span `anchor_line: usize` + `buffer_byte_range:
Range<usize>` pair collapses to a single `projection_target:
Selection` field. Phase 1 (2026-05-03) introduced the read-only
Selection-shaped accessor over the legacy fields; Phase 2 retires
the legacy fields entirely and stores Selection as the source of
truth.

- (core) `state::shadow_cursor::EditableSpan` — `anchor_line`
  and `buffer_byte_range` removed; `projection_target: Selection`
  added. Invariant: `anchor.line == cursor.line` (Mirror
  projections target one buffer line); `min().column..max().column`
  recovers the byte range. The Phase 1 `projection_target()`
  method is gone — callers read the field directly.
- (core) `state::shadow_cursor::ShadowCursor::buffer_projection_target`
  — closure body returns `s.projection_target` (was
  `EditableSpan::projection_target` method).
- (core) `state::shadow_cursor::build_mirror_commit` — derives
  `(anchor_line, col_min, col_max)` from `span.projection_target`;
  Hippocratic empty-range check uses `col_min == col_max` rather
  than `Range::is_empty`.
- (test) Six `EditableSpan { ... }` literals collapsed onto a
  `mk_span(line, start_col, end_col)` test helper. Three Phase 1
  tests covering field shape and indexing retained; the
  overflow-clamping test (which exercised the deleted
  `projection_target()` method's usize → u32 narrowing) is gone
  — the new field is `Selection` (already u32-typed), so the
  narrowing concern moves entirely to construction sites.

No external API change: `EditableSpan` is a payload type carried
through `display`, `display_algebra::primitives`, `display::unit`,
and `plugin::safe_directive`, but none of those callsites read or
construct the consolidated fields. The full workspace builds
clean (TUI + GUI + syntax) and all 1770 lib tests + 2011
integration tests pass.

ADR-035 §Implementation Status updated with the Phase 2 milestone
entry; the in-module migration docstring (`shadow_cursor.rs`)
renumbers the deferred phases (formerly Phase 2/3/4 — now
Phase 3/4 with Phase 2 marked landed).

### Added — ADR-035 Time-aware Salsa integration (2026-05-03 follow-up)

Builds on the ADR-035 Selection / Time foundation entry below with
production-grade Salsa wiring and ADR-034 cross-integration. After
this work ADR-035 is at 10/11 milestones — only the ShadowCursor
re-host and WIT 3.0 coordinated bump remain.

- (core) `salsa_inputs::HistoryInput` — new `#[salsa::input]` with
  `backend: Arc<InMemoryRing>` and `current_version: VersionId`
  fields. Concrete-type backend (rather than trait-object Arc)
  because Salsa input fields require Update-deriving types; a
  wrapper enum will support pluggable backends later.
- (core) `salsa_queries::text_at_time(db, BufferInput, HistoryInput,
  Time) -> Option<Arc<str>>` — Time::Now projects `BufferInput.lines`
  to plain text; Time::At(v) reads from history. Cache key
  `(BufferInput, HistoryInput, Time)`.
- (core) `salsa_queries::selection_at_time(db, HistoryInput, Time)
  -> Option<SelectionSet>` — Time::Now resolves through
  `history.current_version(db)`; Time::At(v) reads the snapshot's
  selection. Demonstrates the Time-aware pattern generalises beyond
  text. Cache key `(HistoryInput, Time)`.
- (core) `salsa_queries::display_directives_at_time(db, HistoryInput,
  Time) -> NormalizedDisplay` — ADR-034 + ADR-035 integration
  query. Synthesises Decorate primitives from the SelectionSet at
  the requested Time and runs `algebra_normalize`. Demonstrates the
  Time-aware Salsa pattern feeding the Display algebra in a single
  query.
- (core) `salsa_sync::SalsaInputHandles::history` field +
  `sync_inputs_from_state` updates the field every frame: the
  backend Arc is swapped to `Arc::clone(&state.history)` (so all
  three queries see the same ring AppState writes to) and the
  current_version is pushed from `state.history.current_version()`
  (so Time::Now-resolving queries invalidate when apply's
  auto-commit hook bumps the version).
- (core) `state::inference_state::InferenceState.selection_set:
  SelectionSet` — canonical SelectionSet field projected from the
  heuristic detector's output by `AppState::apply()`. Stored
  alongside the simultaneous history commit so
  `current_selection_set()` and `selection_at_time(Time::Now)`
  observably agree. SelectionSet gained `Default` /
  `default_empty()` to support the field's default value.
- (core) `plugin::app_view::AppView::current_selection_set()` —
  direct field-read accessor for the canonical SelectionSet,
  avoiding the history-lookup overhead for the common
  "what's selected right now" query.
- (core) `display_algebra::normalize::pass_c_filter_evt` fast-path
  early-returns when no EVT leaves are present, skipping the
  invisible-line scan / partition / sort / dedup. EVT is rare in
  typical workloads; the fast-path improves
  `bridge_overhead/mixed_full` from 7.72 µs to 7.04 µs (Phase 2 →
  Phase 3b regression reduced from +28 % to +17 %).
- (test) 15 new Salsa Time-aware unit tests in
  `salsa_queries::time_query_tests` covering all three queries
  across Time::Now / Time::At, empty/populated history, FIFO
  eviction, current_version-advance invalidation, integration
  query selection-to-decorate synthesis. 4 new production-path
  integration tests in `tests/salsa_history_sync.rs` covering Arc
  identity preservation, post-apply / post-sync queries, version-
  advance invalidation through the production sync, integration
  query against synced state. 3 new history_selection tests
  covering canonical SelectionSet field consistency.
- (example) `examples/selection-algebra-native` extended with a
  Time-aware history section: InMemoryRing capacity-3 commits 4
  snapshots, demonstrates FIFO eviction, Time::Now resolution,
  earliest..current walk. Workspace-external dogfood now exercises
  both ADR-035 §1 (set algebra) and §2 (Time / HistoryBackend).

ADR-035 §Implementation Status updated; ADR-037 §Acceptance criteria
#6 amended with the fast-path bench numbers.

### Added — ADR-037 Fold-in-Algebra Accepted (2026-05-03)

The hybrid bridge introduced by ADR-034 is retired. Every directive
— `Hide`, `Fold`, `EditableVirtualText`, and the other 9 variants —
now flows through a single `algebra_normalize` + `pass_c_filter_evt`
path. Legacy `display::resolve` is `#[deprecated]` and slated for
deletion in the next release. Status: **Accepted**
(`docs/decisions.md` ADR-037 §Acceptance criteria).

- (core) `kasane-core/src/display_algebra/primitives.rs`
  - `Content::Fold { range, summary }` (Phase 1) — multi-line fold as
    a `Replace` payload anchored at `range.start`. Replaces
    `derived::fold`'s old multi-line decomposition (1 summary `Replace`
    + N-1 `Empty` `Replace`s) with a single leaf.
  - `Content::Hide { range }` (§6 — added 2026-05-03 to mitigate the
    Phase 3a perf regression) — multi-line hide as a single leaf,
    with `Hide`-`Hide` overlaps treated as commutative (set-union
    idempotent, matching legacy `hidden_set` semantics).
- (core) `kasane-core/src/display_algebra/normalize.rs`
  - **Pass B** (Phase 2) — `replace_conflicts(a, b)` extends Pass A's
    Span overlap with Fold/Hide range coverage cross-check via
    `content_range()`. `Hide`-`Hide` overlap is explicitly
    non-conflicting (commutative).
  - **Pass C** (Phase 3b) — `pass_c_filter_evt(normalized, line_count)`
    filters EditableVirtualText leaves: drops out-of-bounds anchors,
    anchors on Hide/Fold-covered lines, and same-anchor duplicates
    (legacy-compat dedup: ascending-priority retain-first ⇒ lowest
    priority survives). Mirrors legacy `display::resolve` Rules 8-10.
- (core) `kasane-core/src/display_algebra/bridge.rs`
  - `resolve_via_algebra` is now a thin wrapper:
    `forward translate → algebra_normalize → pass_c_filter_evt →
    reverse translate → coalesce_legacy_directives`. No call to
    `display::resolve` from production paths.
  - `coalesce_legacy_directives` reactivated to re-condense per-line
    decompositions back into multi-line legacy enum shapes for
    `DisplayMap::build` consumers. Fold-vs-hide adjacency rule
    tightened to **strict overlap** (touching at a half-open
    boundary does not absorb).
- (core) `kasane-core/src/display/resolve.rs` — `pub fn resolve` and
  `pub fn resolve_incremental` carry `#[deprecated(since = "0.5.0",
  note = "...")]` pointing at `bridge::resolve_via_algebra`. The
  deprecation notes spell out the conflict-semantic differences
  (fold-vs-hide partial overlap now resolves by L6 priority instead
  of conservative fold-drop). All in-tree callers — tests in
  `display/resolve/tests.rs`, `display/tests.rs`, `display/unit.rs`
  test mod, the `bridge_overhead` bench, and `bridge/proptests.rs` /
  `bridge/tests.rs` — opt out via `#![allow(deprecated)]`
  (intentional comparison workloads).
- (test) 7 Pass B unit tests + 7 Pass C unit tests in
  `display_algebra/tests.rs`. Proptest L1–L6 strategy extended with
  `arb_fold` (weight 2 in `arb_leaf`); all six laws still hold under
  the extended distribution. 22 bridge tests updated for Phase 3a/3b
  semantics.
- (bench) `bridge_overhead` bench re-run across the four phases.
  `mixed_full` (realistic workload) progression:

  | Phase | bridge time | Δ vs Phase 2 |
  |---|---|---|
  | Phase 2 (hybrid baseline) | 6.02 µs | — |
  | Phase 3a (no opt) | 8.32 µs | +38 % |
  | Phase 3a + `Content::Hide` | 7.21 µs | +20 % |
  | **Phase 3b (Pass C)** | **7.72 µs** | **+28 %** |

  ADR-037 acceptance criterion #6 (`< +10 %`) is honestly marked as
  not satisfied. ADR-024 SLO (200 µs) impact is +3.9 % and the
  240 Hz scanout impact is +0.18 % — well within the production
  perceptual-imperceptibility budget. The criterion gap is
  documented as a follow-up optimisation surface (Pass C fast-path
  for EVT-empty inputs is the largest remaining lever).
- (docs) `docs/decisions.md` ADR-037 (~330 lines after Phase 4
  amendments, ~390 after Phase 5) — design (Content::Fold +
  Content::Hide), conflict semantics tables, migration plan,
  five-phase acceptance evidence with bench numbers, deprecation
  rationale, deletion inventory.

### Removed — ADR-037 Phase 5 legacy resolve deletion (2026-05-03)

The deprecation cycle was collapsed and the legacy resolver was
deleted the same day Phases 1–4 landed. The "next release"
schedule given in the Phase 4 deprecation note was overtaken by
the in-tree migration audit surfacing zero remaining production
callers. **Net cleanup: −1,900 LOC** (vs ADR-037 §Implications
prediction of −1,200; surplus from also dropping
`bridge/proptests.rs`, which was wholly legacy-comparison
material).

- (core) `kasane-core/src/display/resolve.rs` — shrunk from 798 to
  129 LOC. Removed: `pub fn resolve()`, `pub fn resolve_incremental()`,
  `pub fn check_editable_inline_box_overlap()`,
  `pub fn partition_directives()`, `pub fn resolve_inline()`,
  `pub struct DirectiveGroup`, `pub struct ResolveCache`. Retained
  the input boundary types (`TaggedDirective`, `DirectiveSet`) and
  the production-routing helpers
  (`CategorizedDirectives`, `partition_by_category`) consumed by
  `plugin/registry/mod.rs`.
- (test) `kasane-core/src/display/resolve/tests.rs` deleted (645 LOC)
  along with the now-empty `display/resolve/` directory.
- (test) `kasane-core/src/display_algebra/bridge/proptests.rs`
  deleted (~470 LOC) — the file's purpose was legacy-vs-algebra
  equivalence proptesting, which is moot now that legacy is gone.
  Algebra correctness remains pinned by `display_algebra/proptests.rs`
  (L1–L6 with `arb_fold` weight 2) and the rewritten
  `display_algebra/bridge/tests.rs` (algebra-only round-trip
  scenarios).
- (test) `kasane-core/src/display_algebra/bridge/tests.rs` rewritten
  as algebra-only round-trip tests (12 tests covering single-
  directive variants, multi-directive scenarios, and Pass C
  invariants exercised end-to-end through the bridge).
- (test) `kasane-core/src/display/tests.rs` and
  `kasane-core/src/display/unit.rs` (test mod) — `resolve::resolve`
  callsites rewired to `display_algebra::bridge::resolve_via_algebra`;
  file-level `#![allow(deprecated)]` shims removed.
- (bench) `kasane-core/benches/bridge_overhead.rs` — legacy
  benchmark group removed; only the bridge-side timings remain.
  Historical comparison numbers preserved in `docs/decisions.md`
  ADR-037 §Acceptance criteria #6.
- (core) `kasane-core/src/display/mod.rs` — `pub use` purged of
  deleted names; the `#[allow(deprecated)]` shim is gone.
- Workspace test count: **2452 → 2440** (only legacy / comparison
  tests removed; no functional coverage loss). All remaining
  tests green.

### Added — ADR-034 Display Algebra Accepted (2026-05-03)

`DisplayDirective` (the 12-variant enum in `kasane-core/src/display/`)
now has a parallel algebraic representation: `Display` with five
primitives (`Identity`, `Replace`, `Decorate`, `Anchor`) plus `Then` /
`Merge` composition operators. Plugin-emitted directives travel through
the production Salsa pipeline via a hybrid bridge that legacy-forwards
`Hide` / `Fold` / `EditableVirtualText` and routes the remaining nine
variants through `display_algebra::normalize`. Status: **Accepted**
(decisions.md ADR-034 §Acceptance Evidence).

- (core) `kasane-core/src/display_algebra/`
  - `primitives.rs` — `Display`, `Span`, `Content`, `AnchorPosition`,
    `Side`, `Style`, `EditSpec`, `SegmentRef`, `support()`.
  - `derived.rs` — smart constructors recovering the legacy 12 variants
    (`hide_lines`, `fold`, `insert_after`, `gutter`, `style_inline`,
    `editable_virtual_text`, etc.).
  - `normalize.rs` — `TaggedDisplay`, `MergeConflict`,
    `NormalizedDisplay`, `normalize()`, `disjoint()`. Conflict
    resolution is total-order deterministic via
    `(priority, plugin_id, seq, position_key)`.
  - `apply.rs` — `LineRender`, `BufferLine::Real(usize)` /
    `Virtual { host_line, side, order }`, `Replacement`,
    `Decoration`, `Anchor`. `apply(&NormalizedDisplay, &[usize])`
    projects normalised leaves into a per-line render plan.
  - `bridge.rs` — `directive_to_display`, `display_to_directive`,
    `tagged_directive_to_tagged_display`, `resolve_via_algebra`. The
    last is the public drop-in companion to legacy `display::resolve`.
- (core) `kasane-core/src/plugin/registry/collection.rs:852, 893` —
  both production callsites switched from `display::resolve` to
  `bridge::resolve_via_algebra`.
- (test) 23 algebra unit tests, 7 proptest fixtures (L1–L6 over
  randomised `Display` trees), 22 bridge tests (17 hand-built + 4
  proptest equivalence properties + 1 hybrid-invariant case). All
  green.
- (bench) `kasane-core/benches/bridge_overhead.rs` — criterion bench
  comparing legacy `resolve()` against `resolve_via_algebra` across
  five workloads (`hide_only`, `fold_only`, `mixed_legacy`,
  `mixed_pass_through`, `mixed_full`). Post-zero-clone-optimisation
  results (median):

  | Workload | Legacy | Bridge |
  |---|---|---|
  | `hide_only` (24 plugins × Hide) | 635 ns | 631 ns |
  | `fold_only` (8 plugins × Fold) | 684 ns | 653 ns |
  | `mixed_legacy` (Hide+Fold+EVT) | 340 ns | 371 ns |
  | `mixed_full` (realistic) | 209 ns | 6.02 µs |
  | `mixed_pass_through` (extreme) | 68 ns | 9.46 µs |

  `mixed_full` adds 5.81 µs over legacy — within ADR-024 perceptual
  imperceptibility budget (+10.2 % vs `frame_warm_24_lines = 56.7 µs`,
  +2.9 % vs the 200 µs SLO; 240 Hz scanout impact < 0.25 %). The
  zero-clone optimisation (passing the full `DirectiveSet` to legacy
  `resolve()`, which already filters by variant) cut legacy-heavy
  workloads by 48–66 %.
- (docs) `docs/decisions.md` ADR-034 (255 lines) — primitive design,
  L1–L6 algebraic laws, derived constructor mapping, hybrid bridge
  rationale, performance table, follow-up notes (ShadowCursor on
  algebra, eventual Fold-in-algebra ADR, partition zero-clone analysis).

### Added — ADR-035 Selection / Time foundation (2026-05-03)

`SelectionSet` is now a first-class algebraic type, and `Time` is a
new query coordinate that lets buffer state be read at any past
version. The full end-to-end loop is wired — Kakoune protocol echoes
land in a history backend, plugins read past states via
`AppView::text_at(Time)` / `AppView::selection_at(Time)`. Status:
**Proposed** (6/11 milestones complete); see
`docs/decisions.md` ADR-035 §Implementation Status.

- (core) `kasane-core/src/state/selection.rs` — `Selection` (anchor /
  cursor / direction), `Direction { Forward, Backward }`, `BufferPos`
  (line: u32, column: u32), `BufferId`, `BufferVersion`.
- (core) `kasane-core/src/state/selection_set.rs` — `SelectionSet`
  with the full set algebra (union / intersect / difference /
  symmetric_difference), pointwise transformation (map / filter /
  flat_map), pattern operations (extend_to_pattern stub for the
  follow-up SyntaxProvider integration), and per-(plugin, name) save /
  load store. Half-open `[min, max)` ranges; adjacent selections
  coalesce in `from_iter` (point selections — `anchor == cursor` —
  are not first-class set members; documented in the type's rustdoc).
- (core) `kasane-core/src/history/`
  - `mod.rs` — `Time { Now, At(VersionId) }`, `VersionId`, `Snapshot`
    (text + selection + version / buffer metadata), `HistoryBackend`
    trait, `HistoryError { Evicted, Unknown }`.
  - `in_memory.rs` — `InMemoryRing` default backend with FIFO
    eviction at `DEFAULT_CAPACITY = 256`.
- (core) `AppState`
  - `pub history: Arc<InMemoryRing>` field added (default: fresh ring).
  - `commit_snapshot(buffer, version, text, selection) -> VersionId`.
  - `text_at(Time) -> Option<Arc<str>>`,
    `selection_at(Time) -> Option<SelectionSet>`.
- (core) `AppState::apply()` — auto-commit hook: when a protocol
  message sets `DirtyFlags::BUFFER_CONTENT`, projects `observed.lines`
  to plain text via `lines_to_text` and `inference.selections`
  (heuristic detector) via `selections_to_set`, then calls
  `commit_snapshot`. Lossy by design (drops style payloads).
- (core) `AppView` — `text_at(Time)`, `selection_at(Time)`,
  `history() -> &dyn HistoryBackend` accessors. Plugin-facing entry
  points for time-travel queries.
- (test) Five integration test files:
  - `history_roundtrip.rs` — 9 tests (commit, text_at, FIFO
    eviction, Arc-shared history, bounded Debug).
  - `history_apply_hook.rs` — 5 tests (Draw round-trip, multi-version
    monotonicity, empty buffer, `\n`-joined multi-line, DrawStatus
    does-not-commit).
  - `history_app_view.rs` — 5 tests (current text, past version,
    history metadata, version-range iteration, empty None).
  - `history_selection.rs` — 7 tests (round-trip via AppState +
    AppView, Time::Now is latest, paired text+selection, empty None,
    apply auto-commit empty for default-style atoms, projection
    populates set for styled atoms).
- (test) `kasane-core/src/display_algebra/proptests.rs`,
  `state/selection_set_proptests.rs` — proptest fixtures (idempotency,
  commutativity, associativity, identity, absorption, distributive,
  difference characterisation, symmetric difference, disjointness ↔
  intersect-empty), 64 cases per property.
- (example) `examples/selection-algebra-native/` — runnable binary
  demonstrating `SelectionSet` from a workspace-external crate;
  exercises every operation and witnesses 7 algebraic laws at runtime.
- (docs) `docs/decisions.md` ADR-035 (~290 lines) — Selection /
  SelectionSet / Time / HistoryBackend type design, Salsa
  integration plan, pluggable backend strategy (InMemoryRing /
  GitBacked / RocksDb), risk register, §Implementation Status
  tracking 6/11 completed milestones with dates.

### Total test impact (2026-05-03)

- `cargo test --workspace --lib`: **2463 tests, 0 failed** (was 2350
  pre-ADR-034 baseline at 2026-05-01).
- `kasane-core` lib + integration tests: 1815 (was 1763 baseline).
- New test code: ~2,510 LOC.
- New implementation code: ~2,530 LOC.

### Added — ADR-032 W2 bootstrap (2026-05-01)

- (gui) `kasane_gui::gpu::scene_renderer::FrameTarget` enum (`Surface` /
  `View` variants) abstracts where a frame is rendered. Production paths
  use `Surface(&gpu.surface)` for the swap chain; headless tests use
  `View { view, width, height, format }` to render to an offscreen
  texture. Internally driven by `FrameTarget::acquire(&GpuState) ->
  AcquiredFrame`, which encapsulates the surface state machine
  (Outdated, Lost, Suboptimal, Timeout, Occluded, Validation).
- (gui) `SceneRenderer::render_to_target(gpu, target, commands,
  resolver, cursor)` — public entry point that takes a `FrameTarget`
  directly, used by the golden harness. The existing
  `render_with_cursor` and `render` methods are unchanged behaviourally;
  they now build a `FrameTarget::Surface` internally.
- (gui) `tests/golden_render.rs` drives `SceneRenderer` through
  `FrameTarget::View` against an offscreen Rgba8UnormSrgb texture.
  First fixture: `monochrome_grid` (full-frame `FillRect`). Sandbox
  environments without GPU access skip gracefully.
- (docs) ADR-032 augmented with §Non-Spike Decision Factors covering
  plugin wire protocol impact, backend semantic divergence, Salsa
  compatibility, color management opportunity, self-optimisation
  alternative, Linebender engagement operating cost, and the hybrid-vs-
  -compute strategic position. Spike Measurement Matrix gains a
  per-frame CPU heap allocation row (baseline 583 allocs / 89.7 KB at
  80×24, see `docs/performance.md`).
- (bench) `cargo bench --bench rendering_pipeline --features
  bench-alloc` now reports per-scene-encode allocation counts for
  `scene_full_frame` (80×24 / 200×60), `scene_one_line_changed`, and
  `scene_menu_visible` scenarios.

### Performance — Self-optimisation step #1 (2026-05-01)

- (core) `render::scene::ResolvedAtom.contents`: `String` → `CompactString`.
  The previous `atom.contents.to_string()` in `resolve_atoms` forced a
  heap allocation per atom regardless of size; `CompactString` stores
  ≤24-byte contents inline in the struct, eliminating the alloc for
  short atoms (the common case for code lines). Effect on per-frame
  Scene-encode allocations:
  - 80×24 typical_state: 583 → 163 allocs (**−72 %**)
  - 80×24 one_line_changed: 571 → 163 allocs (−71 %)
  - 200×60 typical_state: 1339 → 271 allocs (**−80 %**)

  Bytes-allocated barely changed (89.7 → 87.5 KB at 80×24) because the
  string content is the same — the savings materialise as fewer
  allocator calls, not fewer total bytes touched. ADR-032 §Spike
  Measurement Matrix's "Per-frame CPU heap allocations" target updated
  to ≤ 245 (1.5× of new 163 baseline). This is the first concrete
  validation of ADR-032 §Non-Spike Decision Factors §Self-optimisation
  alternative.

- (core, gui) Same-pattern follow-up: `DrawCommand::DrawText.text`
  and `DrawCommand::DrawPaddingRow.ch` switched from `String` to
  `CompactString`. Construction sites in `walk_scene.rs`, `ime.rs`,
  `diagnostics_overlay.rs` updated; consumers (`as_str()`, `==
  &str`, `Deref` to `&str`) are unaffected. The 4-scenario alloc
  bench shows no delta because `typical_state(23)` does not exercise
  status-bar / padding-row paths — these primitives matter for real
  UI workloads with IME, diagnostic overlays, and padding rows
  (`~`-filled empty buffer rows). The change keeps the
  `String` → `CompactString` convention consistent across all
  text-bearing `DrawCommand` variants.

### Added — ADR-032 W2/W3 expansion + Phase Z plan + BufferParagraph builder (2026-05-02)

Continues the ADR-032 evaluation framework. **No production renderer
change** — extensions land in places that pay off whether or not
ADR-032 closes positive (W2 fixture coverage, GpuBackend trait wiring,
plugin-author-facing builder). The work is sequenced so that
sandbox-internal preparation for a positive Vello adoption decision is
exhausted in this round.

- **docs (ADR-032 textual amendments)**: §Decision item 3 expanded
  with the visible-behaviour table for `DegradationPolicy::Reject`
  / `Skip` / `FallbackToTui` (so the enum is not dead-code semantics).
  §Spike Measurement Matrix gains 4 rows: incremental warm frame
  (Salsa-hit case), hybrid CPU strip share (durable / transitional /
  stepping-stone classification), actual LOC retired, adapter LOC
  introduced. §Decision Gates gains W3-closing degradation_policy spec
  row and pre-W5 baseline-freeze row. §Non-Spike Decision Factors
  expanded from 7 to 9 sub-sections (parallel-paint future closure,
  Linebender alignment metric). §Rejected Alternatives expanded from
  5 to 9 (Forma, custom compute strip, Glifo-only Mode A1, Glifo-only
  Mode A2 — the last with explicit re-open trigger). §Spike Findings
  replaced with a 12-required-fields template + verdict-routing rule
  (mechanical determination of `Accepted with adoption plan` /
  `Accepted as deferred` / `Rejected`). §Implications gains the
  dual-stack rule (`WgpuBackend` not deleted until Vello 1.0). New
  §Adoption Phase Plan (Z0 ABI break prep / Z1 Text path Mode A2 /
  Z2 Quad-Image / Z3 `WgpuBackend` retirement / Z4 ecosystem,
  conditional on positive spike) — Z3 is the one-way door, gated on
  Vello 1.0 + 3-month soak + Linebender alignment metric green. See
  `docs/decisions.md` ADR-032.
- **docs (roadmap baseline freeze)**: ADR-031 post-closure perf
  opportunities item (3) sub-line shape cache reopen triggers
  (a/b/c) suspended for the duration of the W5 measurement window
  per `docs/roadmap.md`. Suspension expires automatically when ADR-032
  §Spike Findings is finalised; cross-referenced from ADR-032
  §Decision Gates "Pre-W5" row.
- **core (plugin diagnostics)**: new
  `PluginDiagnosticKind::BackendCapabilityRejected { primitive_kind:
  &'static str, backend: &'static str }` variant. Constructor
  `PluginDiagnostic::backend_capability_rejected(plugin_id,
  primitive_kind, backend)` keeps the per-frame emission path
  allocation-free (static-string-shaped fields). Severity: Warning
  (capability rejection is non-fatal — the contribution is dropped,
  the frame proceeds, the plugin remains active). Scoring + tag-kind
  + summarize + tracing report all extended for the new variant. Five
  unit tests pin the ADR-032 §Decision item 3 contract: Warning
  severity, summary text shape, static-string carriage, Runtime
  overlay tag, default `DegradationPolicy::Reject`.
- **gui (BackendCapabilities)**: new `degradation_policy:
  DegradationPolicy` field on `BackendCapabilities`. New
  `DegradationPolicy { Reject, Skip, FallbackToTui }` enum with
  `Default = Reject`. Both backends (`SceneRenderer` →
  `WgpuBackend`-equivalent, `kasane_vello_spike::VelloBackend`)
  advertise `Reject` as their default. The per-frame check is not
  yet wired at any production site — currently no `DrawCommand`
  variant exceeds `WgpuBackend`'s capability set, so the rejection
  path is unreachable in production. Phase Z0 of the Adoption Phase
  Plan wires the check when `DrawCommand::DrawPath` lands.
- **gui (tests, W2 Phase 10 fixture skeletons)**:
  `kasane-gui/tests/golden_render.rs` gains 8 fixtures pinning
  ADR-031 Phase 10 features. 6 buildable today (snapshot bootstrap on
  GPU-capable env via `KASANE_GOLDEN_UPDATE=1`):
  `subpixel_quantisation_4step`, `curly_underline`,
  `color_emoji_priority`, `inline_box_text_flow`, `rtl_bidi_cursor`,
  `cjk_cluster_double_width`. 2 deferred behind documented blockers
  (`#[ignore]` with reason): `variable_font_axes` (waits on
  `Style.font_weight` public surface, ADR-031 Phase 10 Step C),
  `font_fallback_chain` (waits on `render_scene_to_image` `FontConfig`
  override). Each fixture follows the `monochrome_grid` template:
  graceful-skip on no-GPU sandbox, deterministic input, DSSIM ≤ 0.005
  threshold against committed snapshot.
- **core (BufferParagraph builder)**: new public
  `BufferParagraphBuilder` API (`BufferParagraph::builder().atom(...)
  .primary_cursor_at(...).inline_box_slot(...).build()`). The builder
  is the test-and-plugin-author-friendly alternative to going through
  the full Element pipeline; it keeps `inline_box_slots` and
  `inline_box_paint_commands` in lock-step by construction (the
  `len() == len()` invariant cannot be violated through the builder).
  4 unit tests pin the contract (minimal, cursor annotations,
  inline-box pairing, `base_face` default). Used by 3 of the W2
  Phase 10 fixtures above.
- **kasane-vello-spike (paper-design + skeleton)**: module docstring
  gains a 13-row §Translation Contract table (DrawCommand → vello
  Scene mapping per cost class: rect-coarse-only, stroke-coarse, text
  fast path, image, clip-stack/layer-stack, composed, uncertain
  (DrawShadow blur), undefined (DrawCanvas)). `DrawCanvas` deliberately
  resolved as `BackendError::Unsupported` for the spike per
  §DrawCanvas — pre-spike resolution required (option 1: reject via
  BackendCapabilities). `render_with_cursor` body filled with a
  match-arm-exhaustive walk over all 13 `DrawCommand` variants — each
  arm currently raises `BackendError::Unsupported(<variant_name>
  (<Day target>: <vello-side mechanism>, pending))`. New variants
  added to `DrawCommand` produce a compile error in the spike,
  forcing the §Translation Contract to extend before the variant
  ships.
- **kasane-vello-spike (paired bench harness)**:
  `benches/spike_bench.rs` lifted from a 26-LOC stub to a criterion
  harness with deterministic fixture builders (`fixture_warm_80x24`,
  `fixture_warm_200x60`, `fixture_warm_80x24_one_line_changed`).
  Two GPU-free bench groups: `fixture_build` (input-construction
  cost) and `translation_walk` (translator dispatch cost — sets a
  CPU-only floor that real Vello translation must clear). GPU-side
  benches stubbed as commented-out placeholders for W5 Day 1+
  fill-in. Runs meaningfully without `with-vello` feature; criterion
  delta against a future feature-on run characterises Glifo + Vello
  cost.
- **core (workspace test deps)**: `kasane-gui` and
  `kasane-vello-spike` gain `compact_str` as a dev-dependency for
  test-side `ResolvedAtom.contents` construction.

### Changed — ADR-032 W2 prerequisites — **BREAKING (kasane-gui only)**

- (gui) `GpuState::surface` is now `Option<wgpu::Surface<'static>>`.
  Production callers always carry `Some`; headless paths use `None`.
  External consumers of `GpuState` need `as_ref().expect(...)` at the
  three sites that touch the surface directly (`app/render.rs`,
  `gpu/mod.rs::resize`, internal scene_renderer callers updated).
- (gui) `SceneRenderer::new` no longer takes an `EventLoopProxy`. The
  proxy is set separately via `set_event_proxy` (`pub(crate)`) so that
  integration tests can construct a renderer without observing the
  internal `GuiEvent` type. Production code in `app/mod.rs` calls
  `SceneRenderer::new(...)` then `sr.set_event_proxy(self.event_proxy
  .clone())`.
- (gui) `TextureCache::get_or_load` now takes
  `Option<&EventLoopProxy<GuiEvent>>`. When `None` (headless mode), an
  attempted file load logs a warning and returns `LoadState::Failed`
  rather than dispatching a thread.

### Changed — ADR-031 closure cascade (PR-5a..PR-7) — **BREAKING**

ADR-031 closes 2026-04-30. The closure cascade on
`feat/parley-color-emoji-test` retires the public Face↔Style bridges
in `kasane-core`, bumps the WIT plugin contract to **2.0.0** with
Style-native function names, and rebuilds all bundled / fixture
WASM. Plugin authors writing against host APIs see a one-shot ABI
break that covers the remaining face misnomers; the Kakoune wire
format is unchanged.

**WIT 2.0.0 — function renames** (signatures unchanged, names only):

| 1.1.0                       | 2.0.0                        |
|-----------------------------|------------------------------|
| `get-default-face`          | `get-default-style`          |
| `get-padding-face`          | `get-padding-style`          |
| `get-status-default-face`   | `get-status-default-style`   |
| `get-menu-face`             | `get-menu-style`             |
| `get-menu-selected-face`    | `get-menu-selected-style`    |
| `get-theme-face`            | `get-theme-style`            |
| `get-menu-style` (→ string) | `get-menu-mode` (→ string)   |

The last rename frees `get-menu-style` for the actual menu-item
style brush; the string is now `get-menu-mode` (`"inline"` /
`"search"` / etc.) which more accurately describes Kakoune's menu
metadata. `HOST_ABI_VERSION` and the 23 `abi_version = "1.1.0"`
literal sites in fixtures / manifests / resolver tests bumped to
`2.0.0`.

**Public Face↔Style bridges retired:**

- `Cell::face()` and the `terminal_style_to_face` helper deleted.
  Production consumers read `cell.style: TerminalStyle` fields
  (`fg` / `bg` / `reverse` / …) directly.
- `Atom::face()` deleted. Wire-format-aware callers
  (`detect_cursors`, selection segmentation, `inline_decoration`'s
  `atom_face` plumbing) move to `atom.unresolved_style().to_face()` —
  the explicit form keeps the wire-format intent visible.
- `kasane-tui::sgr::emit_sgr_diff(Face)` legacy shim and
  `convert_attribute(Attributes)` test helper deleted; the
  `TerminalStyle`-direct `emit_sgr_diff_style` has been the
  production path since PR-5b.
- `Atom::from_face` renamed to `Atom::from_wire`. The wire-format
  intent is now in the constructor name.
- `Style::from_face` / `Style::to_face`, the `From<Face> for Style`
  / `From<&Face> for Style` / `From<Face> for ElementStyle` impls,
  and `TerminalStyle::from_face` are marked `#[doc(hidden)]` —
  invisible from rendered API docs but callable for the Kakoune
  wire-format conversion path that the JSON-RPC parser, the new
  `Atom::from_wire` constructor, and the wire `test_support`
  helpers depend on. `Style::to_face_with_attrs` downgraded from
  `pub fn` to `pub(super)`.
- `Face` / `Color` / `Attributes` are `#[doc(hidden)]`.

**Style-native rendering pipeline:**

- `Truth::default_face` / `padding_face` / `status_default_face` →
  `*_style`, returning `&'a Style`. `AppView`'s parallel
  Face-bridge accessors deleted.
- Added `Brush::linear_blend(a, b, ratio, fallback_a, fallback_b)`.
  `make_secondary_cursor_face` rewritten as Brush-native
  `make_secondary_cursor_style`; `apply_secondary_cursor_faces`
  mutates `cell.style: TerminalStyle` directly with no
  `Cell::face()` round-trip.
- `BufferRefParams` / `BufferLineAction::BufferLine` /
  `BufferLineAction::Padding` carry `Style` end-to-end through the
  TUI walker (`paint.rs`) and the GPU walker (`walk_scene.rs`);
  per-line `Style::from_face` round-trips are gone.
- `BufferRefState.{default,padding}_face` → `_style`.
  `salsa_inputs.rs` `BufferInput` / `StatusInput` field names
  realigned with their `Style` types.
- `state/mod.rs` and `state/tests/dirty_flags.rs` mapping table
  string literals (`default_face` → `default_style` etc.) updated
  so the `state/tests/truth.rs` structural witness matches the
  `ObservedState` field names.

**Bundled rebuild:**

- All 10 examples (`cursor-line` / `color-preview` / `sel-badge` /
  `fuzzy-finder` / `pane-manager` / `smooth-scroll` /
  `prompt-highlight` / `session-ui` / `image-preview` /
  `selection-algebra`) and the 2 guest fixtures (`surface-probe`,
  `instantiate-trap`) rebuilt with `cargo build --target
  wasm32-wasip2 --release`. Artefacts copied to
  `kasane-wasm/bundled/` (6) and `kasane-wasm/fixtures/` (12).
- The `define_plugin!` macro `theme_style_or` helper and the
  `surface-probe` guest's `host_state::get_default_face` →
  `get_default_style` migration.

**Performance after closure** (`cargo bench --bench parley_pipeline`):
warm 63.3 µs, one_line_changed ~83 µs. The +18 % `one_line_changed`
gap is structurally bounded by Parley's `shape_warm = 13.58 µs` per
L1 miss and is formally accepted under ADR-024 (well below the
200 µs SLO and the 4.17 ms 240-Hz scanout). Phase 11 perf-tune
opportunities (StyledLine alloc reuse, sub-line shape cache,
`atom_styles: Vec<Arc<Style>>`) tracked in `docs/roadmap.md` §2.2.

`cargo test --workspace`: **2494 passed**.

### Changed — ADR-031 Phase B3 Style-native cascade (PR-1..PR-3c)

Five-PR sequence on `feat/parley-color-emoji-test` that pushes
`Style` / `TerminalStyle` end-to-end through the menu, info, status,
buffer, and cursor render paths. Internal API migration only — no
Kakoune wire format change, no plugin ABI change.

- **`54a466b7`** (PR-1) — retired the `ColorResolver` Style→Face→Style
  round-trips on the GPU `FillRect` / `DrawBorder` / `DrawBorderTitle`
  / `DrawPaddingRow` matchers and the dead-code `scene_graph.rs`
  scaffold (`ResolveFaceFn` → `ResolveStyleFn` type alias). The
  817b61da Phase A migration had only covered the paragraph paths;
  this commit closes the remaining four matchers.
- **`34f30e54`** (PR-2) — `Theme` API became `Style`-native. `set` /
  `get` / `resolve(&_, &Face) -> Face` / `resolve_with_protocol_fallback(_,
  Face) -> Face` retired in favour of `set_style` / `get_style` /
  `resolve(_, &Style) -> Style`. `AppView::theme_face` →
  `theme_style(token) -> Option<&Style>`. The four production
  `resolve_with_protocol_fallback` callsites (`view/info`, `view/menu`,
  `view/mod ×2`) all already held a `Style` ready, so a Style→Face→Style
  round-trip on every status / menu / info repaint disappears.
- **`7815e3c2`** (PR-3a) — `view/info` / `view/menu` / `view/mod` /
  `salsa_views/{info,menu,status}` / `render::builders` helpers
  (`truncate_atoms`, `wrap_content_lines`, `build_content_column`,
  `build_scrollbar`, `build_styled_line_with_base`) consume `&Style`.
  ~12 `Style::from_face(&face)` round-trips in `view/menu` collapse to
  direct `style.clone()`; the docstring portion of split menu items
  uses `resolve_style(&atom.style, &style)` instead of
  `Style::from_face(&resolve_face(&atom.face(), &face))`.
- **`eba04c4a`** (PR-3b) — `CellGrid` mutation API takes
  `&TerminalStyle` directly: `clear` / `clear_region` / `fill_row` /
  `fill_region` / `put_char` all match the internal
  `Cell.style: TerminalStyle` storage.
  `put_line_with_base(_, _, _, _, base_style: Option<&Style>)` uses
  `resolve_style` on the atom's existing `Arc<UnresolvedStyle>` and
  converts to `TerminalStyle` once per atom rather than once per
  grapheme. `paint_text` / `paint_shadow` / `paint_border` /
  `paint_border_title` cache one `TerminalStyle` per call site.
  ~250 test sites cascade.
- **`6ce6e75b`** (PR-3c) — GPU `process_draw_text` / `emit_text` /
  `emit_atoms` / `emit_decorations` consume `&Style`.
  `emit_decorations` reads `style.underline.style: DecorationStyle`
  enum and `style.strikethrough` directly instead of the
  `face.attributes.contains(Attributes::*UNDERLINE*)` bitflag cascade.
  Underline / strikethrough thickness now also honours the
  per-decoration `TextDecoration.thickness: Option<f32>` override
  (previously only the metrics-derived default was used).

The `Atom::from_face` test cascade noted as ~250 refs in the
B3 commits 1-5 status was already complete pre-branch — Block E
(`75439f1f` + `3724556f`) had migrated all post-resolve sites. The 13
remaining `Atom::from_face` callsites are wire-aware (cursor_face
with `FINAL_FG`, `detect_cursors` fixture, parser, `test_support::wire`).

Bridge function deletion (`Style::from_face` / `to_face` /
`to_face_with_attrs`, `UnresolvedStyle::to_face`, `Atom::face`,
`Cell::face`, `From<Face> for Style` / `for ElementStyle`,
`TerminalStyle::from_face`, `sgr::emit_sgr_diff(Face)` shim) and the
`Face` / `Color` / `Attributes` `pub(in crate::protocol)` downgrade are
the remaining Phase B3 commits 6-7.

### Changed — ADR-031 Phase B3 commits 1-5 (plugin extension points de-Faced)

The plugin extension surface migrates from `Face` (the Kakoune
wire-format type) to `Style` / `Arc<UnresolvedStyle>` / `ElementStyle`
across nine atomic commits (`057a67d2..05c0be16`). Per ADR-031's
"no backward compat" stance, plugin authors writing against host
APIs see a one-shot ABI break covering all extension points in this
sweep. The Kakoune wire format is unchanged.

- **protocol**: `KakouneRequest` enum fields migrate from `Face` to
  `Arc<UnresolvedStyle>`. `Draw { default_face, padding_face }` →
  `{ default_style, padding_style }`; `DrawStatus { default_face }`
  → `{ default_style }`; `MenuShow { selected_item_face, menu_face }`
  → `{ selected_item_style, menu_style }`; `InfoShow { face }`
  → `{ info_style }`. The parser's per-request interner shares Arcs
  across atoms and the new style fields when the wire face is
  identical (`bca4d5b5`).
- **element tree**: `kasane_core::element::Style` enum renamed to
  `ElementStyle` to remove the long-standing collision with
  `protocol::Style`. `Direct(Face)` variant replaced by
  `Inline(Arc<UnresolvedStyle>)`; `From<Face> for ElementStyle`
  preserves call-site ergonomics. `BorderConfig.face: Option<Style>`
  → `style: Option<ElementStyle>` (`930d1132`, `2c56f610`).
- **element constructors**: `Element::plain_text(s)` and
  `Element::styled_text(s, ElementStyle)` introduced; together with
  the existing `Atom::plain(s)` they collapse 316 explicit
  `Face::default()` references at authoring sites (`11c5ddea`).
  `Element::text(s, face: Face)` is retained as a transitional bridge.
- **plugin api (transforms)**: `ElementPatch::ModifyFace { overlay: Face }`
  → `ModifyStyle { overlay: Arc<UnresolvedStyle> }`. `WrapContainer
  { face: Face }` → `WrapContainer { style: Arc<UnresolvedStyle> }`.
  `Hash`/`Eq` impls deref the Arc so Salsa memoization keys remain
  content-based (`b4445770`).
- **plugin api (annotation)**: `BackgroundLayer { face: Face }`
  → `{ style: Style }` (`844fff10`).
- **plugin api (decoration)**: `CellDecoration { face: Face,
  merge: FaceMerge }` → `{ style: Style, merge: FaceMerge }`. New
  `FaceMerge::apply_to_terminal(&mut TerminalStyle, &Style)` mirrors
  the legacy semantics directly on the cell-grid representation
  (`846ca960`).
- **render hot path**: `Cell::with_face_mut` and `Cell::set_face`
  retired in favour of `Cell::with_style_mut<F: FnOnce(&mut
  TerminalStyle)>` operating directly on the stored style. The
  `TerminalStyle ↔ Face ↔ bitflags` round-trip on every decoration /
  ornament merge is eliminated. 8 hot-path callers migrated to use
  `apply_to_terminal` (`05c0be16`).
- **state derivation**: `state/derived/cursor.rs` reads
  `atom.unresolved_style()` directly instead of routing through
  `atom.face().attributes.contains(...)`. Same wire-format semantics
  (FINAL_FG + REVERSE = cursor); per-frame per-line scan no longer
  pays the Face projection (`057a67d2`).
- **performance**: `parley/frame_warm_24_lines` 65.1 → 64.4 µs (−1.0 %)
  vs Phase 11 case A baseline. `parley/frame_one_line_changed_24_lines`
  84.4 → 81.6 µs (−3.3 %), narrowing the gap toward the Phase 11
  closure target without crossing it (the ~12 µs residual remains
  bounded by `shape_warm` + L1-miss raster, per the existing closure
  framework).

**Pending Phase B3 commits 6-7:** internal-only bridge cleanup
(`Atom::from_face`/`face`, `Style::from_face`/`to_face`,
`UnresolvedStyle::from_face`/`to_face`, `Theme::set/get/resolve` Face
versions, `FaceMerge::apply` Face version, `Cell::face()` accessor,
`From<Face> for Style`/`for ElementStyle`, `TerminalStyle::from_face`)
followed by `Face`/`Color`/`Attributes` visibility downgrade to
`pub(in crate::protocol)`. ~250 test/bench refs cascade. The new
`Atom::with_style(text, Style)` constructor (`c7e21b36`) provides
the migration vehicle. Tracked on `phase-b3-block-e`.

### Changed — ADR-031 Phase 3 design-δ + Phase 10 SDK closure round

Closes the bulk of the ADR-031 follow-up backlog. **Cell representation
shifts from `Face` to `TerminalStyle`** (Copy, ~50 bytes, SGR-emit-ready),
retiring the per-cell `TerminalStyle::from_face(&cell.face)` projection
that was paid every frame on every visible cell by both the TUI backend
and the GUI cell renderer. `Face` survives only at the API surface
(paint.rs, decoration, theme, plugin API), bridged via `Cell::face()` /
`Cell::with_face_mut`. Full `Face` removal is tracked as a non-blocking
follow-up.

- **core**: `kasane_core::render::TerminalStyle` (moved from `kasane-tui`).
  `Cell { grapheme, style: TerminalStyle, width }` replaces `Cell { grapheme,
  face: Face, width }`. Grid functions (`put_char` / `clear` / `fill_row`
  / `fill_region` / `clear_region`) keep their `&Face` API surface and
  project internally; `Cell::face()` and `Cell::with_face_mut(|f| ...)`
  bridge the legacy field-access pattern.
- **tui**: `backend.rs` reads `cell.style` directly into
  `emit_sgr_diff_style`. The local `terminal_style` module is now a
  re-export of `kasane_core::render::{TerminalStyle, UnderlineKind}`.
- **gui**: `cell_renderer.rs` reads `cell.style.fg` / `cell.style.bg` /
  `cell.style.reverse` directly, dropping the `Face`-routed
  `attributes.contains(REVERSE)` indirection.
- **wasm**: `atom_to_wit` switches to `style_to_wit(&a.style_resolved_default())`,
  retiring the `Style::from_face(&a.face())` round-trip on the
  native→wire path. The wire `Style` is post-resolve per the Phase A.4
  split contract.
- **plugin sdk macros**: `define_plugin! { paint_inline_box(box_id) { ... } }`
  section parser added. Bundled WASM plugins can now override Phase 10
  inline-box paint without dropping out of the macro DSL. Capability
  flag (`INLINE_BOX_PAINTER = 1 << 13`) auto-detected from the emitted
  function name.
- **core (plugin)**: `PluginView::paint_inline_box` enforces a per-thread
  `MAX_INLINE_BOX_DEPTH = 8` recursion bound and detects self-cycles /
  mutual cycles between inline-box owners. Overflow and cycle errors
  log once per `(plugin_id, box_id)` pair; subsequent re-entries return
  `None` silently. Hardens the host against malicious or buggy reentrancy
  in `paint_inline_box` chains.
- **gui (tests)**: hit_test coverage extends to RTL Arabic
  (`is_rtl == true` post ICU4X bidi), combining marks (`e + U+0301`),
  ZWJ family emoji (`👨‍👩‍👧‍👦`), and trailing-position visual offset.
  Mixed RTL+LTR direction alternation and narrow-CJK + ASCII advance
  monotonicity also pinned to address the input class that motivated
  ADR-031 §動機 (1).
- **gui (tests)**: L1 LayoutCache negative tests added for decoration
  colour, decoration thickness, and strikethrough colour — all
  paint-time properties that must NOT evict the shaped layout cache.
- **docs**: `semantics.md` § "InlineBox boundary against ShadowCursor"
  pins the three invariants (placement exclusion, width accounting,
  EditProjection unit boundary). `decisions.md` ADR-031 gains a
  § Next-ADR seeds table — five workstreams (WIT 2.0, Atom interner,
  Display↔Parley canonical coordinate utility, Atlas pressure policy,
  Vello adoption) that future engineers pick up without re-deriving
  the constraints.

### Added — ADR-032 Vello evaluation framework (in flight)

Forward-looking framework that re-opens the ADR-014 Vello rejection in light of
2026 Q1 changes (Glifo glyph caching, Vello Hybrid GPU/CPU path). **No
production renderer change** — current `winit + wgpu + Parley + swash` stack
remains authoritative until ADR-032 is updated to "Accepted with adoption plan"
based on a future spike outcome.

- **gui**: `kasane_gui::gpu::backend::GpuBackend` trait — current
  `SceneRenderer` implements it via pass-through; reserved for a future
  Vello-backed implementor. `BackendCapabilities { supports_paths,
  supports_compute, atlas_kind }` for runtime feature negotiation. Pure
  additive; no production call site changes.
- **gui (tests)**: headless wgpu golden-image harness scaffold at
  `kasane-gui/tests/golden_render.rs`. Renders to an offscreen RGBA8 texture,
  reads back via `copy_texture_to_buffer`, compares with DSSIM via
  `image-compare`. Snapshots at `kasane-gui/tests/golden/snapshots/`. Update
  via `KASANE_GOLDEN_UPDATE=1`. Sandboxed environments without GPU access
  graceful-skip rather than fail. Pipeline-level fixtures (QuadPipeline,
  ImagePipeline, full SceneRenderer) tracked as W2 Phase 2 follow-up.
- **workspace**: new `kasane-vello-spike` member — isolated, exploratory
  crate that hosts a stub `VelloBackend` behind the `with-vello` feature
  flag. Pinned to `vello_hybrid = 0.0.7`. With the feature off, all methods
  return `BackendError::Unsupported`; with the feature on, the impl is a
  documented `todo!()` placeholder pending Glifo crates.io publication and
  the 5-day spike timebox per ADR-032 §Spike Plan.
- **docs**: ADR-032 in `docs/decisions.md` (Status: Proposed); roadmap
  Backlog entry in `docs/roadmap.md` §2.2 with externalised triggers
  (Vello ≥ 1.0, Glifo on crates.io, spike `frame_warm_24_lines` ≤ 70 µs).

### Changed — ADR-031 Parley text stack migration (Phase 11 lands)

The GPU text pipeline is now Parley + swash end-to-end; cosmic-text and the
glyphon-derived `text_pipeline` (`TextRenderer`, `TextAtlas`, `TextArea`,
`TextBounds`, `ColorMode`) are gone, along with the cosmic-text-backed
`LineShapingCache` and the `text_helpers` shim. The
`KASANE_TEXT_BACKEND=parley` opt-in is removed; Parley is the only backend.

- **gui**: `SceneRenderer` is Parley-only. Buffer text, atom rows, status bar,
  menus, info popups, padding rows, and decorations all flow through
  `parley_text::shaper` → swash rasteriser → L2 `GlyphRasterCache` →
  `GpuAtlasShelf` → `ParleyTextRenderer`.
- **gui**: L2 cache uses frame-epoch eviction. Same-frame entries cannot be
  evicted (their drawables are already queued); stale entries from earlier
  frames remain candidates. Fixes the "info-popup glyphs scramble" symptom
  caused by mid-frame slot reuse.
- **gui**: Decoration geometry (underline / strikethrough offset + thickness)
  is driven from the font's own `RunMetrics`, not a `cell_h × ratio`
  heuristic.
- **gui**: Mouse hit_test temporarily falls back to cell-grid resolution.
  Glyph-accurate per-cluster hit testing through `parley_text::hit_test` is
  still on the roadmap; CJK and ligature cursor positioning may be off by
  fractions of a cell until that wires in.
- **deps**: `cosmic-text` removed from `kasane-gui` and the workspace.
  `parley = 0.9` + `swash = 0.2` are now the production text stack.
- **diagnostics**: `KASANE_PARLEY_NO_CACHE=1` invalidates the L2 cache and
  clears both atlases per frame for atlas / eviction debugging.

### Added — earlier ADR-031 phases (already shipped)

- **core**: `protocol::Style` Parley-native text style alongside the legacy
  `Face`. Continuous `FontWeight` (100..=900), `FontSlant`, `FontFeatures`
  bitset, `FontVariation` axes, `BidiOverride`, and `TextDecoration` with
  five `DecorationStyle` variants (Solid/Curly/Dotted/Dashed/Double).
- **gui**: `kasane-gui::gpu::parley_text` module — facade (`ParleyText`),
  shaper, L1 `LayoutCache`, swash glyph rasteriser (4-level subpixel x
  quantisation, color emoji via `Source::ColorOutline` → `ColorBitmap` →
  `Outline` → `Bitmap` priority), L2 `GlyphRasterCache` + `GpuAtlasShelf`
  with frame-epoch-aware eviction.
- **gui**: `parley_text::hit_test` provides `(x, y) → byte_offset` and
  `byte → x_advance` helpers built on `parley::Cluster::from_point` /
  `from_byte_index`. Bidi-aware (`HitResult::is_rtl`).
- **bench**: `cargo bench --bench parley_pipeline` measures the new pipeline.

See [ADR-031](docs/decisions.md#adr-031-text-stack-migration--cosmic-text--parley--swash-with-protocol-style-redesign) for the full decision record and phase plan.

## [0.5.0] - 2026-04-10

### Highlights

- **Declarative widget system**: Customize the status bar, add line numbers, highlight the cursor line, apply mode-dependent colors — all from KDL, no plugins required. Six widget kinds (contribution, background, transform, gutter, inline, virtual-text) with templates, conditions, theme token references, and 40+ variables.
- **Unified KDL configuration**: `config.toml` replaced by `kasane.kdl` with live hot-reload (~100ms, notify-based). See the [migration guide](docs/config.md#migrating-from-v040) for conversion examples.
- **`kasane init`**: One command to generate a starter `kasane.kdl` with sensible widget defaults.
- **Widget CLI**: `kasane widget check [-v] [--watch]` to validate widget definitions without starting Kasane, plus `kasane widget variables` / `kasane widget slots` for discovery.

### Breaking Changes

- **config**: Configuration file format changed from TOML (`config.toml`) to KDL (`kasane.kdl`). Kasane detects a stale `config.toml` on startup and prints a warning. There is no automatic migrator — the structural mapping is mechanical; see [docs/config.md § Migrating from v0.4.0](docs/config.md#migrating-from-v040) for side-by-side examples (0f7d4a60)
- **widget**: Top-level widget definitions (flat form, outside a `widgets {}` block) are now a hard error. Wrap your widgets in `widgets { ... }` (544b548e)
- **core**: Removed `PaintHook` trait — it had no external consumers. Use `RenderOrnaments` instead (496cb5e3)

### Added

- **widget**: Declarative widget system with six kinds — contribution (status bar slots), background (cursor line / selection), transform (face overlay on existing elements), gutter (per-line annotations), inline (pattern-match highlighting), virtual-text (end-of-line text) (a52165b4, cf1a29a9)
- **widget**: Template syntax with format specs — `{var}`, `{var:N}` (left-align), `{var:>N}` (right-align), `{var:.N}` (truncate with ellipsis), `{var:>N.M}` (combined), unicode-width aware (0b5c159b, f03db2e4)
- **widget**: Inline template conditionals — `{?condition => then => else}`, nested branches, variables and formatting inside branches (6d4f1682, 41487e93)
- **widget**: Condition expressions with comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`), regex match (`=~`), set membership (`in`), logical (`&&`, `||`, `!`), and parentheses; 16-node / 256-char limits (f071eb72, 41487e93)
- **widget**: Multi-effect widgets — combine contribution, background, transform, etc. under a shared `when=` condition in a single block (41487e93)
- **widget**: Widget groups — `group when="cond" { ... }` shares a condition across multiple named children with implicit AND composition and nesting (f03db2e4)
- **widget**: Widget ordering via `order=` attribute (falls back to file order) (f03db2e4)
- **widget**: Widget includes — `include "path/*.kdl"` with glob patterns, `~` expansion, and circular-include detection; all included files are watched for hot-reload (41487e93)
- **widget**: `opt.*` variable bridge — read any Kakoune `ui_options` value (`{opt.git_branch}`) with smart type inference (`"42"` → `Int`, `"true"`/`"false"` → `Bool`) (00aa0348)
- **widget**: `plugin.*` variable bridge — plugins can expose named values via `Command::ExposeVariable` (6d4f1682)
- **widget**: Theme token references — `face="@status_line"` (with `.` / `_` normalization) auto-updates on theme change (6ba9cb27)
- **widget**: Gutter per-line variables — `line_number`, `relative_line`, `is_cursor_line` for per-line templates and `line-when=` conditions (cf1a29a9)
- **widget**: Gutter branching (`GutterBranch`) for cursor-line / other-line display (544b548e)
- **widget**: Parse diagnostics routed to the diagnostic overlay; fuzzy suggestions for unknown variables; duplicate-name warnings (babcbef4, 3cbd9254)
- **config**: Hot-reload via `notify` filesystem watcher with 100ms debounce and 2s polling fallback; content-hash diffing skips re-parse on unchanged content (6ba9cb27, 41487e93)
- **config**: Restart-required field detection — warns when hot-reload touches fields that require a restart (`ui.backend`, `ui.border_style`, `ui.image_protocol`, `scroll.lines_per_scroll`, `window`, `font`, `log`, `plugins`) (f69cfbee)
- **config**: Startup detection of a legacy v0.4.0 `config.toml` with migration guidance
- **config**: Fuzzy suggestions for unknown top-level config sections (f03db2e4)
- **cli**: `kasane init` generates a starter `kasane.kdl` with mode, cursor position, line numbers, and cursor-line widgets (b9612fb2)
- **cli**: `kasane widget check [path] [-v|--verbose] [--watch]` validates widget definitions without starting Kasane; `--watch` re-validates on save (a52165b4, f03db2e4)
- **cli**: `kasane widget variables` / `kasane widget slots` list available template variables and layout slots (f03db2e4)
- **display**: `InverseResult` enum replacing `Option<BufferLine>` for clearer display-unit inverse semantics; `DirectiveStabilityMonitor` for oscillation detection; sealed `FrameworkAccess` trait (494443ef)
- **plugins**: Bundle `smooth-scroll` plugin (default-disabled, opt-in via `plugins { enabled "smooth_scroll" }`) (5db47a0a)

### Fixed

- **widget**: Unicode display width used for template padding/truncation — correct handling of CJK and emoji (0b5c159b)
- **widget**: `opt.*` variables resolve with typed values so `opt.tabstop = "0"` is correctly falsy (00aa0348)
- **widget**: Warn on duplicate widget names during parse (last-wins behavior preserved) (3cbd9254)
- **widget**: Dedicated `CondParseError::TooLong` error for the 256-character condition length limit (f071eb72)
- **nix**: Packaging improvements for nixpkgs submission (319a6fcd, e527f225)

### Changed

- **docs**: README rewritten for clarity and impact (b2c9373a)
- **docs**: Widget system comprehensive reference in `docs/widgets.md`; WASM workstream roadmap cleanup (6ba9cb27, 1933b8bb)
- **docs**: Replace obsolete `decorate_cells()` / `cursor_style_override()` references with `render_ornaments()` (5db47a0a)

### Internal

- Unify `config.toml` + `widgets.kdl` into a single `kasane.kdl` parser; format-preserving save via `patch_config_in_document()`; consolidate `Event::WidgetReload` + `Event::ConfigReload` into `Event::FileReload`; drop the `toml` dependency from kasane-core (0f7d4a60)
- Typed `Value` enum (Int/Str/Bool/Empty) replacing string-based widget variable resolution (544b548e)
- Unified `Predicate` algebra merging widget `CondExpr` with element-patch `PatchPredicate` (6d4f1682)
- `VariableRegistry` replacing three separate data sources; `WidgetPlugin` + `HandlerRegistry` replacing `SingleWidgetBackend`; `Style::Token` passthrough for deferred theme resolution (6d4f1682)
- Widget visitor pattern eliminating ~170 lines of duplication across parse/register paths (41487e93)
- Per-widget `WidgetPlugin` instances via the plugin `HandlerRegistry` — widgets share the entire plugin composition infrastructure (544b548e)
- `notify`-based file watcher replacing 2s mtime polling; content-hash diffing to skip re-parse on `touch`-like changes (41487e93)
- `ConfigError` diagnostic kind with cyan `"C"` tag, separate from `RuntimeError` (f03db2e4)

## [0.3.0] - 2026-03-29

### Highlights

- **Plugin architecture redesign**: HandlerRegistry model replaces 30+ Plugin trait methods with `register()` + `handle()` (ADR-025–029)
- **Display Unit Model**: Unified display coordinate system (DU-1 through DU-4) with virtual-text-aware mouse translation
- **Declarative key map DSL**: Framework-managed chord sequences with `KeyMap` builder
- **Image rendering pipeline**: SVG support (resvg), Kitty Graphics Protocol, GPU texture rendering
- **Plugin manifest system**: `kasane-plugin.toml` for declarative plugin metadata and activation

### Breaking Changes

- **plugin**: `HandlerRegistry` replaces 30+ `Plugin` trait methods with `register()` + `handle()`
- **plugin**: Unified `Effects` type replaces `BootstrapEffects`/`SessionReadyEffects`/`RuntimeEffects`
- **wasm**: WIT key-code breaking change: `character(string)` → `char(u32)`
- **wasm**: `kasane-plugin.toml` manifest required for all WASM plugins
- **sdk**: kasane-plugin-sdk 0.3.0 (requires kasane >= 0.3.0)

### Added

- **plugin**: Implement plugin architecture redesign — HandlerRegistry, capability derivation from handler presence, exhaustive dispatch (a5c57e2, ed29da8)
- **plugin**: Add `PluginTag` ownership to `InteractiveId` for namespace isolation and O(1) dispatch (490f1e9)
- **plugin**: Plugin authoring ergonomics overhaul (12ea5bc)
- **plugin**: Implement plugin manifest system with `kasane-plugin.toml` (24386ae)
- **plugin**: Implement EOL virtual text (Phase VT-1) (73cf6f1)
- **plugin**: Add cursor decoration plugin extension APIs with `decorate_cells()` (WIT v0.19.0) (e9e5d07)
- **display**: Implement Display Unit Model (DU-1 through DU-4) (7edb96a)
- **display**: Add `DisplayDirective::InsertBefore` for virtual text before buffer lines (WIT v0.17.0) (9c575eb)
- **display**: Implement display scroll offset for virtual line overflow (c33b45b)
- **display**: Extend `InsertAfter`/`Fold` to `Vec<Atom>` and add `get-active-session-name` (7a7cc5f)
- **input**: Declarative key map DSL with framework-managed chords (5b3513a)
- **core**: Add `Element::Image` type for GPU rendering with TUI text placeholder fallback (48a0338)
- **core**: Add SVG rendering support with resvg (b8dfd2a)
- **core**: Integrate SVG into TUI halfblock rendering path (25337d7)
- **core**: Split divider glyphs with focus-adjacency detection and TUI halfblock image rendering (70731eb)
- **gui**: Implement Image element GPU rendering pipeline with texture caching (20bb2e0)
- **gui**: Integrate SVG into GPU texture rendering path (3c0d8ca)
- **gui**: Update cosmic-text to 0.18 and enable font hinting (298cb45)
- **tui**: Add Kitty Graphics Protocol support for high-quality image rendering (48b8ef2)
- **tui**: Integrate SVG into Kitty Graphics Protocol path (2959c02)
- **wasm**: Expose buffer file path via `get-buffer-file-path` (WIT v0.15.0) (13dbff8)
- **wasm**: Add image element API `create-image` for WASM plugins (WIT v0.20.0) (91f76c7)
- **wasm**: Add workspace resize command (WIT v0.21.0) (377ef79)
- **wasm**: Add `svg-data` image source variant (WIT v0.22.0) (ba7b02c)
- **wasm**: Add `image-preview` WASM plugin example (a30fbaa)
- **wasm**: Add SDK v0.3.0 DX helpers and migrate examples (841002d)
- **wasm**: Improve plugin DX with `define_plugin!`, `view_deps`, logging, and runtime diagnostics (96b9ec9)
- **wasm**: Add bulk buffer line retrieval APIs `get-lines-text`, `get-lines-atoms` (WIT v0.18.0) (3d98b42)
- **inline**: Add `InlineOp::Insert` for inline virtual text insertion (WIT v0.16.0) (1357627)
- **pane**: Per-pane status bar rendering in multi-pane mode (beeca62)
- **pane**: Implement directional pane resize key bindings `<C-w>>/<` (d975a4a)
- **workspace**: Add pane layout persistence across sessions (8a1aadb)
- **nix**: Add Nix package derivation with `cleanSourceWith` filtering (d4e2a24)
- **nix**: Add `packages` output to flake.nix (07786ba)

### Fixed

- **gui**: Add gamma-correct sRGB→linear conversion in GPU shaders (5f399e4)
- **gui**: Fix unlimited frame rate and improve GPU backend (fb89f96)
- **gui**: Handle REVERSE attribute and sync default colors from Kakoune theme (cd07cba)
- **gui**: Correct `ImageFit::Contain` and harden image pipeline caching (04a7002)
- **core**: Comprehensive color/face system remediation (5f97282)
- **core**: Integrate plugin transforms into Salsa rendering path (25035b5)
- **core**: Persist `DisplayMap` on `AppState` for mouse coordinate translation (6fd2247)
- **tui**: Use inline RGBA transfer for Kitty image uploads instead of file path (a4984b0)
- **test**: Gate `debug_assert` `#[should_panic]` tests with `cfg(debug_assertions)` (4207dd0)

### Changed

- **sdk**: Bump kasane-plugin-sdk to 0.3.0; WIT ABI from 0.14.0 to 0.22.0
- **gui**: Internalize glyphon as `text_pipeline` module (24a353e)
- **deps**: Update portable-pty to 0.9 (24c7cd3)

### Internal

- Structural cleanup — split large modules, remove deprecated API, type-safe config (8884f99)
- Nix/cargo CI caching and fix `cargo metadata` running outside Nix (758877c)
- CI fixes: POSIX grep, shellHook stdout isolation, lychee-action reference (cb840ae, 6069432, ca5dd2c)
- Comprehensive documentation refresh: plugin cookbook, design documents, README rewrite, ADR-024/025–029

## [0.2.0] - 2026-03-23

### Highlights

- **Multi-pane support**: Split the editor into multiple panes with independent Kakoune sessions, directional focus navigation (`<C-w>h/j/k/l`), and per-pane rendering with overlay offset correction
- **Salsa incremental computation**: Replace the hand-rolled ViewCache/PaintPatch/LayoutCache stack with Salsa 0.26 as the sole caching layer, yielding simpler code and automatic dependency tracking
- **WASM plugin SDK maturity**: Publish `kasane-plugin-sdk` 0.2.0 to crates.io with `#[plugin]` proc macro, `define_plugin!` with `#[bind]`, typed effects, and provider-based loading
- **Display transform system**: Add `DisplayMap` with virtual text support and multi-plugin directive composition, enabling byte-range `InlineDecoration` (Style/Hide) on buffer lines
- **Smooth scroll as a plugin**: Extract scroll runtime into a host-owned policy hook, expose it to WASM plugins, and ship the `smooth-scroll` example
- **Monoidal plugin composition**: Algebraize the transform system with `TransformChain` monoid, `TransformSubject` sum type for overlay-aware transforms, and 4-element sort keys for commutativity
- **Color system redesign**: Implement `ColorContext` derivation with `rgba:` color support and improved cursor detection for third-party themes

### Added

- **pane**: Add `PaneMap` data structure with auto-generated server session names, per-pane rendering via `BufferRefState`, command routing with `SpawnPaneClient`/`ClosePaneClient`, TUI/GUI event routing, pane resize, and `KakouneDied` cleanup
- **pane**: Add `<C-w>h/j/k/l/W` directional focus bindings and migrate `WindowModePlugin` to `PaneManagerPlugin`
- **pane**: Migrate pane management to a WASM plugin with workspace authority
- **salsa**: Add Salsa incremental computation layer and integrate into TUI/GUI event loops; deepen to ViewCache-free rendering path
- **plugin**: Add monoidal composition framework for extension points with `TransformChain` monoid and target hierarchy
- **plugin**: Add plugin extensibility features G1-G8 (view_deps, typed effects, provider-based loading, transactional reload, diagnostics overlay)
- **plugin**: Introduce `TransformSubject` sum type for overlay-aware transforms
- **plugin**: Introduce `AppView<'a>` to decouple plugins from `AppState` internals
- **display**: Add `DisplayMap` foundation with virtual text support and multi-plugin display directive composition (P-031)
- **annotation**: Add `InlineDecoration` for byte-range Style/Hide on buffer lines
- **scroll**: Extract host-owned scroll runtime and policy hook; expose to WASM plugins; add `smooth-scroll` WASM example
- **theme**: Implement color system redesign with `ColorContext` derivation
- **session**: Add session observability infrastructure (ADR-023), enrich session descriptors with `buffer_name` and `mode_line`, add session affinity with correctness proof
- **process**: Separate Kakoune into headless daemon and client processes
- **protocol**: Add `StatusStyle` from Kakoune PR #5458
- **sdk**: Add `#[plugin]` proc macro to auto-fill Guest trait defaults; improve `define_plugin!` with `#[bind]`, auto state access, and `StateMutGuard`; prepare for crates.io publish
- **cli**: Add `kasane plugin` subcommand for WASM plugin workflow
- **gui**: Add `DecorationPipeline` for text decoration rendering (R-053)
- **macros**: Add `#[epistemic(...)]` compile-time classification for `AppState` fields
- **inference**: Add documentation, cross-validation, and proptest for inference rules
- **examples**: Replace `line-numbers` native example with `prompt-highlight` transform example
- **dist**: Add AUR `kasane-bin` package, Homebrew formula with auto-update workflow

### Fixed

- **protocol**: Support `rgba:` colors and improve cursor detection for third-party themes
- **protocol**: Make `widget_columns` optional in `draw` protocol parsing
- **render**: Fix `MenuSelect` dirty flags bug; add `MENU_STRUCTURE` to info overlay cache deps
- **core**: Fix info overlay collision with menu and `MenuSelectionPatch` crashes
- **layout**: Add rounding to flexbox space distribution
- **plugin**: Use 4-element sort key for `DirectiveSet` commutativity; enforce inline decoration uniqueness in release builds
- **plugin**: Deterministic plugin ordering
- **pane**: Route all commands to focused pane writer
- **diagnostics**: Account for tag+space overhead in overlay width calculation
- **session**: Fix session lifecycle bugs and complete multi-session UI parity
- **wasm**: Update SDK macro default dirty deps to include `SESSION` bit; respect disabled config for bundled plugins

### Performance

- **core**: Stratified incremental composition (SIC) phases I and II
- Strengthen performance stance with allocation budget enforcement, CI guards, and Salsa latency regression test

### Changed

- **plugin**: Unify `Plugin`/`PurePlugin` naming -- `PurePlugin` becomes `Plugin` (ADR-022)
- **plugin**: Externalize effects for TEA purity; extract `PluginEffects` trait to decouple `update()` from `PluginRuntime`
- **plugin**: Switch runtime and WASM ABI to typed effects; make plugin authoring typed-only
- **plugin**: Provider-based plugin loading with structured activation diagnostics
- **plugin**: Transactional plugin reload with delta-based resource reconciliation
- **render**: Abolish `RenderBackend` trait; extract `SystemClipboard`; move diff engine to `TuiBackend`
- **render**: Unify dual paint pipeline via Visitor pattern
- **salsa**: Remove `salsa-view` feature flag -- Salsa is now mandatory (ADR-020)
- **sdk**: Bump `kasane-plugin-sdk` to 0.2.0; bump WASM plugin ABI to `kasane:plugin@0.14.0`

### Internal

- Remove legacy caching infrastructure: `PaintPatch`, `ViewCache`, `ComponentCache`, `LayoutCache`, `cache.rs`, plugin `*_deps()` methods, `FIELD_FLAG_MAP`/`StateFieldVisitor` macros, and `DirtyFlags` guards from Salsa sync
- Split `event_loop` god module into focused submodules; split `salsa_views.rs` into submodules
- Consolidate `PluginRuntime` parallel `Vec`s into `PluginSlot`; introduce `EventResult` struct
- Replace bare `unwrap()` with descriptive `expect()` messages across the codebase
- Unify test `Surface` mocks into `TestSurfaceBuilder`; add proptest for `DisplayMap` invariants and cascade depth limits
- Add Renovate for automated dependency updates; add SRCINFO consistency check in CI
- Consolidate and deduplicate documentation: absorb `architecture.md` into `index.md`, merge performance docs, remove stale reference files
