# ADR-039: Plugin Path Consolidation (R2.x)

**Status**: Accepted (2026-05-08). Supersedes [ADR-038](./adr-038-plugin-authoring-path-consolidation.md).

### Context

ADR-038 (2026-05-05) froze the R1.x super-trait migration at R1.6
on the premise that `capability_traits.rs` provided value via
narrow trait views — call sites taking `&dyn Lifecycle`,
`&dyn Contributor`, etc. instead of the full `&dyn PluginBackend`.

A 2026-05-08 audit grepping for that pattern across the workspace
returned **zero consumers**. The 1040-line `capability_traits.rs`
is dead architecture; the `impl_migrated_caps_default!` macro is
a 21-site scaffolding cost producing no realised benefit. ADR-038
decision points 1 and 3 (freeze + opportunistic builtin migration)
are therefore unjustified and ADR-039 reverses them.

The wider refactor program also unlocks a chain of transitional-API
deletions previously blocked by backwards-compatibility concerns
(now waived):

- `has_decomposed_annotations` (legacy/decomposed annotator
  discriminator) becomes obsolete once all builtins use
  `HandlerRegistry`.
- `annotate_line_with_ctx` (61-line joiner in `bridge.rs`) is the
  only `inject_owner` call site post-builtin migration.
- `WireFace` public visibility (`plugin_prelude.rs:50-53`) is
  blocked on 22 files still consuming it directly. With
  `detect_cursors` rewritten to read `UnresolvedStyle.final_*`
  fields, the public surface can collapse.
- `Atom::from_wire` vs `Atom::with_style` semantic split (memory
  `project_adr_031_phase_b3_semantic_split.md`) was preserved
  because `Atom::from_wire` is the only entry point that carries
  Kakoune `final_*` resolution flags through. `Style` builders
  (`Style::with_final_fg` etc.) make `Atom::with_style` sufficient.
- `#[deprecated] type PluginRegistry = PluginRuntime` (`plugin/mod.rs:88`)
  has been deprecated for one release.

### Decision

Execute a 12-PR program (R2.x) that:

1. **Deletes `capability_traits.rs`** (1040 LoC) and the
   `impl_migrated_caps_default!` macro across 21 sites.
2. **Migrates all 9 builtin plugins** to `Plugin + HandlerRegistry`
   (~525 LoC of `impl PluginBackend` rewritten):
   `BuiltinInputPlugin`, `BuiltinDragPlugin`, `BuiltinFoldPlugin`,
   `BuiltinMouseFallbackPlugin`, `BuiltinShadowCursorPlugin`,
   `BuiltinInfoPlugin`, `BuiltinMenuPlugin`,
   `ProjectionStatusPlugin`, plus any test fixtures.
3. **Reduces `PluginBackend` to an internal-marked ABI**
   consumed by `PluginRuntime` and the WASM adapter
   (`WasmPlugin`). The trait stays — but as ABI, not authoring
   surface. *Execution note (2026-05-08, P6 closure):* the literal
   `pub(crate)` target was not viable because `kasane-wasm` (WASM
   adapter), `kasane-tui` / `kasane` builtins, the
   `locked_wasm_provider` factory, and the `#[kasane_plugin]` proc
   macro all hold `impl PluginBackend` or `Box<dyn PluginBackend>`
   outside `kasane-core`. The achieved encapsulation is
   `#[doc(hidden)] pub` (added in P3 at `traits.rs:128`),
   removing the trait from rendered docs while preserving the
   cross-crate ABI surface. True `pub(crate)` would require a
   ~1000+ LoC WASM adapter rewrite plus migrating the proc-macro
   generation path; deferred to a future ABI-extraction
   workstream if a concrete consumer surfaces.
4. **Deletes transitional APIs** unblocked by builtin migration:
   `has_decomposed_annotations`, `annotate_line_with_ctx`,
   `Atom::from_wire`, `WireFace` public visibility,
   `#[deprecated] PluginRegistry` alias.
5. **Mechanises Bridge dispatch fully** via a new
   `dispatch_owner_inject!` macro covering the `inject_owner`
   pattern (5 methods); remaining hand-coded methods reduce to
   `try_process_task_event` and similar genuine outliers.
   `bridge.rs` target: 1900 → ~700 LoC.
6. **Splits 3 large modules** along natural axes:
   `state/shadow_cursor.rs` (types/keyboard/commit),
   `registry/collection.rs` (6 collection axes),
   `handler_registry.rs` (6 registration axes).
7. **Contracts `kasane-core` public module surface** from 28 to
   ~12 by `pub(crate)`-gating the 5 Salsa modules and gating
   `test_support` behind `cfg(any(test, feature = "test_support"))`.

The 12 PRs sequence as P0 (this ADR + roadmap entry) → P1a–P1d
(builtin migration) → P2 (vestigial deletes) → P3
(`capability_traits.rs` delete) → P4
(`has_decomposed_annotations` + `annotate_line_with_ctx` delete)
→ P5 (`PluginCapabilities` bitflag scope reduction) → P6
(`PluginBackend` `pub(crate)`) → P7 (`WireFace` visibility) → P8
(Bridge dispatch mechanisation) → P9 (`Atom::from_wire` delete)
→ P10a–P10c (structural splits) → P11 (`kasane-core` surface
contraction). Total estimate ~12 working days; parallelisable to
~8 days.

### Implications

- `Plugin + HandlerRegistry` is the **only** native plugin
  authoring path. There is no `PluginBackend`-based path for
  external consumers.
- `PluginBackend` is documented as internal ABI; the prelude
  re-export is removed.
