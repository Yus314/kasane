# ADR-030: Observed/Policy Separation — Staged Projection Rollout

**Status:** Current (Levels 1–6 shipped). Level-3 / Level-5 type names
renamed per [ADR-044](./adr-044-handler-effect-tier-hierarchy.md): the
projection types previously called `KakouneSafeCommand` /
`KakouneSafeEffects` / `KakouneSafeKeyResult` are now
`KakouneTransparentCommand` / `KakouneTransparentEffects` /
`KakouneTransparentKeyResult`. The rename frees the `KakouneSafe`
namespace for the tier hierarchy and better names what these types
witness — that the handler cannot write to Kakoune state, which ADR-030
already calls *transparency*. Source files moved from
`kasane-core/src/plugin/kakoune_safe_command.rs` →
`kakoune_transparent_command.rs` (similarly for `_effects.rs`).

### Context

Requirement P-032 (`docs/requirements.md`) states that display transformations
must be treated as **display policy**, not as falsification of the observed
Kakoune protocol state. The World Model in `docs/semantics.md` §2.5
formalises this as a dependent-sum decomposition:

```
AppState ≅ Σ_{k : KakouneProtocolFacts} Delta(k)
```

with the projection `p : AppState → KakouneProtocolFacts` and Axioms A2
(Truth Integrity) and A9 (Delta Neutrality) constraining any write path.

Before this ADR, the separation existed only at the **field-attribute
level** (`#[epistemic(observed | derived | heuristic | config | session |
runtime)]` on `AppState` fields). Nothing in the type system prevented a
plugin, a middleware chain, or a non-protocol message handler from writing
through the observed surface, and nothing rejected a Salsa input layout
that lossily dropped observed fields.

Audit findings (pre-ADR-030):

1. `StatusInput` in `salsa_inputs.rs` stored only the derived `status_line`;
   `status_prompt`, `status_content`, and `status_content_cursor_pos`
   (all `#[epistemic(observed)]`) never entered the Salsa world.
2. The `AppView` accessor surface exposed observed, derived, heuristic,
   and config fields through the same method namespace, with no way for a
   plugin to state *"this code path reads only protocol facts."*
3. No property test witnessed A9 (Delta Neutrality) at runtime.

### Decision

Introduce a staged enforcement model for the observed/policy split.
**Level 1** ships now; Levels 2–6 are reserved for follow-on work.

**Level 1 — `Truth<'a>` Projection (shipped).**

- Add `kasane_core::state::Truth<'a>`: a zero-cost newtype wrapping
  `&'a AppState` that exposes **only** accessors for fields carrying
  `#[epistemic(observed)]`.
- `Truth` is `Copy`, has no `&mut` accessors, and has no inherent escape
  hatch. Any write attempt is a compile error (`E0070` / borrow-check
  failure), witnessed by
  `kasane-macros/tests/fail/truth_write_denied.rs`.
- `AppState::truth()` and `AppView::truth()` return the projection.
- A structural test (`state/tests/truth.rs`) pins
  `Truth::ACCESSOR_NAMES` against the macro-generated
  `AppState::FIELDS_BY_CATEGORY["observed"]` set, so adding, removing, or
  reclassifying an observed field forces a corresponding update to
  `Truth`.
- An A9 property test (`kasane-core/tests/delta_neutrality.rs`) witnesses
  that no non-`Msg::Kakoune(..)` message mutates the projection.
- `StatusInput` is extended with `status_prompt`, `status_content`, and
  `status_content_cursor_pos` so that the Salsa projection is no longer
  lossy; `sync_inputs_from_state` is updated accordingly, and a
  regression test (`kasane-core/tests/salsa_projection_coverage.rs`)
  pins the fix.

**Level 2 — `Inference<'a>` / `Policy<'a>` Projections (shipped).**

- Add `kasane_core::state::Inference<'a>`: a zero-cost newtype wrapping
  `&'a AppState` that exposes **only** accessors for fields carrying
  `#[epistemic(derived)]` or `#[epistemic(heuristic)]`. Realises the
  `I` component of the world model `W = (T, I, Π, S)` (§2.5).
- Add `kasane_core::state::Policy<'a>`: the analogous projection over
  `#[epistemic(config)]` fields. Realises the `Π` component. As part
  of this work, `fold_toggle_state` was reclassified from
  `#[epistemic(runtime)]` to `#[epistemic(config)]`, because it is
  user-controlled policy that shapes the DisplayMap, not ephemeral
  runtime state.
- Both projections are `Copy`, have no `&mut` accessors, and pin
  their accessor sets against the macro-generated category map via
  `state/tests/inference.rs` and `state/tests/policy.rs` — mirroring
  the Level 1 `Truth` coverage contract.
