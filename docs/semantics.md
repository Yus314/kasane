# Kasane Semantics

This document is the authoritative reference for Kasane's current semantics and correctness conditions.
What is defined here is "what Kasane means." Benchmark values, implementation progress, upstream issue tracking, and API signature listings are out of scope.

## 1. Document Responsibilities

### 1.1 What This Document Defines

- Kasane's system boundaries
- The meaning of state, update, rendering, and invalidation
- Plugin composition and Surface/Workspace semantics
- WASM plugin constraints and state access model
- Formal correctness theorems for optimization paths
- Currently known theoretical gaps

### 1.2 What This Document Does Not Define

- Benchmark values or performance measurement listings
- History of when features were implemented
- User-facing configuration details
- Complete plugin API reference
- Detailed design of future proposals

### 1.3 Related Documents

- [requirements.md](./requirements.md): Authoritative reference for requirements
- [index.md](./index.md): Documentation entry point and architecture overview
- [plugin-development.md](./plugin-development.md): Guide for plugin authors
- [performance.md](./performance.md): Performance principles, benchmarks, and optimization status
- [decisions.md](./decisions.md): History of design decisions
- [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md): Analysis of upstream protocol constraints

## 2. Fundamental Model

### 2.1 System Boundaries

Kasane is a JSON UI frontend for Kakoune. Kakoune sends drawing commands and UI state as JSON-RPC messages, and Kasane reflects them into `AppState`, rendering through a declarative UI and backend.

Kasane is not a general-purpose UI framework. It is designed to be tightly coupled to Kakoune's JSON UI protocol.

### 2.2 Division of Responsibilities Between Kakoune and Kasane

Kakoune manages the editing model, buffer contents, selections, triggering of menus and info popups, and the protocol truth.
Kasane manages how to display those through a declarative UI and backend implementation, how to perform plugin composition, and how to handle frontend-native capabilities not expressed by the protocol.

Kasane's core is responsible for "what to display and where," while the backend is responsible for "how to draw it."

### 2.3 Resolution Layer (HOW) and Responsibility Layer (WHERE)

Kasane classifies functionality along two axes.

- Resolution Layer (HOW)
  - Renderer
  - Configuration
  - Infrastructure
  - Protocol constraints
- Responsibility Layer (WHERE)
  - Upstream (Kakoune)
  - Core (`kasane-core`)
  - Plugins

The resolution layer represents "which mechanism resolves it," and the responsibility layer represents "which layer is responsible." The two are independent and must not be conflated.

### 2.4 Default Frontend Semantics and Extended Frontend Semantics

Kasane has a two-tier semantics.

- Default Frontend Semantics
  - The semantics when Kasane acts as an alternative frontend to `kak` for general users
  - Prioritizes conservatively displaying Kakoune's protocol truth and maintaining compatibility with existing configurations, plugins, and workflows
- Extended Frontend Semantics
  - The semantics when display structure, interaction policy, and surface composition are significantly reconfigured through plugins or explicit display policies
  - Builds additionally upon Default Frontend Semantics as its foundation

Kasane's primary purpose as a product lies in Default Frontend Semantics. Extended Frontend Semantics is a capability of Kasane and an important goal, but it is not a precondition for overriding the standard semantics for ordinary users.

### 2.5 World Model

The projection-centered theory models Kasane's semantics through a World tuple W = (T, I, Π, S):

- **Truth (T)**: Protocol facts received from Kakoune. These are the `#[epistemic(observed)]` fields in `AppState` (§3.2). Truth is the sole authoritative source for what Kakoune intends to display.
- **Inference (I)**: Values derived or estimated from Truth. This includes `#[epistemic(derived)]` fields (§3.3) and `#[epistemic(heuristic)]` fields (§3.4). Inference carries declared strength: derived values are deterministic; heuristic values may degrade gracefully.
- **Policy (Π)**: Display policies, configuration, and plugin-contributed presentation decisions. This includes Display Policy State (§3.6), `#[epistemic(config)]` fields, and all plugin extension point outputs (§9).
- **Scope (S)**: The spatial and lifecycle context: which session is active, which surface is focused, which region of the buffer is visible. Scope determines which subset of W is relevant for a given frame.

The World model is a theoretical framing of the existing `AppState` structure. It does not introduce new runtime state or new types. Every component of W maps directly to existing fields and subsystems documented in §3–§11.

Formally, AppState realizes a dependent sum

```text
AppState ≅ Σ_{k : KakouneProtocolFacts} Delta(k)
```

where `KakouneProtocolFacts` is the subset of `AppState` expressible by Kakoune's JSON-RPC protocol (the `#[epistemic(observed)]` fields of §3.2) and `Delta(k)` is the Kasane-internal extension over a given Kakoune state: runtime state (§3.5), display policy state (§3.6), derived caches (§3.3), heuristic estimates (§3.4), and plugin-contributed presentation (§9).

The projection

```text
p : AppState → KakouneProtocolFacts
p(s) = extract_observed(s)
```

is a left-inverse of the canonical embedding of a bare Kakoune state into `AppState`. The fiber `p⁻¹(k) = {k} × Delta(k)` is the space of Kasane-side configurations compatible with the Kakoune state `k`.

This factorisation is what makes Kasane's frontend-native capabilities (overlay, display transformation, multi-surface layout, per-pane status bars) coexist with the "alias kak=kasane" substitutability goal (§2.4, A3): mutation confined to a single fiber is guaranteed Kakoune-invisible by Axiom A9 (§2.7).

### 2.6 Projection Function

The central theoretical construct is a projection function P that maps the World to a Presentation:

```text
P(T, I, Π, S) = Ω
```

The projection is decomposed into two stages:

- **Logical projection (Ω_L)**: The Element tree produced by `view()` after plugin contributions and transforms. This is the abstract UI structure independent of any backend.
- **Physical projection (Ω_P)**: The positioned content produced by `layout()` from Ω_L. This is the concrete placement of elements within a rectangular viewport.

The final frame is produced by applying a backend-specific renderer R:

```text
Frame = R(Ω_P, backend)
```

For input, the inverse projection maps events back to intents:

```text
Intent = ρ(Ω_P, event)
```

where ρ uses the previous frame's Ω_P (specifically, the DisplayMap and HitMap) to translate screen coordinates to buffer coordinates and route interactions.

Whether P is computed by Salsa memoization or direct evaluation is an implementation choice. Theorem T3 (§12.3) guarantees equivalence between these paths.

The intermediate type `Element` is the initial algebra of a polynomial endofunctor P on the category of sets. Informally,

```text
P(X) = Text × Style
     + Vec<Atom>                                     -- StyledLine
     + (SlotName × Direction × Gap)                  -- SlotPlaceholder
     + (Direction × Vec<(X, Flex)> × Gap × Align²)   -- Flex / ResolvedSlot
     + (X × Vec<(X × Anchor)>)                       -- Stack
     + (X × Offset × Direction)                      -- Scrollable
     + (X × BorderOpt × ShadowBool × Edges × Style × TitleOpt) -- Container
     + (X × InteractiveId)                           -- Interactive
     + (Vec<GridColumn> × Vec<X> × Gap² × Align²)    -- Grid
     + 1                                             -- Empty
     + (ImageSource × (u16, u16) × ImageFit × f32)   -- Image
     + BufferRef                                     -- leaf reading from state
```

```text
Element ≅ μX. P(X)
```

`Element` thus carries the standard universal properties of an initial P-algebra: unique catamorphism (fold), naturality of `view()` with respect to P, and the characterisation of plugin transforms as P-algebra morphisms on subterms designated by `TransformTarget` (§9.5). Theorem T11 (§12.11) records this structure. The concrete shape of P is tracked directly from the `Element` enum defined in `kasane-core/src/element.rs`; any variant added there must be reflected in P.

### 2.7 Axioms

The following axioms constrain the projection function and its implementation. Each axiom references the code or mechanism that enforces it.

**A1 (Determinism)**: For identical World state, P produces identical Ω. Two calls to `render_pipeline(S)` with the same S yield byte-identical CellGrid output. Verified by `trace_equivalence.rs` property tests.

**A2 (Truth Integrity)**: Observed state is stored exactly as received from Kakoune. No transformation, filtering, or policy is applied to `#[epistemic(observed)]` fields during `apply()`. Heuristic and derived fields are clearly separated by `#[epistemic]` compile-time annotations, and synthetic content (from display transformations) carries `SourceMapping::None` to prevent confusion with buffer content.

Equivalently, any internal state transition that does not emit `Command::SendToKakoune`, `Command::InsertText`, or `Command::EditBuffer` (the three Command variants whose handlers write to the Kakoune byte stream; see `plugin/command.rs::execute_commands`) leaves the projection `p : AppState → KakouneProtocolFacts` (§2.5) invariant. Display-only operations, cache refreshes, plugin tick, overlay recomputation, and layout resolution are in this class and are therefore Kakoune-transparent by construction (cf. A9, T10).

**A3 (Behavioral Equivalence)**: Under Default Frontend Semantics (§2.4), Kasane is behaviorally equivalent to `kak -ui ncurses` for the same Kakoune session. This is the "alias kak=kasane" goal. Behavioral equivalence holds only under D-semantics; Extended Frontend Semantics may intentionally diverge.

Formally, there exists a weak bisimulation relation `R ⊆ S_kak × S_kas` between Kakoune's labelled transition system (`S_kak` with observable actions: key receipt, protocol event emission, option change) and Kasane's labelled transition system (`S_kas`) such that:

- **(Sim-Fwd)** If `(k, s) ∈ R` and `k →^a k'` for an externally observable action `a`, then there exist Kasane states `s₀, s₁, ..., s_n = s'` with `s →^τ* s₀ →^a s₁ →^τ* s'` and `(k', s') ∈ R`.
- **(Sim-Bwd)** If `(k, s) ∈ R` and `s →_kas s'` emits a Kakoune-writing Command variant (see A2), the corresponding Kakoune transition `k →^a k'` exists and `(k', s') ∈ R`.
- **(Init)** For initial states: `(k_0, s_0) ∈ R` when Kasane starts against a fresh Kakoune session.

The `τ`-transitions in `S_kas` include: rendering pipeline phases (view/layout/paint), Salsa cache (re)computation, plugin state evolution that emits no Kakoune-writing Command, display map recomputation, and hit map rebuild. None of these are observable by Kakoune. The weak bisimulation formalism allows Kasane's τ-transitions to be freely interleaved with externally observable actions without breaking A3. Theorem T8 (§12.8) records this property; Theorem T9 (§12.9) records the τ-invariance of `p : AppState → KakouneProtocolFacts`. Property tests in `kasane-core/tests/trace_equivalence.rs` provide empirical evidence for `R`'s existence under proptest-generated state mutations.

**A4 (Display Coherence)**: The DisplayMap maintains bidirectional consistency between buffer lines and display lines. Forward-backward consistency (INV-1), backward-forward consistency (INV-2), monotonicity (INV-5), and all other structural invariants (INV-1 through INV-7) are verified by `DisplayMap::check_invariants()` in debug builds and by `assert_display_map_invariants()` in tests.