- Build time improves modestly post-P3 (1040 LoC of generic
  blanket-impls compiled in every kasane-core build).
- The "two ways to author plugins" cognitive overhead disappears
  from documentation and from the public API surface.
- The `Atom::from_wire` / `Atom::with_style` semantic split is
  retired in favour of explicit `Style::with_final_*` builders;
  the wire parser path becomes a single-purpose internal helper.
- `WireFace` becomes `#[doc(hidden)] pub` (the literal
  `pub(in crate::protocol)` target is rejected — see Plan B
  execution note below). The wire-format type is removed from
  `plugin_prelude` and is invisible from rendered API docs.

### Plan B execution (2026-05-08, P7+P9 expansion)

The original P7 ("WireFace full visibility downgrade") was
estimated at 1 day. Discovery during execution revealed the
real scope: ~200 occurrences of `WireFace { ... }` literals
across the workspace, plus 12 kasane-core public API surfaces
holding `face: WireFace` fields (`DisplayDirective::StyleInline`/
`StyleLine`, `InlineOp::Style`, `CursorEffectOrn`, `SurfaceOrn`,
`ContainerPaintInfo`, `Command::RegisterThemeTokens`, etc.).

Plan B (8 PRs, ~2.5 days) executed the full migration:

- PR1 `7020bc52`: `Element::text(WireFace)` → `(Style)` unification
- PR2 `c84933c8`: diagnostic overlay primitives → Style
- PR3 `5f3cee58`: `ColorResolver` WireFace API removal
- PR4 `519dec14`: IME preedit overlay → Style
- PR5 `be7b25de`: bench/test fixtures → Style
- PR6 (`6c11adec`+`7ebb643a`): DisplayDirective + InlineOp + lens
  + ornament types + Container + RegisterThemeTokens → Style;
  `wit_style_to_face` deleted
- PR7 `ec95e691`: `Atom::from_wire` → `pub(crate)` (the original
  P9; merged into the Plan B sequence since P7's cascade already
  forced ~60 callers off the public API)
- PR8 (this commit): `WireFace` removed from `plugin_prelude`;
  `Atom::from_wire` doc updated; roadmap §2.2 closed; memory
  updated.

The literal `pub(in crate::protocol)` step (b) was not pursued:
- The remaining external `WireFace` consumers are
  `kasane-tui benches/backend.rs::WireAtomBench` (JSON wire
  encoder) and `kasane-wasm convert/tests` (WIT round-trip).
- Both legitimately mirror the on-the-wire JSON layout that
  Kakoune emits; suppressing them would require either moving
  the helpers into `kasane-core::test_support` (cross-crate
  refactor with no payoff — these are bench/test code only) or
  duplicating the four-field struct in those crates.
- `#[doc(hidden)] pub` already hides `WireFace` from rendered
  API docs and from `plugin_prelude`, so plugin authors never
  see it. The hardened goal is met without the additional
  restructure.

### Acceptance Evidence

- `kasane-core/src/plugin/capability_traits.rs` does not exist
  (verified by `find`).
- `grep impl_migrated_caps_default!` returns zero hits in
  production code; test fixtures use a minimal helper if
  needed.
- `grep 'impl PluginBackend for'` returns hits only for
  `PluginBridge`, `WasmPlugin`, intentional legacy-path test
  fixtures, and the `#[kasane_plugin]` proc-macro generated
  impls plus the four `kasane`/`kasane-tui` top-level builtins
  that use `PluginBackend` directly. The trait is
  `#[doc(hidden)] pub` (post-P3) — invisible from rendered docs
  but accessible across crates.
- `kasane-core/src/lib.rs` `pub mod` count is ≤ 12.
- `cargo bench --bench rendering_pipeline frame_warm_24_lines`
  shows no regression vs the pre-program baseline (≤ 70 µs at
  80×24).
- All workspace tests pass.

### Rejected Alternatives

1. **Honour ADR-038 freeze and accept the dead architecture.**
   Rejected: 1040 LoC of unused code is documented technical
   debt that compounds future plugin extension work. The
   `capability_traits.rs` file's existence forces every new
   plugin extension point to consider whether it should also
   become a capability trait — a decision that has zero
   correct answer because the trait views have no consumers.
2. **Migrate WASM adapter to HandlerRegistry as part of this
   program.** Rejected for the same reason as in ADR-038
   §Rejected #4: ~1600-line mechanical WIT translation with no
   consumer benefit. Out of scope.
3. **Extract `PluginBackend` to a separate `kasane-plugin-abi`
   crate.** Considered as P6 alternative. Rejected for this
   program: `pub(crate)` achieves the encapsulation goal
   without the cross-crate refactor cost. Re-evaluate if a
   future workstream surfaces ABI-stability requirements.
4. **Keep `Atom::from_wire` indefinitely as a wire-parser
   helper.** Rejected: the wire parser is a single internal
   call site; an explicit `parse_wire_atom()` function in
   `protocol::parser` keeps the call site clear without
   bifurcating the public `Atom` constructor surface.

### Migration Notes

This ADR is implemented incrementally; no single PR carries the
full migration. Each P# is independently revertible. The
reversibility chain is:

- P3 / P4 / P5 / P6 form a forward-only chain (each strips a
  layer that the next removes); reverting P5 requires also
  reverting P6 (and so on).
- P1, P2, P7, P8, P9, P10, P11 are each independently revertible
  in isolation.

If unforeseen consumers of `capability_traits.rs` surface during
P3, the PR is held until they migrate to `PluginBackend` directly
or to `HandlerRegistry`. The narrow-trait-views design is not
restored.