- `AppState::inference()` / `AppView::inference()` and
  `AppState::policy()` / `AppView::policy()` return the projections.
- The projection subset of A8 (Inference Boundedness) is witnessed by
  `kasane-core/tests/inference_boundedness.rs`, which proptest-
  mutates session + runtime fields on an `AppState` and asserts that
  Truth / Inference / Policy accessors all return bit-identical
  values. The fully dynamical form of A8 (applying protocol messages
  and re-deriving fields) is still deferred.
- A Level 2 Salsa coverage regression,
  `kasane-core/tests/salsa_projection_coverage_level2.rs`, extends
  the Level 1 invariant: every derived / heuristic / config field
  must either be surfaced through a Salsa input or carry an explicit
  `#[epistemic(..., salsa_opt_out = "<reason>")]` justification. The
  `salsa_opt_out` key is a new universal option on the
  `#[epistemic(...)]` attribute, parsed by `kasane_macros` and
  exposed as a `SALSA_OPT_OUTS` constant on the derived type.
- A small PoC migration of three read sites
  (`render/view/info.rs`, `render/pipeline_salsa.rs`,
  `surface/buffer.rs`) moved from `state.<config>` direct access to
  `state.policy().<config>()`, establishing the pattern without
  undertaking a full rewrite.

**Level 3 — `TransparentCommand` Projection (shipped).**

- Add `Command::is_kakoune_writing()`: exhaustive match (no `_`
  wildcard) classifying every variant as writing or transparent. New
  variants cause a compile error until explicitly classified. Parallel
  refactoring of `is_deferred()` and `is_commutative()` to the same
  exhaustive pattern.
- Add `Command::variant_name()`, `ALL_VARIANT_NAMES`, and
  `KAKOUNE_WRITING_VARIANTS` constants for structural witness tests.
- Add `TransparentCommand`: a newtype wrapping `Command` that exposes
  named constructors only for the 26 non-writing variants. There is no
  constructor for `SendToKakoune`, `InsertText`, or `EditBuffer`,
  making transparency a compile-time property.
- Add `TransparentKeyResult`: transparent variant of `KeyHandleResult`
  whose `Consumed` arm carries `Vec<TransparentCommand>`.
- Add 5 `_transparent` handler registration methods on
  `HandlerRegistry` (`on_key_transparent`, `on_key_middleware_transparent`,
  `on_text_input_transparent`, `on_handle_mouse_transparent`,
  `on_drop_transparent`). Each wraps the handler closure to convert
  `TransparentCommand` → `Command` and sets a transparency flag.
- Add `TransparencyFlags` on `HandlerTable` and
  `HandlerRegistry::is_input_transparent()` for per-plugin T10
  auto-derivation: returns true iff all registered input handlers
  used their `_transparent` variant.
- 8 structural witness tests
  (`kasane-core/src/plugin/tests/command_classification.rs`) pin the
  classification constants and cross-check the three classification
  axes.
- A3 τ-transition property test
  (`kasane-core/tests/a3_transparent_tau.rs`) witnesses that
  non-deferred transparent commands produce zero bytes of Kakoune
  output.
- Note on direct vs transitive writing: `InjectInput` is classified as
  transparent because it re-enters the plugin pipeline rather than
  writing to Kakoune directly. `Session(Switch)` is transparent because
  session switching is a framework-internal operation. A future Level 5
  (free monad) analysis could track transitive writing paths.

**Level 4 — `RecoveryWitness` for Destructive Display Directives (shipped).**

- Add `DisplayDirective::is_destructive()`: exhaustive match (no `_`
  wildcard) classifying every variant as destructive or non-destructive.
  `Hide` is the sole destructive variant. New variants cause a compile
  error until explicitly classified.
- Add `DisplayDirective::variant_name()`, `ALL_VARIANT_NAMES`,
  `DESTRUCTIVE_VARIANTS`, `PRESERVING_VARIANTS`, and
  `ADDITIVE_VARIANTS` constants for structural witness tests.
- Add `SafeDisplayDirective`: a newtype wrapping `DisplayDirective` that
  exposes named constructors only for the 3 non-destructive variants
  (`fold`, `insert_after`, `insert_before`). There is no constructor for
  `Hide`, making non-destructiveness a compile-time property.
- Add `RecoveryWitness` and `RecoveryMechanism`: registration-time
  evidence that a plugin's destructive directives are user-recoverable.
- Add `DisplayRecoveryStatus` and `RecoveryFlags` on `HandlerTable` for
  per-plugin Visual Faithfulness auto-derivation.