**A5 (Frame Isolation)**: During the render phase, all plugin view methods operate on a frozen `PluginView<'_>` (immutable borrow). No plugin can observe state changes made by other plugins within the same render phase. This is enforced statically by Rust's borrow checker through the mutable/immutable phase split (§4.6).

**A6 (Input Coherence)**: Mouse coordinate translation uses the DisplayMap from the previous frame to map display-space coordinates to buffer-space. Lines with `InteractionPolicy::ReadOnly` or `Skip` suppress the event. The DisplayMap is persisted on `AppState.display_map` after each render and consumed by `mouse_to_kakoune()` during the next frame's event processing. Staleness is bounded to one frame (~16ms).

**A7 (Plugin Boundary)**: Plugins affect presentation (Π) but not truth (T). Plugin effects flow through `Command::SendToKakoune`, not direct mutation of observed state (§9.9). The rendering pipeline reads plugin contributions through `PluginView<'_>` which provides read-only access.

**A8 (Inference Boundedness)**: Every heuristic inference carries a declared severity via `#[epistemic(heuristic, rule="...", severity="...")]`. When a heuristic fails (e.g., cursor detection under unexpected Kakoune face patterns), the degradation is bounded to the declared severity level. The catalog of inference rules is maintained in `derived/mod.rs`.

**A9 (Delta Neutrality)**: `AppState` factors through the fibration `p : AppState → KakouneProtocolFacts` of §2.5. Every state transition `s →_kas s'` that leaves the fibre unchanged (`p(s) = p(s')`) emits no Kakoune-writing Command:

```text
∀ s, s' ∈ AppState.
  (s →_kas s' ∧ p(s) = p(s'))
    ⟹ the transition generates no Command::SendToKakoune,
       Command::InsertText, or Command::EditBuffer
```

A9 is the fibration-level restatement of the A2 sharpening above and the structural basis of A7 (Plugin Boundary): plugin effects on `Delta(k)` propagate freely inside a fibre but cannot cross `p` without an explicit Kakoune-writing Command. Theorem T9 (§12.9) records the converse direction (τ-transitions preserve `p`). A9 is currently enforced by code review — see known gap §13.13 for the absence of a compile-time check.

## 3. State Semantics

### 3.1 Role of AppState

`AppState` is a single state space that holds facts observable from Kakoune, values derived from them, values estimated through heuristics, and frontend runtime state.

`AppState` does not treat "everything as the same kind of truth." Each field has a different epistemological strength. In the projection-centered theory (§2.5), AppState is the concrete realization of World W = (T, I, Π, S). The epistemic annotations on each field classify it into one of the four World components.

### 3.2 Observed State

Observed State is information explicitly communicated by Kakoune's protocol. These are Kasane's first-class facts and must not be altered by Kasane-side policy.

Examples:

- Buffer lines received via `draw`
- `draw.cursor_pos`
- `menu_show` / `menu_hide`
- `info_show` / `info_hide`
- `draw_status` and `draw_status.cursor_pos`

### 3.3 Derived State

Derived State is information that can be deterministically recomputed from Observed State. Derived State may be held for caching or convenience purposes, but semantically it is uniquely determined from Observed State.

Examples:

- Layout results
- Contents of various caches
- Per-section rendering data

### 3.4 Heuristic State

Heuristic State is information estimated from patterns in display data that Kakoune does not explicitly provide. It exists for convenience, but its accuracy is not guaranteed by the upstream protocol.

Examples:

- Cursor count estimation via `FINAL_FG + REVERSE`
- Cursor style estimation from mode line strings
- Info identity estimation

Heuristic State does not carry the same strength of truth as Observed State. Fallback behavior and non-goals for heuristic failures must be explicitly stated.

### 3.5 Runtime State

Runtime State is state that exists only during frontend execution. It includes backend caches, animations, focus, plugin internal state, and so on.

Runtime State must not override Kakoune's truth, but it is held to inform rendering and input handling strategies.

In the fibration of §2.5, Runtime State together with Display Policy State (§3.6), Derived State (§3.3), and Heuristic State (§3.4) constitutes the Kasane-internal extension `Delta(k)` over a Kakoune state `k`. By A9 (Delta Neutrality), updates restricted to these Kasane-internal categories are guaranteed Kakoune-invisible.

### 3.6 Display Policy State

Display Policy State is the frontend-side policy that determines how Observed State is projected into display. It includes overlay visibility policies, display transformations, proxy displays, display unit grouping, and reconfiguration rules introduced by plugins.

Display Policy State is not Observed State itself. Kasane may use it to omit, proxy-display, supplement, or reconfigure Observed State, but must not treat the result as "facts stated by Kakoune."

In Default Frontend Semantics, Display Policy State is in principle Observed-preserving. That is, Kasane's standard behavior preserves the visible structure of protocol truth while improving placement, decoration, supplementary display, and overlay. Observed-eliding transformations and large-scale reconfiguration belong to Extended Frontend Semantics and are introduced through explicit policy or plugins.

### 3.7 Principles of State Updates

Input from external sources is in principle processed through the following flow.

1. Receive protocol or frontend input
2. Update `AppState`
3. Generate `DirtyFlags`
4. Notify plugins and the rendering pipeline

State is updated before rendering. Rendering is always a function of state, and rendering results must not generate state truth.

### 3.8 Treatment of Heuristics

Heuristics follow these principles.

- Do not override protocol facts
- Accept explicit degraded mode on failure
- Separate problems that should be resolved upstream as upstream dependencies
- Features derived from heuristics may weaken their exactness targets

In Default Frontend Semantics, heuristic failure should be treated as graceful degradation rather than UI collapse. Even when heuristics do not hold, Kasane prioritizes maintaining its meaning as a core frontend, with only extended features degrading.

### 3.9 Compile-Time Enforcement

Epistemological categories are enforced at compile time via `#[epistemic(...)]` attributes on `AppState` fields. Every field must carry exactly one classification. The `DirtyTracked` derive macro validates completeness and generates constants (`FIELD_EPISTEMIC_MAP`, `HEURISTIC_FIELDS`, `DERIVED_FIELDS`, `FIELDS_BY_CATEGORY`) for test validation.

The `Truth<'a>` projection (`kasane-core/src/state/truth.rs`, ADR-030 Level 1) consumes `FIELDS_BY_CATEGORY["observed"]` to expose a write-denying, observed-only view of `AppState`. A structural test pins the `Truth` accessor set against the generated constant, so adding or reclassifying an observed field forces the projection to be updated. See §13.13 for the enforcement status and ADR-030 for the staged rollout plan.

The `Inference<'a>` projection (`kasane-core/src/state/inference.rs`, ADR-030 Level 2) realises the `I` component of `W = (T, I, Π, S)` as a read-only view over the union of `FIELDS_BY_CATEGORY["derived"]` and `FIELDS_BY_CATEGORY["heuristic"]`. The `Policy<'a>` projection (`kasane-core/src/state/policy.rs`, same ADR, same level) realises the `Π` component as a read-only view over `FIELDS_BY_CATEGORY["config"]`. Both mirror `Truth<'a>`: `Copy`, lifetime-parameterised, no `&mut` accessors, and backed by structural coverage tests in `state/tests/inference.rs` and `state/tests/policy.rs`. Plugins reach all three projections through `AppView::truth()` / `inference()` / `policy()`.

Axiom A8 (Inference Boundedness, §2.4) is witnessed at the projection layer by `kasane-core/tests/inference_boundedness.rs`, which asserts that mutating any `session` or `runtime` field on an `AppState` leaves all three projections bit-identical. A fully dynamical witness — applying protocol messages and verifying derivation outputs — remains tracked separately under §13 and ADR-030.

In addition, `kasane-core/tests/salsa_projection_coverage_level2.rs` enforces that every `derived`/`heuristic`/`config` field is either surfaced through a Salsa input in `salsa_inputs.rs` or carries an explicit `#[epistemic(..., salsa_opt_out = "<reason>")]` justification. This closes the gap where a new policy knob could silently bypass Salsa's revision tracking.

## 4. Update Semantics

### 4.1 From External Input to State Update

Kasane's update system receives protocol input from Kakoune and key/mouse/focus inputs from the frontend, converting them into state updates and command sequences.

The basic flow is as follows.

1. Receive a message from Kakoune
2. Update `AppState` via `state.apply()` and compute dirty flags
3. If necessary, perform additional state transitions and `Command` generation via `update()`
4. Perform plugin notification and rendering based on dirty flags

### 4.2 Role of TEA Update

Kasane adopts TEA as its runtime model. `update()` aggregates inputs and centralizes state transitions and side-effect instructions.

The semantic benefits of TEA are as follows.

- Clear entry point for state transitions
- Makes it easier to keep `view` as a pure function of state
- Aligns well with Rust's ownership model
- Enables testable state transition units

### 4.3 Meaning of Command

`Command` is not a side effect itself but a description of a side-effect request. `Command` is not generated from view; it is generated from the update system or plugin hooks.

Commands fall into the following categories.

- Protocol commands: `SendToKakoune` (key forwarding, command execution), `EditBuffer` (structured buffer edits translated to key sequences)
- Frontend commands: `Paste`, `Quit`, `RequestRedraw(DirtyFlags)`, `InjectInput` (synthetic input re-dispatch with depth guard)
- Timer and scheduling: `ScheduleTimer`
- Plugin communication: `PluginMessage` (inter-plugin messaging)
- Configuration: `SetConfig`, `RegisterThemeTokens`
- Process management: `SpawnProcess`, `WriteToProcess`, `CloseProcessStdin`, `KillProcess`, `ResizePty`
- Surface management: `RegisterSurface`, `UnregisterSurface`, `RegisterSurfaceRequested`, `UnregisterSurfaceKey`
- Structural commands: `Session(SessionCommand)`, `Workspace(WorkspaceCommand)`, `SpawnPaneClient`, `ClosePaneClient`

The runtime receives Commands and executes them as side effects. The important invariant is that Command generation is deterministic given the same state and input, even though Command execution may involve I/O.

Structurally, the `Command` enum is an effect signature `CommandSig`, and the output of a single frame's update cycle is a value of the free monad `Free(CommandSig)` (concretely, `Vec<Command>` together with the ordering induced by update semantics). The runtime interpreter in `event_loop/dispatch.rs` and `plugin/command.rs::execute_commands` plays the role of an algebraic-effect handler: it maps `Free(CommandSig)` into actual I/O, process management, and protocol traffic.

This framing clarifies two invariants that are otherwise stated only prose:

- **(E1) Purity of generation.** `update()` is a pure function `AppState × Msg → AppState × Free(CommandSig)`. All impurity is confined to the handler. Theorem T1 (Presentation Equivalence) and Theorem T4 (Composition Determinism) rest on (E1).
- **(E2) Compositional sequencing.** Command sequences compose by free-monad bind. A multi-step workflow (plugin message → Kakoune forward → timer reschedule) is a single value of `Free(CommandSig)`, not three effectful statements, so composition of plugins composes effect trees rather than interleaving side effects.

Kakoune-writing Commands (`SendToKakoune`, `InsertText`, `EditBuffer`) form the distinguished subset of `CommandSig` that drives externally observable transitions in the bisimulation of A3. The remaining variants are internal effects whose handler interpretation stays within Kasane's fibre `Delta(k)` (§2.5) and therefore preserves `p` by A9. See Theorem T12 (§12.12).

### 4.4 Generation of DirtyFlags

`DirtyFlags` is a coarse-grained change set representing "which observable aspects have changed." `DirtyFlags` serves as input for cache invalidation and selective redraw, not as a complete proof of state differences.

The important point is that `DirtyFlags` represents "what kind of information has changed," not "the detailed content of the change."

### 4.5 Semantic Split of Buffer Flags

`DirtyFlags` splits buffer-related changes into two independent flags.

- `BUFFER_CONTENT`: Buffer lines, faces, and structural changes received via `draw`
- `BUFFER_CURSOR`: Cursor position, cursor mode, and secondary cursor coordinates

This split is a semantic design decision, not merely an optimization. It encodes the invariant that cursor movement alone does not change the meaning of the buffer body. Consequently, the Salsa input separation places buffer content in `BufferInput` and cursor state in `CursorInput`, enabling cursor-only changes to skip base section re-evaluation.

The composite flag `BUFFER` is defined as `BUFFER_CONTENT | BUFFER_CURSOR` for convenience.

### 4.6 Frame Structure and Phase Ordering

A frame is one iteration of the backend event loop. Each frame processes input and optionally renders. The phases execute in strict order:

1. **Event batch**: Drain channel up to 256 events or 16ms deadline, whichever comes first. Each event is processed sequentially via `update()`, accumulating `DirtyFlags` via bitwise OR.
2. **Plugin cache validation**: Compare each plugin's generation counter against the previous frame to determine whether any plugin state changed.
3. **Input synchronization**: Unconditionally project all `AppState` fields into Salsa inputs (PartialEq early-cutoff). Refresh Salsa-tracked extension point data (contributions, display directives) when plugin state has changed.
4. **Render and present**: Execute the rendering pipeline (Salsa demand-driven), present output to the backend, and refresh the hit map for the next frame's input routing.

If dirty flags are empty after the batch phase, phases 2–4 are skipped entirely.

```text
Invariant (Intra-Frame Plugin Isolation):
  During the render phase, plugin view methods (contribute_to, transform,
  annotate_line, contribute_overlay) operate on a frozen PluginView<'_>
  (immutable borrow). No plugin can observe state changes made by other
  plugins within the same render phase. Inter-plugin state effects
  propagate only via the next frame's event processing.
```

This invariant is the structural enforcement of Axiom A5 (§2.7). The borrow checker guarantees that the immutable and mutable phases never overlap.

The plugin system enforces a two-phase lifecycle per frame:

- **Mutable phase**: event processing, state transitions (`&mut PluginRuntime`)
- **Immutable phase**: rendering, view queries (`PluginView<'_>`)

This boundary is enforced at compile time by Rust's borrow checker. The two phases never overlap within a frame.

## 5. Rendering Semantics

### 5.1 Projection Stages

The rendering pipeline implements the projection function P (§2.6) in two stages:

- **Logical projection (P_L)**: `view(S, registry)` constructs the Element tree Ω_L from state and plugin contributions. This includes slot fills, annotations, overlays, transforms, and display map computation.
- **Physical projection (ρ)**: `layout(Ω_L)` followed by `paint(Ω_L, layout)` produces the positioned content Ω_P, then converts it to backend-specific output (CellGrid for TUI, DrawCommand sequence for GPU).

Multiple rendering implementations exist for testing and optimization purposes. All implementations must produce observably identical output for the same state (Theorem T1, §12.1). See `render/pipeline_salsa.rs` for implementation details.

```text
P_L(S, registry) = view(S, registry)           → Ω_L (Element tree)
ρ(Ω_L, rect)     = layout(Ω_L, rect) + paint   → Ω_P (CellGrid / DrawCommands)
```

### 5.2 Incremental Evaluation

Incremental evaluation describes the practical rendering produced by Salsa-based memoization. It is the meaning of the output when memoization and early-cutoff may skip recomputation of unchanged subgraphs.

In the current implementation, Exact Semantics and incremental evaluation coincide. Salsa's automatic dependency tracking ensures that cached rendering produces the same result as complete re-rendering — there is no intentional staleness.

A future optimization (e.g., removing `no_eq` from Salsa tracked functions to enable output-level early-cutoff, which is feasible since `Element` already implements `PartialEq`) could reintroduce a gap between the two tiers. If that happens, the distinction will be re-specified here.

In Default Frontend Semantics, any future policy-permitted staleness must remain within the range that does not break "the meaning existing users expect from a `kak` replacement." Staleness tolerance may exist for the freedom of plugin-defined extensions, but it must not take priority over the semantic consistency of the core frontend.

### 5.3 Separation of Responsibilities: view, layout, paint

- `view`: Constructs a declarative `Element` tree from state
- `layout`: Computes rectangular placement from `Element` and constraints
- `paint`: Converts `Element` and layout results into a representation for the drawing backend

In TUI, the output of `paint` is `CellGrid`; in GUI, it is a sequence of `DrawCommand`. Differences exist per backend, but both share the same UI semantics.

The pipeline composes functorially:

```text
AppState ──view──▶ Element ──layout──▶ LayoutTree ──paint──▶ CellGrid / DrawCommand
```

Each arrow is a pure function (possibly Salsa-memoized). `Element` is the polynomial-functor initial algebra of §2.6; `LayoutTree` and the paint target are less structured data types. The separation is not merely aesthetic: Theorem T2 (Backend Equivalence, §12.2) is the statement that different paint implementations compute observably equivalent output, and Theorem T3 (Incremental Equivalence, §12.3) is the statement that Salsa memoisation is natural with respect to this composition.

### 5.4 Common Semantics Between TUI and GUI

TUI and GUI differ in output representation.

- TUI: Diffs `CellGrid` and converts to terminal I/O
- GUI: Converts scene descriptions to GPU drawing

However, both are required to display the same UI structure and the same semantic content for the same state. The backend's freedom is limited to "how to draw it."

One intentional exception is `Element::Image`: the GPU backend renders raster images natively, while the TUI backend renders a low-resolution approximation using Unicode halfblock characters (`▀`, U+2580), where each cell represents two pixel rows (fg = top, bg = bottom). If the `tui-image` feature is disabled or image decoding fails, the TUI falls back to a text placeholder (e.g., `[IMAGE: filename]`). The semantic content (presence and position of the image element) is identical; only the visual fidelity differs.

### 5.5 What Constitutes an Observable Result

Kasane's observational equivalence is defined not by the state of internal caches but by the finally observable rendering result.

Examples of observable targets:

- Displayed text
- Faces and styles
- Display positions
- Presence and placement of overlays/menus/info popups
- Cursor display

### 5.6 Rendering Faithfulness

Under Default Frontend Semantics, every element of Observed State must appear in the final rendering output unless explicitly elided by Display Policy State.

```text
Invariant (Rendering Faithfulness):
  For all observable elements e in Observed State S:
    e is visible in render(S)
    ∨ e is transformed by an active Display Policy in a manner
      classified as Additive, Transforming, or Preserving (§10.2),
      such that e's semantic content remains visually recoverable
      (§10.2a)
    ∨ e is elided by an active Destructive Display Policy whose
      recovery interaction is advertised to the user
```

This invariant does not apply to Extended Frontend Semantics, where unrecoverable elision is permitted under explicit user consent. The Visual Faithfulness condition (§10.2a) makes the "recoverable" qualifier precise.

### 5.7 Diff and Incremental Drawing

In TUI, the output of the rendering pipeline is not drawn in full each frame. Instead, `TuiBackend` maintains a previous frame buffer and diffs against the current `CellGrid`.

1. `paint` writes into the current grid (with row-level dirty tracking)
2. `backend.present()` diffs dirty rows against the previous buffer, emitting terminal I/O only for changed cells
3. `present()` copies dirty rows into the previous buffer and clears dirty flags

On terminal resize, `backend.invalidate()` clears the previous buffer, forcing a full redraw on the next `present()` call.

## 6. Input Semantics

### 6.1 Input Routing Model

Input events are routed through two paths:

- **Key events**: Processed through the plugin key middleware chain (`dispatch_key_middleware`). If no plugin consumes the key, it is forwarded to Kakoune via `Command::SendToKakoune`. See §9.7 for dispatch semantics.
- **Mouse events**: First routed through the HitMap for plugin-specific interactive elements. If no plugin handles the event, the mouse coordinates are translated to buffer-space and forwarded to Kakoune.

### 6.2 Mouse Coordinate Translation

When a DisplayMap is active (non-identity), mouse events require coordinate translation from display-space to buffer-space. The function `mouse_to_kakoune()` performs this translation:

1. The screen line coordinate is offset by `display_scroll_offset` to obtain the display line index.
2. The display line's `InteractionPolicy` is checked:
   - `Normal`: proceed with translation
   - `ReadOnly` or `Skip`: suppress the event (return `None`)
3. `display_to_buffer()` maps the display line to the corresponding buffer line.
4. The translated (buffer_line, column) pair is sent to Kakoune.

Without a DisplayMap (identity mapping), the screen line is used directly with the scroll offset applied.

### 6.3 Input Coherence Invariant

Mouse coordinate translation uses the DisplayMap from the **previous** frame. This is Axiom A6 (§2.7):

```text
Invariant (Input Coherence):
  For any mouse event E at screen position (x, y):
    mouse_to_kakoune(E, display_map_prev, scroll_offset_prev)
  uses the DisplayMap and scroll offset persisted on AppState
  after the most recent render. The DisplayMap is set by the
  rendering pipeline via AppState.display_map.
```

The DisplayMap is persisted on `AppState.display_map` after each render frame. Before the first render, `display_map` is `None`, and mouse events use identity mapping (no translation).

### 6.4 Frame Delay

Both the HitMap (§13.11) and the DisplayMap operate on one-frame-delayed data. Input events within a batch are processed using the previous frame's spatial information. This introduces at most ~16ms of stale routing, which is an accepted tradeoff: the cost of mid-batch spatial recomputation outweighs the marginal correctness improvement.

## 7. Invalidation and Caching

### 7.1 Meaning of DirtyFlags

`DirtyFlags` is the input for state dependency tracking and cache invalidation. It does not represent the full diff of the entire state but rather an approximation of which observable aspects require recomputation.

### 7.2 Section-Level Invalidation

The current core view is primarily divided into `base`, `menu`, and `info` sections. Salsa input granularity (`BufferInput`, `StatusInput`, `MenuInput`, `InfoInput`, etc.) provides natural section-level isolation — changes to one input struct do not trigger re-evaluation of tracked functions that depend only on other inputs.

This design means that a menu change does not always require rebuilding the buffer body.

### 7.3 ViewCache

> Removed. Replaced by Salsa incremental computation (ADR-020).

### 7.4 SceneCache