- Add 3 display handler registration methods on `HandlerRegistry`:
  `on_display` (unwitnessed — marks plugin as non-faithful),
  `on_display_safe` (compile-time non-destructive via
  `SafeDisplayDirective`), `on_display_witnessed` (destructive with
  recovery evidence).
- Add `HandlerRegistry::is_display_recoverable()` for per-plugin §10.2a
  auto-derivation: returns true unless the plugin registered a raw
  `on_display` handler without recovery evidence.
- 8 structural witness tests
  (`kasane-core/src/plugin/tests/directive_classification.rs`) pin the
  classification constants and cross-check the three classification axes.
- 4 recovery flag auto-derivation tests verify the `NotRegistered`,
  `NonDestructive`, `Witnessed`, and `Unwitnessed` status paths.
- 2 property tests (`kasane-core/tests/visual_faithfulness.rs`) witness
  that `FoldToggleState::toggle` recovers all folded lines in a single
  interaction, confirming Fold's Preserving classification.
- Note: `Fold` is classified as Preserving (not Destructive) because
  `FoldToggleState` provides framework-maintained recovery. `Hide` is
  the sole Destructive variant; plugin-side recovery requires explicit
  `RecoveryWitness` evidence.

**Level 5 — Effect Footprint (implemented).**

Closes §13.15 (lifecycle transparency) and §13.17 (transitive effect analysis).

Phase 5a — `TransparentEffects` + lifecycle transparency:
- `TransparentEffects` newtype wrapping `Effects` but constructible only
  from `TransparentCommand` (same pattern as Level 3). Converts to
  `Effects` before the type erasure boundary in `register_state_effect!`.
- 7 `_transparent` lifecycle registration methods on `HandlerRegistry`:
  `on_init_transparent`, `on_session_ready_transparent`,
  `on_state_changed_transparent`, `on_io_event_transparent`,
  `on_update_transparent`, `on_process_task_transparent`,
  `on_process_task_streaming_transparent`.
- `TransparencyFlags` extended with 5 lifecycle handler fields.
- `is_lifecycle_transparent()` and `is_fully_transparent()` queries.
- Per-task `transparent` flag on `ProcessTaskEntry`.

Phase 5b — `EffectCategory`:
- `EffectCategory` bitflags (14 categories) with exhaustive
  `Command::effect_category()` classification method.
- `CASCADE_TRIGGERS` composite constant: `PLUGIN_MESSAGE | TIMER | INPUT_INJECTION`.
- Theoretical note: the design analysis found that T12's "free monad"
  claim is algebraically a free monoid (list). The correct framework is
  a graded monad `(𝒫(EffectCategory), ∪, ∅)` where each handler
  carries a grade (set of effect categories it may produce).
- The original Phase 5b also shipped a per-plugin `EffectFootprint`
  + `compute_transitive_footprints()` least-fixed-point analysis. That
  artefact was retired (R3.x cleanup) after a workspace grep confirmed
  zero non-test consumers: the math was correct but no dispatcher ever
  read the result. The conservative `EffectCategory` classification
  remains the source of truth for transparency tier checks.

**Level 6 — Type-level `&mut AppState` Ownership (shipped).**

- Decompose `AppState` into 5 epistemic sub-structs: `ObservedState`,
  `InferenceState`, `ConfigState`, `SessionState`, `RuntimeState`. Each
  sub-struct owns the fields of its epistemic category, and `AppState`
  composes them.
- Extract `apply_protocol()` as a free function that takes `&mut ObservedState`
  + `&mut InferenceState` + `&ConfigState` (immutable). Config mutation from
  the protocol ingestion path is now a compile error, turning the A2/A9
  invariants from convention into compiler-checked properties.
- Update `Truth<'a>`, `Inference<'a>`, and `Policy<'a>` projections to wrap
  the corresponding sub-structs directly, preserving zero-cost projection
  semantics while eliminating redundant accessor generation.

### Implications

- Plugins and framework code can now mark observation sites with
  `state.truth()` to statically prove they only consult protocol facts,
  even where `AppView` would otherwise allow wider reads.
- Adding a new `#[epistemic(observed)]` field to `AppState` is a
  compile-or-test failure until `Truth` is updated, preventing silent
  gaps in the projection.
- The Salsa layer is no longer a lossy projection of observed state,
  unblocking future Salsa views that need to distinguish status-prompt
  from status-content.
- As of Level 6, the protocol ingestion path receives `&ConfigState`
  (immutable), making config mutation from protocol handling a compile
  error. The `&mut AppState` surface remains available for non-protocol
  paths (plugin lifecycle, user commands) where broader mutation is
  intentional.