`SceneCache` holds `DrawCommand` sequences per section for the GUI backend. Like `ViewCache`, it has an invalidation mask, but it is used for GUI-specific fast paths.

### 7.5 PaintPatch

> Removed. Replaced by Salsa incremental computation (ADR-020).

### 7.6 LayoutCache

> Removed. Replaced by Salsa incremental computation (ADR-020).

### 7.7 Meaning of `stable()`

> Removed. `stable()` and the `#[kasane::component(deps(...))]` macro were removed when Salsa replaced manual dependency tracking (ADR-020). Exact Semantics and incremental evaluation now coincide.

### 7.8 Meaning of `allow()`

> Removed with `#[kasane::component(deps(...))]` (ADR-020).

### 7.9 Locations Where Exactness Is Intentionally Weakened

> Removed. No intentional exactness weakening exists in the current system (ADR-020).

### 7.10 ComponentCache

> Removed. Replaced by Salsa incremental computation (ADR-020).

### 7.11 Salsa Incremental Computation

Salsa 0.26 is the sole caching layer for Element tree construction and the rendering pipeline.

**Input projection.** Each frame, application state is unconditionally projected into Salsa input structs. Salsa's built-in PartialEq-based change detection automatically skips re-evaluation of unchanged inputs. See `salsa_sync.rs` for implementation.

**Input granularity.** Salsa inputs are split into fine-grained structs providing section-level isolation: buffer content, cursor state, status line, menu, info popups, configuration, and plugin outputs (contributions, annotations, overlays, display directives). See `salsa_inputs.rs` for the full list.

**Memoization level.** Input-level memoization is enabled: if all inputs to a rendering function are unchanged, the function is not re-executed. Output-level memoization is disabled because no downstream tracked functions currently depend on rendering outputs. `Element` implements `PartialEq`, so output-level cutoff is technically feasible if the pipeline is deepened.

**DirtyFlags role.** `DirtyFlags` serves as a semantic classifier of protocol messages, not as a cache invalidation driver. It provides hints about which inputs to update and gates optimizations like line-level dirty tracking. Cache invalidation itself is handled automatically by Salsa's `PartialEq`-based change detection.

**Plugin change detection (dual structure).** Plugin state changes are tracked by two complementary tiers:

- **Tier 1 (coarse)**: Plugin state is compared via `PartialEq` after each mutable hook. On change, a monotonic generation counter is incremented. The runtime reads generation counters to determine whether any plugin state changed.
- **Tier 2 (fine)**: When the coarse tier indicates a change, plugin outputs are re-collected. Individual contribution inputs use `PartialEq` early-cutoff — unchanged contributions produce cached outputs even when re-collected.

Both tiers are necessary: the generation counter provides the coarse "did anything change?" gate; Salsa provides fine-grained memoization.

## 8. Dependency Tracking Semantics

### 8.1 Contract of `#[kasane::component(deps(...))]`

> Removed. `#[kasane::component(deps(...))]` was replaced by Salsa automatic dependency tracking (ADR-020).

### 8.2 Guarantees of AST-Based Verification

> Removed with the component macro's AST-based verification (ADR-020).

### 8.3 Role of Hand-Written Dependency Information

> Removed. Hand-written dependency tables were eliminated by Salsa (ADR-020).

### 8.4 Limits of Soundness

Salsa provides automatic dependency tracking for native rendering paths, but two limitations remain:

- **WASM `state_hash()` is manual.** WASM plugins implement `state_hash() → u64` by hand. An incorrect hash may cause stale contributions to persist without detection (see §9.12.4).
- **`no_eq` on tracked functions.** Salsa tracked functions use `#[salsa::tracked(no_eq)]`, disabling output-level early-cutoff. `Element` implements `PartialEq`, but removing `no_eq` would add comparison cost without benefit because no downstream tracked functions depend on these outputs.

## 9. Plugin Composition Semantics

### 9.1 Overview of Extension Points

Kasane's UI extensions are primarily composed of the following mechanisms.

- Contribution (`contribute_to`)
- Line Annotation (`annotate_line_with_ctx`)
- Render Ornaments (`render_ornaments` — cell decoration + cursor style)
- Overlay (`contribute_overlay_with_ctx`)
- Transform (`transform`)
- Menu Item Transform (`transform_menu_item`)
- Display Directive (`display_directives`)
- Scroll Policy Override (`handle_default_scroll`)

These are not at the same level of abstraction; they differ in degrees of freedom and responsibilities.

These extension points are available to both native plugins (`Plugin` / `PluginBackend` traits) and WASM plugins (via WIT interface). The semantic contract is identical regardless of the plugin runtime; differences exist only in state access mechanisms and dependency declaration (see §9.11, §9.12).

The following table classifies each extension point by what it affects in the projection model and how outputs compose:

| Extension Point | Affects | Composition | Kakoune-Transparent? |
|---|---|---|---|
| `display_directives` | Π (Policy) | CommutativeComposable | ✓ always |
| `contribute_to` | Ω_L (Logical presentation) | CommutativeComposable | ✓ always |
| `annotate_line_with_ctx` | Ω_L | Accumulated | ✓ always |
| `contribute_overlay_with_ctx` | Ω_L | CommutativeComposable | ✓ always |
| `transform` | Ω_L | TransformChain (non-commutative) | ✓ always |
| `render_ornaments` (emphasis) | Ω_P (Physical presentation) | Priority-merged | ✓ always |
| `render_ornaments` (cursor_style) | Ω_P | Modality+Priority FirstWins | ✓ always |
| `handle_key_middleware` | ρ (Input routing) | FirstWins (3-variant) | iff handler emits no Kakoune-writing Command |
| `handle_mouse` | ρ | FirstWins | iff handler emits no Kakoune-writing Command |
| `handle_default_scroll` | ρ | FirstWins | iff handler emits no Kakoune-writing Command |

The "Kakoune-Transparent?" column records which extension points are guaranteed (by the shape of the extension point alone) to generate no Kakoune-writing Commands, and which depend on the handler body. The seven display/decoration extension points marked ✓ affect only the Kasane fibre `Delta(k)` (§2.5) and therefore satisfy A9 and Theorem T10 (Plugin Transparency, §12.10) unconditionally. Input-routing extension points may consume an event or delegate it to Kakoune; when they delegate, they emit a Kakoune-writing Command and are not transparent.

### 9.2 Contribution

`contribute_to()` is the most constrained extension, contributing `Element`s to framework-defined extension points (`SlotId`). Contributions carry `priority` and `size_hint`, making it easiest to maintain structural consistency. It is preferred whenever possible.

### 9.3 Line Annotation

`annotate_line_with_ctx()` is a mechanism for extending the gutter and background of each buffer line. It does not modify the buffer content itself but provides per-line visual contributions (`LineAnnotation`). Contributions from multiple plugins are composed through `BackgroundLayer` and `z_order`.

**Inline decoration uniqueness**: At most one plugin may provide an inline decoration per buffer line. This constraint is enforced in both debug and release builds with first-wins semantics: the first plugin (by registration order) that provides an inline decoration for a given line wins, and subsequent providers are dropped with a `tracing::warn!` diagnostic.

### 9.3.1 Render Ornaments (Cell Decoration & Cursor Style)

`render_ornaments()` is a unified extension point returning `OrnamentBatch`, which contains cell-level face overrides (`emphasis`) and cursor style proposals (`cursor_style`). These operate on the rendered grid (Ω_P) rather than the Element tree.

**Cell decorations** (`OrnamentBatch.emphasis`) apply face overrides to individual cells, cell ranges, or entire columns after paint. Unlike `annotate_line_with_ctx()` which operates at the line-level gutter/background, cell decorations target arbitrary screen coordinates (e.g., bracket match highlights, column guides).

Decorations from multiple plugins are collected, sorted by `priority` (ascending), and applied in order. The `FaceMerge` mode determines how each decoration interacts with the existing cell face: `Replace` overwrites entirely, `Overlay` merges non-default fields, `Background` applies only the background color.

**Cursor style** (`OrnamentBatch.cursor_style`) allows a plugin to override the cursor shape. When multiple plugins provide a value, resolution uses `OrnamentModality` rank (Must > Approximate > May), then `priority` (lower wins) as tiebreaker.

Render ornaments are available to both native plugins (via `HandlerRegistry::on_render_ornaments()`) and WASM plugins (`render-ornaments()` in WIT v0.25.0). See `plugin/render_ornament.rs` for type definitions.

### 9.4 Overlay

`contribute_overlay_with_ctx()` is a floating element overlaid separately from the normal layout flow. Overlays add display layers but do not modify the underlying protocol state. Display order is controlled via `z_index`.

### 9.5 Transform

`transform()` is a mechanism that receives an existing `Element` and returns a transformed version. It fulfills the roles of both the former Decorator (wrapping/decoration) and Replacement (substitution). The target is specified via `TransformTarget` and the application order via `transform_priority()`.

Element-level transforms are unified in the plugin composition pipeline. The transform chain uses `ElementPatch` — a declarative algebra with variants `Identity`, `WrapContainer`, `Prepend`, `Append`, `Replace`, `ModifyFace`, `Compose`, `ModifyAnchor`, and `Custom`. Patches are collected, composed, normalized (Identity removal, Replace absorption, Compose flattening), and applied. Pure patches (no `Custom`) are data values suitable for Salsa memoization. See `plugin/element_patch.rs` for implementation.

**Target hierarchy**: `TransformTarget` variants form a two-level refinement hierarchy. Style-specific targets (e.g. `MenuPrompt`, `InfoModal`) refine their generic parent (`Menu`, `Info`). Transforms are applied hierarchically: the generic parent chain first, then the specific target chain.

**Declarative properties**: `ElementPatch::scope()` auto-infers `TransformScope` from the patch variant, replacing manual `TransformDescriptor` declarations. In debug builds, the framework emits `tracing::warn!` when multiple plugins declare `Replacement` scope for the same target, or when non-identity transforms precede a replacement (since they will be absorbed). The `Custom` variant wraps an opaque function for transforms that cannot be expressed declaratively; it is treated as `Structural` scope.

**Categorical framing**: An `ElementPatch` is a morphism `Element → Element` acting on a subtree targeted by `TransformTarget`. The pure variants (`Identity`, `WrapContainer`, `Prepend`, `Append`, `Replace`, `ModifyFace`, `ModifyAnchor`, `Compose`, `When`) act by structural rewriting on the polynomial-functor presentation of `Element` (§2.6), and therefore commute with the functorial action of P on unaffected subterms. Plugin transforms are thus P-algebra morphisms on their scope. The `Custom` variant escapes this guarantee: it is an opaque function with no naturality condition, which is why it forces the patch into `Structural` scope and is excluded from the `is_pure()` set used for Salsa memoisation. Theorem T11 (§12.11) records the universal-property consequences of this structure.

`transform_menu_item()` is a separate extension point that transforms individual menu items before rendering. It shares the concept of element transformation but operates on a different pipeline with its own trait method. It is not part of the unified transform chain.

### 9.6 Composition Order and Priority

The rendering pipeline composes plugin outputs in three phases:

1. Build the seed default elements (framework-provided base UI)
2. Apply the transform chain in priority order
3. Compose contributions, annotations, and overlays

Each extension point has its own ordering rule. All multi-plugin results use stable, deterministic sorting. When priorities are equal, `PluginId` (lexicographic string comparison) breaks ties.

| Extension Point | Sort Key | Direction | Semantics |
|---|---|---|---|
| Contribution | `(priority, plugin_id)` | ASC | Lower priority → earlier in layout |
| Transform | `(priority, plugin_id)` | **DESC** (priority reversed) | Higher priority → applied first (innermost) |
| Annotation gutter | `(priority, plugin_id)` | ASC | Lower priority → leftmost |
| Annotation background | `(z_order, plugin_id)` | ASC, **last wins** | Highest z_order takes the line background |
| Overlay | `(z_index, plugin_id)` | ASC | Lower z_index → behind; higher → front |
| Display directive | `(priority, plugin_id, variant, anchor)` | `resolve()` composition | Multi-plugin composable (P-031) |
| Menu item transform | registration order | sequential chain | Output of previous = input of next |
| Cursor style override | registration order | first non-None wins | Single winner |
| Scroll policy override | registration order | first non-None wins | Single winner |

> **Algebraic structure**: The collection phase of each extension point forms a monoid (associative binary operation with identity), formalized in `plugin/compose.rs` as the `Composable` trait. Contribution, Overlay, and DirectiveSet are additionally commutative (`CommutativeComposable`): plugin evaluation order does not affect the collected result. Menu item transform, key dispatch, and cursor style override are non-commutative (order-dependent). `ElementPatch` forms a non-commutative monoid with `Identity` as identity and `Compose` as the binary operation; `normalize()` provides algebraic simplification (Identity removal, Replace absorption). Key middleware (`handle_key` → `KeyHandleResult` 3-variant threading) is an imperative Kleisli-style chain over `(Consumed, Ignored, Forward)` and is not modeled as `Composable`. `resolve()` remains unmodeled.
>
> The monoid laws for `Composable` implementations are proof obligations on each extension point's composition function. Associativity and identity are verified informally by inspection of the `compose()` implementations in `plugin/compose.rs`; the pure `ElementPatch` subset additionally benefits from `normalize()` producing a canonical form, so equality of patches up to normalisation witnesses associativity directly. Pure plugin transforms (§9.5) are P-algebra morphisms on their scope, composing naturally with the polynomial-functor presentation of §2.6.

> **Transform priority inversion**: Transform priority is intentionally inverted from contribution priority. High-priority transforms are applied first (closest to the seed element), so low-priority transforms control the final appearance. This matches the decorator pattern: the outermost decorator has the last word.

> **Effects merge**: When multiple plugins produce `Effects` in the same notification cycle, effects are merged by OR-ing `DirtyFlags` and appending `commands` and `scroll_plans` in plugin registration order.

### 9.7 Input Dispatch and Key Consumption

Plugin input handling follows a defined dispatch order.

1. `observe_key()` is called on **all** plugins (observation only, no consumption)
2. `handle_key()` is called on plugins in registration order; the **first** plugin to return a non-None result consumes the key
3. If no plugin consumes the key, built-in handlers (PageUp, PageDown, etc.) are tried
4. If no built-in handler matches, the key is forwarded to Kakoune

This is a first-wins dispatch model. Plugin registration order determines priority for key consumption. `observe_key` is always exhaustive; `handle_key` is short-circuiting.

**Algebraic characterization**: Key middleware forms a Kleisli-style chain over a 3-variant result type (`Consumed`, `Ignored`, `Forward`). Each plugin receives the key and returns one of these variants; the chain threads through plugins sequentially, short-circuiting on `Consumed`. This is fundamentally imperative and order-dependent, so it is not modeled as `Composable` in `plugin/compose.rs`.

### 9.8 Inter-Plugin Communication

Kasane provides three inter-plugin communication mechanisms with increasing levels of structure:

**PluginMessage** (point-to-point, untyped): Carries a target plugin ID and an opaque `Box<dyn Any>` payload. The runtime delivers the message to the target plugin's handler. Message delivery returns `(DirtyFlags, Vec<Command>)`. Delivery is synchronous within a single update cycle. No type safety or delivery guarantee.

**Topic-based Pub/Sub** (broadcast, typed at runtime): Publishers register on a `TopicId` and produce values each frame; subscribers register on the same topic and receive published values. Evaluation is two-phase: (1) all publications are collected into `TopicBus`, (2) values are delivered to subscribers. Cycle prevention: publishing during the delivery phase panics in debug builds. Type matching is runtime-enforced via `Box<dyn Any + Send>` downcast; mismatched types are silently ignored.

**Plugin-defined Extension Points** (structured, composable): A plugin defines an extension point (`ExtensionPointId`) with a `CompositionRule` (`Merge`, `FirstWins`, `Chain`). Other plugins contribute handlers for that extension point. The runtime evaluates contributions by collecting outputs from all contributors and applying the composition rule. Results are returned as typed `ExtensionResults`. This enables ecosystem-driven extensibility without framework source changes.

### 9.9 What Plugins May and May Not Change

Plugins may change display and interaction where policy can diverge.
Plugins may not change protocol truth, the core state machine, the semantics of the backend itself, or fabricate facts not provided by upstream.

Plugin-defined UI is not a precondition for core frontend semantics. Even in the absence of plugins, Kasane's standard frontend semantics must be self-contained. Display transformations introduced by plugins should in principle be additive, and must not capture core semantics by replacing the sole truth for standard users.

### 9.10 Plugin State Model (State-Externalized)

The `Plugin` trait externalizes plugin state ownership to the framework. The key semantic properties are:

- **State ownership**: The framework holds `Box<dyn PluginState>` for each Plugin. State transitions produce new values; the old state is replaced atomically.
- **Transition semantics**: All state-mutating operations return `(NewState, Vec<Command>)`. The framework detects changes via `PartialEq` on the concrete state type and increments a generation counter for `state_hash()`.
- **Invalidation**: `DirtyFlags::PLUGIN_STATE` (bit 7) signals plugin state changes. Plugin outputs are re-collected, with Salsa `PartialEq` early-cutoff on individual inputs preventing unnecessary downstream re-evaluation.
- **DynCompare**: `dyn PluginState` supports equality comparison via downcasting. Two states of different concrete types are always unequal.
- **Compatibility**: The framework adapts `Plugin` to `PluginBackend` internally, preserving generation-counter-based change detection. See `plugin/bridge.rs` for the adapter.

#### Dual Change Detection

Plugin state changes are tracked by two independent tiers:

- **Tier 1 (coarse)**: Current state is compared against a previous snapshot via `PartialEq` after every mutable hook. If different, a monotonic generation counter is incremented. The runtime reads generation counters to determine whether any plugin state changed.
- **Tier 2 (fine)**: When the coarse tier indicates a change, plugin outputs are re-collected. Individual Salsa inputs use `PartialEq` early-cutoff — unchanged contributions produce cached outputs even when re-collected.

Both tiers are necessary. The generation counter provides the coarse "did anything change?" gate. Salsa provides fine-grained memoization. Removing the generation counter would force Salsa to re-collect all plugin contributions every frame. Removing Salsa would lose incremental computation.

### 9.11 Plugin Model Positioning

Kasane provides two plugin trait models with different levels of abstraction.

- **`Plugin` trait** (recommended, primary API): 3-method trait with `HandlerRegistry`-based registration. The framework owns plugin state; handlers are pure functions. Plugins register only the handlers they need via `register(&self, r: &mut HandlerRegistry<Self::State>)`. Capabilities are auto-inferred from registered handlers. Automatic cache invalidation via `PartialEq`. Suitable for most plugins.
- **`PluginBackend` trait** (internal, advanced): Mutable state model with `&mut self`. Full access to all extension points including `Surface` and workspace observation. Intended for framework-internal use, WASM adapter, and advanced scenarios.

The following extension points are available only via `PluginBackend` (not `Plugin` trait): `surfaces()`, `workspace_request()`.

`PluginBridge` adapts `Plugin` to `PluginBackend` via `HandlerTable` — a type-erased dispatch table produced by `HandlerRegistry`. This enables both models to coexist in `PluginRuntime`. The semantic guarantees (extension point contracts, composition ordering, input dispatch) are identical for both models.

WASM plugins implement the equivalent of `PluginBackend` via WIT interface, with the host providing the adaptation layer. WASM plugins declare capabilities via `register-capabilities()` WIT export.

### 9.12 WASM Plugin Semantics

WASM plugins participate in the same composition model as native plugins but operate under additional constraints imposed by the Component Model boundary.

#### 9.12.1 Snapshot-Based State Access

WASM plugins do not access `AppState` directly. Before each WASM call, the host creates a snapshot of relevant state fields. The plugin reads this snapshot via host-imported functions.

```text
Invariant (WASM State Isolation):
  Within a single WASM call, the state snapshot is immutable.
  The plugin cannot observe state changes made by other plugins
  in the same frame.
```

State fields are organized into tiers (Tier 0–8), reflecting the evolutionary history of the WIT interface. All tiers are refreshed before each call.

#### 9.12.2 Element Arena Lifecycle

WASM plugins construct `Element` trees via host-imported builder functions. Elements are stored in a per-call arena that is cleared at the start of each WASM invocation.

Element handles returned by builder calls are valid only within the current invocation. They must not be cached or reused across calls.

#### 9.12.3 Dependency Declaration

WASM dependency declaration functions (`contribute_deps()`, `transform_deps()`, `annotate_deps()`) have been removed. The host now uses `state_hash()` as the sole mechanism for detecting WASM plugin state changes (see §9.12.4).

Each frame, the host compares the current `state_hash()` against the previous frame's value. If the hash differs, the host re-collects the plugin's contributions. If the hash is unchanged, cached contributions are reused.

#### 9.12.4 State Hash and Cache Invalidation

WASM plugins must implement `state_hash() → u64` to enable the host-side plugin slot cache. The host compares state hashes across frames to determine whether plugin contributions need recomputation.

Unlike the `Plugin` trait, where `PartialEq`-based change detection is automatic, WASM plugins bear full responsibility for state hash correctness. An incorrect state hash may cause stale contributions to persist.

#### 9.12.5 Capability Gating

Privileged operations (process spawning, filesystem access) require explicit capability grants declared via `requested_capabilities()`. The host constructs a per-plugin WASI context based on these grants. Native plugins default to full access; WASM plugins default to sandboxed.

## 10. Display Transformation and Display Units

### 10.1 Meaning of Display Transformation

Display Transformation is a policy that takes Observed State as material and constructs a different display structure. It can include omission, proxy display, supplementary display, and reconfiguration. Display Transformation is a view policy, not a falsification of protocol truth.

### 10.2 Classification of Display Transformations

Display Transformations are classified along their effect on visible Observed State. The classification refines the earlier Observed-preserving / Observed-eliding split (retained in ADR-018 for historical reference) into four cases that match the actual `DisplayDirective` and `ElementPatch` variants implemented in `plugin/element_patch.rs` and `display/mod.rs`.

- **Additive** — Adds new visual elements without removing or relocating any Observed content. Examples: `InsertAfter`, `InsertBefore`, overlay contributions, line annotations (gutter and background layers), cell emphasis via render ornaments. Additive transformations preserve Rendering Faithfulness (§5.6) trivially: no element of Observed State is elided.
- **Transforming** — Changes how an Observed element is displayed while retaining the element at its original location. Examples: face overrides via render ornaments (`FaceMerge::Overlay`, `FaceMerge::Background`), colour preview decorations, cursor-style overrides. Transforming transformations preserve Rendering Faithfulness because the underlying text is still visible at its original position.
- **Preserving (structural)** — Changes the spatial arrangement of Observed content without removing it. Example: `Fold` directive with a visible summary line (the folded lines are hidden, but the summary is interactive and toggling the fold restores the original). Preserving transformations satisfy Rendering Faithfulness provided the transformation is recoverable via user interaction within bounded steps (§10.2a).
- **Destructive** — Removes visual representation of some Observed State from the default display. Example: `Hide` directive without a corresponding summary. Destructive transformations satisfy Rendering Faithfulness only if an explicit recovery interaction exists (§10.2a) and the elision is active Display Policy State, not silent loss.

In Default Frontend Semantics, Additive, Transforming, and Preserving transformations are the standard permitted forms. Destructive transformations are permitted only when a recovery interaction is advertised. Under Extended Frontend Semantics, unrecoverable Destructive transformations may be enabled by explicit user consent (plugin configuration), at the cost of weakening A3 substitutability for that session.

Elided facts are never lost from the underlying `AppState`: Observed State in the fibration of §2.5 is always complete. Display Transformations are view policy applied over the top of the fibre projection.

### 10.2a Visual Faithfulness and Recoverability

The Rendering Faithfulness invariant (§5.6) asks that every Observed element either appear in the rendered output or be elided by active Display Policy. In the presence of Destructive or Preserving transformations this is too coarse: the user must still be *able* to access the elided content. Visual Faithfulness makes this condition precise.

A Display Transformation `T : Element → Element` is **visually faithful** iff for every Observed element `x` that is visible in the untransformed display but absent from `T`'s output, there exists a finite user interaction sequence `σ` (scroll, fold toggle, hover, navigation command, explicit policy override) such that `x` becomes visible again within bounded steps.

In Default Frontend Semantics:

- Additive and Transforming transformations are visually faithful by construction (no content is elided).
- Preserving transformations are visually faithful iff the spatial restructuring is reversible; `Fold` with a summary line that responds to an unfold command satisfies this because the fold toggle is a single interaction.
- Destructive transformations are visually faithful iff an explicit recovery interaction is registered by the plugin and documented in its user-facing surface (help, keybinding list, visible marker, or configuration hint).

Plugins introducing Destructive Display Directives are required under Default Frontend Semantics to advertise their recovery interaction. This is currently a documentation obligation, not a type-level contract; see known gap §13.14 for the missing formal witness.

Visual Faithfulness is a stricter condition than Rendering Faithfulness: it demands not only that the transformation be labelled as a Display Policy but that the elision be *reversible* from the user's viewpoint. It is the condition that preserves the spirit of A3 (Behavioral Equivalence) even when the default display diverges from Kakoune's ncurses output.

### 10.3 Boundaries of Display Transformation

Display Transformation may change display structure and interaction policy. What it may not change is falsifying Observed State content as "facts given by upstream."

For example, a fold summary may summarize multiple lines into one, but that summary must not be treated as the actual buffer lines sent by Kakoune.

**Multi-plugin composition (P-031):** Multiple plugins may contribute display directives simultaneously. The `resolve()` function composes them deterministically:

- **InsertAfter**: All kept; same-line ordering by `(priority, plugin_id)`.
- **InsertBefore**: All kept; same-line ordering by `(priority, plugin_id)`.
- **Hide**: Set union of all ranges (idempotent).
- **Fold overlap**: Higher `(priority, plugin_id)` wins entirely; lower-priority overlapping fold dropped whole (protects summary integrity).
- **Fold-Hide partial overlap**: Fold removed (conservative — partial hide invalidates fold summary).
- **InsertAfter suppression**: Inserts targeting hidden or folded lines removed.
- **InsertBefore suppression**: Inserts targeting hidden or folded lines removed.

Plugins declare priority via `display_directive_priority()` (default 0). The resolved directives are used to construct the display map.

### 10.4 Meaning of Display Unit

A Display Unit is an operable display unit within the reconfigured UI. A Display Unit is not merely a layout box; it collectively represents the display target, its relationship to the source, and the availability of interaction.

A Display Unit can carry the following information.

- geometry
- semantic role
- source mapping
- interaction policy
- navigation relationships with other Display Units

### 10.5 Meaning of Source Mapping

A Display Unit may have a mapping to corresponding buffer positions, buffer ranges, selections, derived objects, or plugin-defined objects.

This mapping is not necessarily one-to-one. A single Display Unit may represent multiple lines, and conversely, a single line may be split into multiple Display Units.

### 10.6 Restricted Interaction

If a Display Unit does not have a complete inverse mapping to its source, that unit may be treated as read-only or with restricted interaction.

The important point is to not leave "undefined operation results" implicit. Kasane should be able to explicitly represent units where interaction is impossible or restricted.

### 10.7 Responsibilities of Plugins and Display Transformation

Plugins can introduce Display Transformations and Display Units, but they bear the following responsibilities.

- Do not fabricate protocol truth
- Keep interaction policy within definable bounds
- Accept degraded mode when source mapping is weak

The core guarantees the following in return.

- Transformations are treated as view policy
- Display units can be represented as targets for hit testing, focus, and navigation
- Plugin-defined UI can participate in the same composition rules as standard UI
- Plugin-defined UI builds upon standard frontend semantics as its foundation, and must not break the meaning of the core frontend in its absence

## 11. Surface, Workspace, and Session

### 11.1 Meaning of Surface

A Surface is an abstraction that owns a rectangular region on screen and handles its own view construction, event processing, and state change notifications. The core's main screen elements are represented as Surfaces.

### 11.2 Meaning of SurfaceId

`SurfaceId` is a stable ID that identifies a surface. Buffer, status, menu, info, and plugin surfaces belong to different `SurfaceId` spaces.

### 11.3 Meaning of Workspace

A Workspace is a layout structure that manages surface placement, focus, splitting, and floating. A Workspace represents "which surface is where."

### 11.4 Relationship Between Surfaces and the Existing View Layer

Surface lifecycle has been integrated into the core view pipeline. The rendering pipeline delegates element composition to the surface registry when surfaces are registered, falling back to default construction otherwise. See `surface/registry/compose.rs` for implementation.

Therefore, Surface is partially integrated as a first-class abstraction. Full unification (where all core UI elements are Surfaces) is not yet complete.

### 11.4a Per-Pane Status Bar Rendering

In multi-pane mode, the global status bar surface is not rendered at the screen-level composition. Instead, each pane renders its own status bar: the same status bar descriptor is rendered once per pane with that pane's own state.

Pane-specific context is propagated through the rendering system to plugin contributions during slot resolution. This ensures that plugin contributions (e.g., sel-badge, session-ui) in status bar slots receive the correct pane-specific state and focus information.

Each pane's element tree is composed as `Column [buffer(flex=1.0), status(fixed)]` (or `[status, buffer]` when `status_at_top` is true). Kakoune clients are resized to `rect.h - 1` to account for the status bar row consumed within each pane.

In single-pane mode, the global status bar rendering path is unchanged.

Prompt cursor positioning in multi-pane mode uses the focused pane's rectangle (`focused_pane_rect`) to compute absolute screen coordinates for the cursor, rather than assuming the status bar is at row 0 or `grid.height() - 1`.

### 11.5 Current Constraints

The current implementation has at least the following constraints.

- Invalidation is still centered on global `DirtyFlags`
- There are places where the `rect` received by a `Surface` and the final rendering are not fully consistent
- Overlay positioning and parts of the core view coexist with legacy paths

### 11.6 Relationship to Future Per-Surface Invalidation

`SurfaceId`-based invalidation is a promising future direction, but this document does not treat it as part of the current semantics. What is addressed here is solely the fact that the current system assumes global dirty.

### 11.7 Meaning of Session

A Session represents a single managed Kakoune client process and its associated UI state. `SessionManager` assigns a stable `SessionId` to each session and tracks multiple sessions concurrently. At any given time, exactly one session is active and rendered; inactive sessions are held in the background with their Kakoune readers still alive. The Kakoune server runs as a separate headless daemon (`kak -d`); sessions correspond to client connections (`kak -ui json -c`), not to the server process itself.

A session is not a Surface, a Workspace layout, or a buffer. It is the runtime container that binds a Kakoune client process, an `AppState` snapshot, and (in the future) a set of session-bound surfaces into a single switchable unit.

### 11.8 Session State Preservation

When the active session switches, `SessionStateStore` saves a full `AppState` clone of the outgoing session and restores the stored snapshot of the incoming session. This transition is atomic from the perspective of the rendering pipeline: the pipeline always sees a complete, consistent `AppState`.

Inactive sessions continue to receive Kakoune protocol messages. Their `AppState` snapshots are updated in the background, so when an inactive session is activated, it reflects the latest state from its Kakoune process rather than a stale snapshot from the moment of deactivation.

### 11.9 Session and Surface Binding (Current Constraints)

In the current implementation, the relationship between sessions and surfaces is not yet formalized. Surfaces are registered globally in `SurfaceRegistry` and are not scoped to a specific session. Automatic generation of session-bound surface groups (buffer, status, supplemental) and deterministic surface detachment on session switch are planned but not yet implemented.

Until session-bound surface generation is in place, multi-session operation relies solely on `AppState` snapshot swapping, and the surface composition does not change on session switch.

## 12. Equivalence and Proof Obligations

### 12.1 T1: Presentation Equivalence

For identical World state, all implementation paths produce identical presentation Ω. This is the foundational determinism property.

```text
Theorem T1 (Presentation Equivalence):
  For all valid states S:
    render_pipeline(S) produces deterministic output.
    Two calls with identical S yield byte-identical CellGrid.
```

Verified by `trace_equivalence.rs` property tests using proptest-generated state mutations.

### 12.2 T2: Backend Equivalence

TUI and GUI differ in output representation but are required to display the same UI structure and semantic content for the same state.

```text
Theorem T2 (Backend Equivalence):
  For all valid states S:
    The semantic content of TUI output (CellGrid) and GPU output
    (DrawCommand sequence) is identical: same text, same positions,
    same overlays, same interactive elements.
```

The backend's freedom is limited to "how to draw it" — rendering technology, not semantic content.

### 12.3 T3: Incremental Equivalence

The Salsa-cached production path must produce output identical to the reference full-pipeline path.

```text
Theorem T3 (Incremental Equivalence):
  For all valid states S:
    render_pipeline(S) ≡obs render_pipeline_cached(S)
  where ≡obs denotes identity of the final CellGrid / DrawCommand
  sequence as observable output.
```

Pipeline variants:

- `render_pipeline` — DirectViewSource, `DirtyFlags::ALL` hardcoded. Reference path for correctness testing.
- `render_pipeline_direct` — DirectViewSource with explicit `DirtyFlags` parameter. Used in incremental rendering benchmarks.
- `render_pipeline_cached` — SalsaViewSource. Production path with Salsa memoization.

Salsa is an optimization, not a theoretical primitive. T3 guarantees that the optimization preserves semantics. Verified by `salsa_pipeline_comparison.rs` and `trace_equivalence.rs`.

### 12.4 T4: Composition Determinism

Plugin composition order is deterministic for a fixed set of plugins.

```text
Theorem T4 (Composition Determinism):
  For CommutativeComposable extension points, output is independent
  of plugin registration order. For TransformChain, the chain order
  is determined by plugin priority (stable sort on priority value).
```

The monoid-based composition traits (`CommutativeComposable`, `TransformChain`) enforce algebraic properties that guarantee determinism.

### 12.5 T5: Default Sufficiency

```text
Theorem T5 (Default Sufficiency):
  With no plugins registered, Kasane satisfies all requirements
  of Default Frontend Semantics (§2.4). Plugin contributions are
  purely additive to a self-contained core.
```

### 12.6 T6: Plugin Safety

```text
Theorem T6 (Plugin Safety):
  Plugin effects ⊄ Truth. No plugin can modify #[epistemic(observed)]
  fields. Plugin effects flow through Command::SendToKakoune (which
  Kakoune may accept or reject) or through presentation-layer
  extension points that affect only Π and Ω.
```

This is the structural enforcement of Axiom A7 (§2.7). `PluginView<'_>` provides read-only access during the render phase.

### 12.7 T7: Degradation Boundedness

```text
Theorem T7 (Degradation Boundedness):
  When a heuristic inference fails, the degradation is bounded to
  the declared severity level of that inference rule. No heuristic
  failure can corrupt Truth or prevent rendering.
```

Each heuristic carries `#[epistemic(heuristic, rule="I-N", severity="...")]`. The severity levels are: `degraded` (visual quality loss), `absent` (feature unavailable), `incorrect` (wrong information displayed). See `derived/mod.rs` for the inference rule catalog.

### 12.8 T8: Kakoune Bisimulation

A3 (Behavioral Equivalence, §2.7) is formalised as a weak bisimulation between Kakoune's labelled transition system and Kasane's.

```text
Theorem T8 (Kakoune Bisimulation):
  Under Default Frontend Semantics, there exists a weak bisimulation
  relation R ⊆ S_kak × S_kas such that:
    (i)   Initial states are R-related: (k_0, s_0) ∈ R.
    (ii)  For every observable action a ≠ τ:
            (k, s) ∈ R ∧ k →^a k'
              ⟹ ∃ s'. s ⇒^a s' ∧ (k', s') ∈ R
            where ⇒^a denotes τ* a τ*.
    (iii) For every Kasane transition emitting a Kakoune-writing
          Command (Command::SendToKakoune, InsertText, EditBuffer):
            (k, s) ∈ R ∧ s →_kas s'
              ⟹ ∃ k'. k →^a k' ∧ (k', s') ∈ R
          where a is the observable action induced by the Command.
    (iv)  τ-transitions in S_kas (rendering phases, Salsa cache,
          plugin state evolution emitting no Kakoune-writing Command,
          display map recomputation, hit map rebuild) do not advance
          Kakoune's LTS and preserve R.
```

Under Extended Frontend Semantics, R may be weakened: plugin-driven Destructive Display Transformations (§10.2) without advertised recovery interaction permit Kasane's rendered output to diverge from Kakoune's ncurses rendering, breaking clause (ii). When this happens the user is responsible for the divergence through explicit plugin configuration.

Empirical evidence for R's existence on the native rendering path is provided by `trace_equivalence.rs` property tests (proptest-generated state mutations) and `salsa_pipeline_comparison.rs` (Salsa vs direct path equivalence).

### 12.9 T9: Delta Coherence

A9 (Delta Neutrality, §2.7) is the structural basis for all τ-transitions in T8.

```text
Theorem T9 (Delta Coherence):
  Every internal state transition s →_kas s' that emits no
  Kakoune-writing Command preserves the fibration projection:
    p(s) = p(s')
  where p : AppState → KakouneProtocolFacts is the projection
  of §2.5.
  Consequently, layout re-solving, paint re-execution, Salsa cache
  refresh, plugin tick, overlay recomputation, multi-surface display
  manipulation, hit map rebuild, display map recomputation, and
  workspace layout adjustments never cause Kakoune to receive
  unintended commands.
```

Proof sketch: `p` extracts `#[epistemic(observed)]` fields (§3.2). Kasane-internal transitions by definition touch only Runtime / Derived / Heuristic / Display Policy fields (§3.3–§3.6), which are disjoint from Observed fields by the compile-time `DirtyTracked` derive (§3.9). ∎

T9 is the converse direction of A9: A9 constrains which transitions may emit Kakoune commands, while T9 states that τ-transitions leave the fibre alone.

### 12.10 T10: Plugin Transparency

T6 (Plugin Safety, §12.6) is the negative statement "plugin effects ⊄ Truth". The positive form is:

```text
Theorem T10 (Plugin Transparency):
  A plugin P is Kakoune-transparent iff none of P's registered
  handlers — transform, contribute_to, annotate_line, contribute_
  overlay, display_directives, render_ornaments, handle_key,
  handle_mouse, handle_default_scroll, observe_key, event
  processing — emit Command::SendToKakoune, Command::InsertText,
  or Command::EditBuffer in any execution path.

  A Kakoune-transparent plugin is R-preserving: adding or removing
  P from a Kasane instance does not change which bisimulation
  class (§12.8) the instance belongs to. Equivalently, for any
  Kakoune session, Kasane-with-P and Kasane-without-P produce the
  same Kakoune-side behaviour.
```

By the §9.1 extension points table, all display/decoration extension points (`display_directives`, `contribute_to`, `annotate_line_with_ctx`, `contribute_overlay_with_ctx`, `transform`, `render_ornaments`) are structurally Kakoune-transparent: they cannot emit Kakoune-writing Commands. Input-routing extension points (`handle_key_middleware`, `handle_mouse`, `handle_default_scroll`) are transparent iff their handler bodies are. Consequently the entire presentation-layer composition (§9) is Kakoune-transparent by construction, and Kakoune-visible effects are confined to explicit event-loop Commands issued from plugin event handlers.

T10 follows from A2 (sharpened form) and A9: presentation-only transitions do not cross the fibration, and adding further such transitions does not change the bisimulation equivalence class. T10 is the theoretical justification for Kasane's position that the plugin ecosystem does not threaten the "alias kak=kasane" substitutability goal.

### 12.11 T11: Element Initial Algebra

```text
Theorem T11 (Element Initial Algebra):
  Element ≅ μX. P(X) for the polynomial endofunctor P defined in
  §2.6. As an initial algebra, Element admits:
    (F1) Unique catamorphism: for any P-algebra (A, α : P(A) → A),
         there exists a unique fold_α : Element → A satisfying
         fold_α ∘ in = α ∘ P(fold_α) where in : P(Element) → Element
         is the structural constructor.
    (F2) view() factors through P's functorial action: changes to a
         subtree affect only fold_α values that depend on that
         subtree.
    (F3) Pure plugin transforms (§9.5, excluding ElementPatch::Custom)
         are P-algebra morphisms restricted to the scope designated
         by TransformTarget, and therefore commute with P's action
         on unaffected subterms.
```

T11 is a structural fact about the `Element` enum defined in `kasane-core/src/element.rs`. It does not impose new requirements on the code; it records the universal properties already present. The consequences (F1)–(F3) are the theoretical basis for Salsa memoisation of pure patches (`ElementPatch::is_pure()`) and for the structural rewriting performed by `ElementPatch::normalize()`.

If a future `Element` variant is added without a corresponding extension to P in §2.6, T11's accuracy is broken; this is recorded as synchronization obligation in §15 (Change Policy).

### 12.12 T12: Command Free Monad

```text
Theorem T12 (Command Free Monad):
  The Command enum forms an effect signature CommandSig. The output
  of update() for a single frame is a value of Free(CommandSig).
  The runtime interpreter (event_loop/dispatch.rs,
  plugin/command.rs::execute_commands) is an algebraic-effect
  handler that interprets Free(CommandSig) into I/O, protocol
  traffic, and process management.

  Consequences:
    (F4) update() is a pure function
           AppState × Msg → AppState × Free(CommandSig).
    (F5) Effect sequencing composes by free-monad bind; multi-step
         workflows (plugin message → Kakoune forward → timer reschedule)
         are a single effect-tree value.
    (F6) The Kakoune-writing subset {SendToKakoune, InsertText,
         EditBuffer} is the distinguished fragment of CommandSig
         driving Kakoune's LTS in T8; the remaining variants are
         internal effects whose handler interpretation preserves
         the fibre projection p.
```

T12 records the algebraic structure already present in `plugin/command.rs`. Proofs of T1 (Presentation Equivalence) and T4 (Composition Determinism) rest on (F4): identical state and input produce byte-identical `Free(CommandSig)`.

### 12.13 What Tests Guarantee

What tests primarily guarantee are the following properties.

- Presentation equivalence (T1) via proptest in `trace_equivalence.rs`
- Incremental equivalence (T3) via `salsa_pipeline_comparison.rs` and `trace_equivalence.rs`
- Empirical evidence for weak bisimulation (T8) via the same property tests — any divergence in rendered output between identical input sequences indicates a violation of R
- Plugin cache invalidation consistency (generation counter state hash)
- Preservation of semantics shared across backends (T2)

T9 (Delta Coherence), T10 (Plugin Transparency), T11 (Element Initial Algebra), and T12 (Command Free Monad) are structural theorems that follow from the type definitions and compile-time classifications (`#[epistemic(...)]` annotations, `DirtyTracked` derive, the `Element` enum, the `Command` enum). They are not guarded by dedicated property tests; instead, they are invariants that would be broken by incorrect extensions to the underlying types. The synchronization obligations in §15 (Change Policy) track updates to these theorems when the corresponding enums evolve.

### 12.14 Contracts Expressible Only in Prose

The following contracts are difficult to fully express through tests alone.

- That heuristic state is not on par with protocol truth (§3.4, A2)
- The boundaries that plugins may and may not cross (§9.9, A7)
- That WASM state snapshot isolation holds across the Component Model boundary (§9.12)

As a non-goal of Kasane, requiring existing Kakoune users to participate in a Kasane-specific ecosystem within the standard frontend semantics is not included. Kasane has a plugin platform, but Default Frontend Semantics is not subordinate to it.

These are maintained through both prose and tests.

### 12.15 What Must Be Consistent Across Backends

TUI and GUI differ in output methods, but at least the following semantics must be consistent.

- What is displayed
- Where it is displayed
- Which state changes produce which view changes
- Which overlays/menus/info popups are visible

## 13. Known Gaps

### 13.1 ~~Non-Strictness Due to `stable()`~~

> Resolved. `stable()` was removed with the introduction of Salsa (ADR-020). Exact Semantics and incremental evaluation now coincide (§5.2).

### 13.2 Limits of Dependency Tracking

Salsa provides automatic dependency tracking for native rendering paths. Remaining limitations:

- **WASM `state_hash()` is manual.** Incorrect implementations may cause stale plugin output without detection (§8.4, §9.12.4).
- **`no_eq` on tracked functions.** Output-level early-cutoff is disabled because no downstream tracked functions depend on the view functions' Element outputs (§7.11). `Element` implements `PartialEq`, so enabling output-level cutoff is technically feasible if the pipeline is deepened.
- **Salsa input comparison cost.** Input synchronization performs `PartialEq` comparisons each frame, including deep comparisons of buffer content. This is correct but carries a per-frame cost proportional to buffer size.

### 13.3 Mismatch Between Global DirtyFlags and Surface Theory

Surfaces have been introduced as localized rectangular abstractions, but invalidation still heavily depends on global dirty.

### 13.4 Mismatch Between Workspace Ratio and Actual Rendering

There is room for the split ratios computed on the Workspace side and the final flex allocation on the view composition side to not fully agree.

### 13.5 Gap in Plugin Overlay Invalidation

The GUI-side scene invalidation and plugin overlay dependencies are not fully integrated, leaving theoretical room for overlays to become stale.

### 13.6 Display Transformation and Core Invalidation

The `DisplayMap` is integrated into the rendering pipeline. Display directives flow through the incremental computation system, and the `DisplayMap` is rebuilt each frame and propagated through the rendering pipeline and cursor/input functions. Salsa's automatic dependency tracking ensures that changes to display directives trigger re-evaluation of dependent tracked functions.

The display unit model (P-040..P-043) is implemented: `DisplayUnit`, `DisplayUnitId`, `SemanticRole`, and `UnitSource` provide a first-class unit abstraction. Navigation is resolved per unit via `NavigationPolicy` (plugin-dispatched, FirstWins composition) and `NavigationAction` / `ActionResult`. Remaining gap: per-display-unit dirty tracking is not yet implemented; invalidation still operates at the full-`DisplayMap` granularity.

### 13.7 Display-Oriented Navigation Scope

Visual unit-based navigation is implemented via `NavigationPolicy` and `NavigationAction` (P-042, P-043). Plugin-defined navigation policies are dispatched through `HandlerRegistry`. Remaining gap: sub-line display units (`UnitSource::Span`) are defined in the type but not yet produced by the builder, and a complete unification theory with Kakoune's buffer-oriented cursor model is unfinished.

### 13.8 WASM State Hash Accuracy

WASM plugins implement `state_hash() → u64` manually (§9.12.4). An incorrect hash — one that returns the same value despite internal state changes — may cause stale contributions to persist without detection. Unlike the `Plugin` trait where `PartialEq`-based change detection is automatic, WASM plugins bear full responsibility for hash correctness.

### 13.9 WASM Snapshot Consistency Across Plugins

WASM plugins receive a frozen state snapshot before each call (§9.12.1). Multiple plugins' state changes within a single frame are not atomically visible to subsequent WASM calls; each call sees a fresh snapshot. This means WASM plugin ordering may affect observable output when plugins have state dependencies on each other.

### 13.10 Menu Item Transform Outside Unified Pipeline

`transform_menu_item()` operates separately from the Element-level transform chain (§9.5). The two transform mechanisms have independent priority orderings and are not subject to the same composition rules.

### 13.11 HitMap Frame Delay

Mouse routing uses the previous frame's HitMap. The HitMap is rebuilt after rendering, so input events within a batch are routed using a potentially stale hit map. This introduces at most one frame of stale mouse routing (~16ms).

This is an accepted tradeoff documented in the frame loop code. It is recorded here because it represents a deviation from the "current frame reflects current state" ideal.

### 13.12 DisplayMap Frame Delay

Mouse coordinate translation uses the DisplayMap from the previous frame (§6.2). Before the first render, `AppState.display_map` is `None` and mouse events use identity mapping. After the first render, the DisplayMap is persisted and used for subsequent frames. This is the input-side analog of the HitMap frame delay (§13.11).

This gap was partially resolved by persisting the DisplayMap on AppState after each render frame. The one-frame delay remains as an accepted tradeoff.

### 13.13 Delta Neutrality Lacks Static Enforcement

A9 (Delta Neutrality, §2.7) and T10 (Plugin Transparency, §12.10) follow from the shape of state transitions relative to the Kakoune-writing subset `{SendToKakoune, InsertText, EditBuffer}` of `Command`. Field-level classification of observed vs Kasane-internal state is enforced at compile time via `#[epistemic(...)]` and the `DirtyTracked` derive (§3.9).

**Level 1 enforcement (ADR-030, shipped).** A read-side projection `Truth<'a>` (`kasane-core/src/state/truth.rs`) exposes only `#[epistemic(observed)]` fields and is compile-time write-denying. A structural test pins `Truth::ACCESSOR_NAMES` against the macro-generated `FIELDS_BY_CATEGORY["observed"]` set, and a property test (`kasane-core/tests/delta_neutrality.rs`) witnesses that no non-`Msg::Kakoune(..)` message mutates the projection. The Salsa layer no longer drops observed status components, closing the `status_prompt` / `status_content` / `status_content_cursor_pos` projection gap.

**Still open.** There is still no compile-time check that a given handler — especially in plugin code — does not emit a Kakoune-writing Command by mistake. The "Kakoune-Transparent?" column in §9.1 is therefore currently maintained by code review for `handle_key_middleware`, `handle_mouse`, and `handle_default_scroll`. A linting step or a marker trait distinguishing "transparent Command" from "Kakoune-writing Command" would close this gap (see ADR-030 Levels 2–3 and roadmap §2.2).

### 13.14 Visual Faithfulness Has No Formal Witness

The Visual Faithfulness condition (§10.2a) requires every active Destructive Display Transformation to be recoverable by a bounded user interaction sequence. Plugins currently fulfil this obligation by convention: fold-style plugins provide a toggle, hide-style plugins document their reveal command in their help surface. No type or trait currently witnesses the existence of a recovery interaction. A `RecoveryWitness` associated type on destructive directive contributors, or a contract check at plugin registration, would make the condition enforceable.

### 13.15 Plugin Transparency Is Not Statically Decidable

T10 (Plugin Transparency, §12.10) classifies a plugin as Kakoune-transparent iff none of its handlers emit Kakoune-writing Commands. The classification is currently done manually on a per-plugin basis. A static analysis (control-flow reachability from handler entry points to `Command::SendToKakoune`, `InsertText`, `EditBuffer` construction sites) would make the "Kakoune-Transparent?" column of §9.1 automatically derivable.

### 13.16 Element's Polynomial Functor Structure Is Implicit

Theorem T11 (§12.11) records that `Element ≅ μX. P(X)` for a polynomial endofunctor P written out in §2.6. The code does not carry any trait (HKT, polynomial-functor trait, or equivalent) that witnesses this structure. Plugin transforms manually pattern-match on `Element` variants instead of factoring through P's functorial action. Consequently, a future `Element` variant added without updating §2.6 would silently break the accuracy of T11. A regression test comparing the `Element` variant list against the §2.6 P(X) sum could close this gap.

### 13.17 Free Monad of Commands Is Implicit

Theorem T12 (§12.12) records that update output is a value of `Free(CommandSig)`. The code expresses this as `Vec<Command>` plus ordering conventions rather than as an explicit `Free<Sig>` type. Making the free-monad structure explicit would allow static analysis of effect sequences (e.g., detecting a handler path that indirectly produces a Kakoune-writing Command through a chain of deferred Commands), but would require a substantial refactor of the update pipeline. The gap is recorded here so that future work can refer to T12 directly rather than rederiving the effect algebra.

### Resolved Gaps

The following gaps have been resolved and are retained for historical reference.

- **Transform and Replacement unification**: At the Plugin trait level, `transform()` has absorbed both decorator and replacement into a unified transform chain. The old APIs (`decorate()`, `replace()`) have been removed from the Plugin trait.
- **Session invisibility to plugins**: Session observability infrastructure has been implemented: `AppState.session_descriptors` and `active_session_key` expose session state, `DirtyFlags::SESSION` notifies plugins of lifecycle changes, and `SessionCommand::Switch` allows plugins to request session activation. WASM plugins access these via WIT Tier 8 host-state functions and the `switch-session` command variant.
- **P-031 Single-plugin display directive exclusivity**: Display directives now support multi-plugin composition via `DirectiveSet` monoid and `resolve()`. Priority-based fold conflict resolution, hide union, and insert suppression enable combining code folding + virtual text from different plugins.
- **Non-strictness due to `stable()`**: `stable()` and manual dependency tracking were removed with the introduction of Salsa (ADR-020). Exact Semantics and incremental evaluation now coincide.
- **DisplayMap not persisted for mouse input**: `mouse_to_kakoune()` previously received `None` for the DisplayMap parameter, causing mouse clicks on display-transformed content to use incorrect buffer coordinates. Resolved by persisting `DisplayMap` on `AppState.display_map` after each render frame.

## 14. Non-Goals

### 14.1 Optimizations Not Covered in This Document

Individual micro-optimizations and benchmark tuning are not covered here. What is covered is only the semantics that such optimizations must preserve.

### 14.2 User-Facing Configuration Not Covered in This Document

Configuration methods for themes, layout, keybindings, etc. are not covered. Only which semantic boundary a given configuration belongs to is addressed.

### 14.3 Future Proposals Not Covered in This Document

Proposals for Phase 5 and beyond, or ideal designs after upstream changes, are explicitly distinguished from the current semantics.

## 15. Change Policy

### 15.1 When to Update This Document

This document is updated when any of the following change.

- Meaning of state classification
- DirtyFlags or invalidation policy
- Plugin composition order
- Surface/Workspace semantics
- Definition of observational equivalence
- The `Element` enum variants (synchronise with P(X) in §2.6 and T11)
- The `Command` enum variants or the Kakoune-writing subset (synchronise with §4.3, A2, A9, T12)
- The set of extension points or their Kakoune-transparency status (§9.1 table)
- The set of DisplayDirective variants or their faithfulness classification (§10.2)

### 15.2 Relationship with ADRs

ADRs preserve the history of "why that decision was made." This document is the authoritative reference for "what is currently the specification." When the two conflict, this document takes priority as the current specification, and notes are added to the ADR as needed.

### 15.3 Synchronization with Test Updates

When semantics change, the following should also be updated in the same change whenever possible.

- Related prose
- Related tests
- Necessary invalidation / cache implementation

Changes that advance only semantics or only tests are avoided in principle.

## 16. Related Documents

- [index.md](./index.md) — Documentation entry point and architecture overview
- [plugin-api.md](./plugin-api.md) — Plugin API reference
- [requirements.md](./requirements.md) — Authoritative reference for requirements
- [decisions.md](./decisions.md) — History of design decisions
