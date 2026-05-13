# ADR-044: Handler → Effect Tier Hierarchy

**Status:** Closed (2026-05-11). All Phase A subphases plus Phase
B-1 / B-2 / B-3 / B-4 / B-4-bind / B-5 shipped. ABI is now
`kasane:plugin@5.0.0` with tier-typed handler exports; the silent
process-spawn-from-state-changed misuse described in the Context
section is now a wit-bindgen compile error. Phase 2/3 of the
silent-drop fix chain
([#100](https://github.com/Yus314/kasane/issues/100) → ADR Phase 0,
[#101](https://github.com/Yus314/kasane/issues/101) → ADR Phase 1,
this ADR → Phase 2/3).

### Landed phases (2026-05-11)

| Phase | Scope                                                                 | Outcome                                                                                                                                                       |
|-------|-----------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------|
| A-1   | Foundation types (`ObservationEffects`, `KakouneSideEffects`, `ProcessCapableEffects`; command split enums; `From` lifts)                                | Tier types defined in `kasane-core/src/plugin/effect_tiers.rs`, callable but not yet enforced at handler boundaries. |
| A-2   | ADR-030 rename `KakouneSafe*` → `KakouneTransparent*` (108 occurrences)                                                                                  | Frees the `KakouneSafe` namespace, sharpens semantics. ADR-030 §Status records the rename.                            |
| A-3a  | First tier-enforced setter: `HandlerRegistry::on_state_changed_tier1`. Asymmetric `From` web (`KakouneSideEffects → Effects`, no reverse) is the enforcement. | `BuiltinShadowCursorPlugin` migrated as canonical example. `compile_fail` doctest witnesses the type-level rejection of raw `Effects` containing `ProcessCommand`. |
| A-3b  | Tier setters for the rest of the Effects-shaped lifecycle handlers: `on_init_tier1`, `on_session_ready_tier1`, `on_io_event_tier2`, `on_update_tier2`.   | Inter-tier `From` lifts (Tier 0 → Tier 1 → Tier 2) added so narrower-tier closures fit wider-tier setters. `compile_fail` doctest on `ProcessCapableEffects`. |
| A-3c  | Tier setters for declarative process tasks: `on_process_task_tier2`, `on_process_task_streaming_tier2`.                                                  | All seven Effects-returning lifecycle handler categories now have tier-enforced parallels. Legacy setters remain with docstring pointers.                       |
| A-3d  | Opt-in **tier-1** input handler setters: `on_key_tier1`, `on_text_input_tier1`, `on_drop_tier1`, `on_mouse_fallback_tier1`. ADR mapping puts input at Tier 2 by default; Tier 1 is a stricter opt-in for plugin authors who know their handlers don't spawn. | Bound `C: Into<KakouneSideCommand>` rejects raw `Command` returns. `compile_fail` doctest on `KakouneSideCommand` witnesses the asymmetric command projection. |
| A-3f  | In-tree built-in plugin migration to tier setters. | `BuiltinInputPlugin`, `BuiltinMouseFallbackPlugin`, `DebugOverlayPlugin`, plus `BuiltinShadowCursorPlugin` (migrated in A-3a). Validates the tier API against real in-tree code. |
| B-1   | WIT tier-type foundation: add `kakoune-side-command` / `kakoune-side-effects` / `process-command` / `process-capable-effects` to `kasane-wasm/wit/plugin.wit` (single canonical copy; the two SDK paths are symlinks). | Types declared, no handler exports yet wired (deferred to B-2). ABI stays 4.1.0 — no rebuild required for bundled `.wasm` plugins. |
| B-2   | WIT ABI 4.2.0 bump: add `on-state-changed-tier1-effects` export returning `kakoune-side-effects`, host dispatch merges with the legacy export, SDK macros provide a default no-op so existing plugins recompile without source changes. | All 13 `.wasm` blobs rebuilt; host `convert_kakoune_side_effects` routes tier-1 commands through the existing `convert_command` so attribution / `set-setting` / `command-error` rewrites stay uniform. Commit `2aca004d`. |
| B-3   | `define_plugin!` learns the tier-1 DSL key `on_state_changed_tier1_effects(...)` and rejects declaring both legacy and tier-1 simultaneously. SDK ships `kakoune_side_setup_effects!` (eval-command shorthand) and `tier1_effects(commands)` (Vec helper). | Conflict diagnostic is compile-time. Fixture guest `kasane-wasm/guests/tier1-state/` + host test `tier1_state.rs` witness the macro → wire → host merge end-to-end. Commit `6a44b1dd`. |
| B-4   | First batch of `examples/wasm/*` plugins migrated to the tier-1 form: `color-preview`, `image-preview`, `session-ui`, `selection-algebra`. None emit process commands, so the migration is pure type-narrowing. | `sel-badge` / `fuzzy-finder` / `pane-manager` / `prompt-highlight` / `smooth-scroll` / `kakoune-bindings-demo` declare no explicit `on_state_changed_effects` block. Commit `964171ef`. |
| B-4-bind | `define_plugin!` `#[bind]` auto-bindings default to the tier-1 export. When the user writes no explicit handler block (or writes the tier-1 form), bindings emit inside `on_state_changed_tier1_effects` with body `KakouneSideEffects::default()`. An explicit legacy block keeps bindings on the legacy path for source compatibility. | `cursor-line` (only `#[bind]`, no explicit handler) migrated by macro change alone; its in-tree test that drove the legacy export was updated to drive the tier-1 entry point. |
| A-3d-mouse | `KakouneSideMousePreDispatchResult` enum mirrors `MousePreDispatchResult` with `Vec<KakouneSideCommand>` instead of `Vec<Command>`; `From` lift feeds the dispatch table. `HandlerRegistry::on_mouse_pre_dispatch_tier1` accepts the new enum (via `Into` bound), and `HandlerRegistry::on_handle_mouse_tier1` mirrors A-3d's other per-command-bound input setters for click handlers. | Three positive integration tests cover the `Pass`, `Consumed`, and click-handler lift paths. Existing legacy docstrings point at the new tier-1 siblings. |
| A-3d-mouse follow-up | Same pattern applied to `on_key_pre_dispatch` and `on_text_input_pre_dispatch`: new `KakouneSideKeyPreDispatchResult` (preserves `pending_buffer_edit` for the shadow-cursor commit channel) and `KakouneSideTextInputPreDispatchResult` (shares the `Pass`-without-payload variant with the broad type) plus matching `_tier1` setters. | Four additional positive integration tests cover both `Pass` and `Consumed` lift paths for both handlers, including a witness that the key handler's `pending_buffer_edit` round-trips through the lift unchanged. |
| A-3e  | `HandlerRegistry::on_command_error` and `HandlerRegistry::on_subscription` setters. Both handlers were previously only reachable via overriding the `PluginBackend` trait method, so the canonical Plugin-trait path had no way to register them. | `on_command_error` accepts `&PluginErrorEvent` and returns `Effects` through the same `dispatch_state_effect!` machinery as the lifecycle handlers. `on_subscription` mirrors the WIT `on-subscription(topic, values) -> runtime-effects` shape and now also takes `&AppView<'_>`. The exhaustive dispatch coverage test gains both handler names. |
| A-3e effect plumbing | Widen `PluginBackend::deliver_subscriptions` from `-> bool` (a flag no caller read) to `-> Effects` and add `app: &AppView<'_>`. Add `PluginEffects::evaluate_pubsub(&mut self, app) -> EffectsBatch` and route it from `state/update.rs` after `notify_state_changed`. PluginRuntime now owns the topic bus so oscillation history persists across frames without the caller threading a bus in. | Both the native `PluginBridge` and the WASM adapter now forward `on_subscription` effects up. A new test (`test_on_subscription_effects_flow_back_through_evaluate_pubsub`) witnesses a `redraw: BUFFER` flag traveling from a plugin's `on_subscription` handler into the dispatcher's `EffectsBatch`. |
| A-3g  | `#[deprecated(since = "0.7.1")]` on the seven legacy lifecycle setters (`on_init`, `on_session_ready`, `on_state_changed`, `on_io_event`, `on_update`, `on_process_task`, `on_process_task_streaming`) with notes pointing at the tier replacement. | In-tree test fixtures and the `#[kasane::plugin]` proc-macro emission gate the warnings with scoped `#[allow(deprecated)]`. Input setters (`on_key`, `on_text_input`, `on_drop`, `on_mouse_fallback`) stay un-deprecated because tier-1 there is a stricter opt-in, not the default. |
| B-5   | WIT 5.0.0 wire bump. `runtime-effects` record removed; the five `runtime-effects`-returning handler exports replaced with their ADR-mapped tier (`on-state-changed-effects` / `on-command-error-effects` / `on-subscription` → `kakoune-side-effects`; `on-io-event-effects` / `update-effects` → `process-capable-effects`). Transitional `on-state-changed-tier1-effects` parallel from B-2 collapsed into the renamed `on-state-changed-effects`. ABI 4.x rejected at load. SDK `Effects` alias becomes `ProcessCapableEffects`; `effects()` helper auto-routes a `Vec<Command>` into the tier-1 base + tier-2 process slots so existing tier-2 plugin bodies keep working. All 13 in-tree `.wasm` blobs (11 examples + `surface-probe` + `instantiate-trap` + `tier1-state`) rebuilt against SDK 5.0.0; manifests bumped. Migration guide at [`docs/migration/0.6-to-0.7.md` §8.3](migration/0.6-to-0.7.md). |

### Phase B-2 execution playbook (historical)

Retained as a reference for future ABI bumps; the corresponding work
landed in commit `2aca004d`.

The next focused session should execute Phase B-2 as a single PR.
Concrete steps, in order:

1. **WIT changes** (`kasane-wasm/wit/plugin.wit`):
   - Bump package version: `package kasane:plugin@4.1.0;` →
     `package kasane:plugin@4.2.0;`
   - Add export to `plugin-api` interface:
     ```wit
     /// Tier-1 state-changed handler (ADR-044). Returns tier-1 effects
     /// that the host lifts to the unified runtime-effects pipeline.
     /// Plugins that override this take precedence over
     /// `on-state-changed-effects` for the same dirty event.
     on-state-changed-tier1-effects: func(dirty-flags: u16) -> kakoune-side-effects;
     ```
   - Verify the new export reference forces wit-bindgen to generate
     Rust types for `kakoune-side-command` / `kakoune-side-effects`.

2. **Host abi.rs** (`kasane-wasm/src/abi.rs`):
   - `HOST_ABI_VERSION: &str = "4.1.0";` → `"4.2.0"`
   - The compatibility rule (`host minor ≥ plugin minor`) means
     4.1.0-built plugins remain loadable against 4.2.0 host **iff**
     bindgen synthesises a default-empty impl for new exports. Verify
     against the existing precedent of `on-command-error-effects`
     (ABI 4.0 → 4.1 in commit `858581db`).

3. **Host adapter** (`kasane-wasm/src/adapter.rs`):
   - Add `convert_kakoune_side_effects(effects: &wit::KakouneSideEffects) -> Effects`
     mirroring the existing `convert_runtime_effects`.
   - Add a dispatch site that calls `on_state_changed_tier1_effects`
     before / instead of `on_state_changed_effects`. Decide priority
     rule (recommend: tier1 export wins when both implemented).

4. **Convert layer** (`kasane-wasm/src/convert/command.rs`):
   - Add `wit_kakoune_side_command_to_command(&wit::KakouneSideCommand) -> Command`.
   - Add converter for `wit::KakouneSideEffects` → `core::Effects`,
     reusing the command converter.

5. **Rebuild all bundled and fixture WASMs**:
   - `kasane-wasm/bundled/*.wasm` (6 plugins: color-preview,
     cursor-line, fuzzy-finder, pane-manager, sel-badge,
     smooth-scroll)
   - `kasane-wasm/fixtures/*.wasm` (~10 fixtures including
     instantiate-trap, prompt-highlight, sel-badge, selection-algebra,
     session-ui, smooth-scroll, surface-probe, plus mirrors of
     bundled set)
   - Build incantation per plugin:
     ```bash
     cargo build --target wasm32-unknown-unknown --release \
       --manifest-path examples/wasm/$name/Cargo.toml
     cp examples/wasm/$name/target/wasm32-unknown-unknown/release/${name//-/_}.wasm \
        kasane-wasm/bundled/$name.wasm
     ```
   - Note: some plugins use `wasm32-wasip2` target (check each
     `Cargo.toml`'s `[profile.release]` / `[package]` settings).
   - Bump `abi_version` in each `.toml` manifest (bundled + fixtures)
     from `"4.1.0"` to `"4.2.0"`.

6. **Version-string sweep**:
   - `kasane/src/locked_wasm_provider.rs`, `kasane/src/plugin_cmd/*`,
     `kasane/src/plugin_lock.rs`, `kasane/src/plugin_store.rs`,
     `kasane-wasm/src/tests/discovery.rs`, `kasane-wasm/src/tests/mod.rs`
     — search for `4.1.0` and update where the host's expected
     version is referenced.

7. **Verify**:
   - `cargo build --workspace`
   - `cargo test --workspace` (in particular `cargo test -p kasane-wasm`
     — its discovery / lifecycle tests will exercise the new export
     path)
   - `cargo clippy --workspace -- -D warnings`

8. **Migration sequencing for B-3 / B-4** (deferred to next PRs after
   B-2 lands):
   - B-3: extend `kasane-plugin-sdk` to expose `KakouneSideEffects`
     / `KakouneSideCommand` on the guest side; extend
     `kasane-plugin-sdk-macros` so `define_plugin!` detects
     `fn on_state_changed -> (S, KakouneSideEffects)` and emits the
     tier1 export.
   - B-4: migrate `examples/wasm/*` plugins one at a time. The
     migration recipe is: replace `on_state_changed -> (S, Effects)`
     with `on_state_changed -> (S, KakouneSideEffects)`; replace
     `Command::*` constructions with `KakouneSideCommand::*`.
     Plugins that emit process commands stay on the legacy export
     until ADR-044 §A-3e (process-capable handler exports) lands.

The total B-2 size estimate is ~150 LoC of host code + WIT + manifest
edits + 10–15 rebuilt `.wasm` blobs. The 4.0 → 4.1 precedent
(`858581db`) is the closest analogue: 61 files, +123/−48 LoC.

### Remaining work

- ~~**A-3d-mouse**~~ — *Closed.* New `KakouneSideMousePreDispatchResult`
  enum mirrors `MousePreDispatchResult` with `Vec<KakouneSideCommand>`
  in place of `Vec<Command>`; `From<KakouneSideMousePreDispatchResult>
  for MousePreDispatchResult` provides the asymmetric lift.
  `HandlerRegistry` gains `on_mouse_pre_dispatch_tier1` (bound on
  `Into<KakouneSideMousePreDispatchResult>`) and `on_handle_mouse_tier1`
  (per-command bound, sibling of A-3d's other tier-1 input setters).
  Three positive integration tests in
  `kasane-core/src/plugin/handler_registry/input.rs` exercise both
  the `Pass` and `Consumed` lift paths plus the click-handler tier-1
  setter. Existing legacy setter docstrings now point at the new
  tier-1 sibling. The remaining un-tier-1'd pre-dispatch setters
  (`on_key_pre_dispatch`, `on_text_input_pre_dispatch`) follow the
  exact same pattern and can be added later if needed.
- ~~**A-3f leftovers**~~ — *Closed.* Survey of the four named
  candidates (`BuiltinDragPlugin`, `BuiltinFoldPlugin`,
  `SemanticZoomPlugin`, `WidgetPlugin`) plus all other in-tree
  `impl Plugin` sites confirmed none of them register against the
  seven deprecated lifecycle setters: BuiltinDragPlugin uses
  `on_mouse_pre_dispatch`, BuiltinFoldPlugin uses
  `on_navigation_action`, SemanticZoomPlugin uses
  `define_projection` + `on_key_map`, and WidgetPlugin uses the
  view-side handlers (`on_contribute`, `on_decorate_background`,
  `on_transform_for`, `on_decorate_gutter`, `on_decorate_inline`,
  `on_virtual_text`). The remaining-work entry was speculative;
  no plugin migration is required. Two stale module docstrings
  in `handler_registry/mod.rs` and `process_task.rs` were updated
  to use the tier-1 / tier-2 setters as the recommended example.
- ~~**B-4 leftovers**~~ — *Closed.* `define_plugin!` now routes
  `#[bind]` auto-bindings into the tier-1 export by default; only an
  explicit legacy `on_state_changed_effects` block keeps bindings on
  the legacy path. `cursor-line` migrated end-to-end (its `#[bind]`
  auto-binding now drives the tier-1 export, with the legacy export
  staying at the SDK no-op default). The remaining `examples/wasm/*`
  plugins declare no `on_state_changed_effects` block at all, so
  they have nothing to migrate.
- ~~**Phase B-5 / future ABI bump**~~ — *Closed.* Shipped on
  2026-05-11. WIT bumped to `kasane:plugin@5.0.0`; the five
  `runtime-effects`-returning handler exports now return their
  ADR-mapped tier (`on-state-changed-effects` /
  `on-command-error-effects` / `on-subscription` →
  `kakoune-side-effects`; `on-io-event-effects` / `update-effects` →
  `process-capable-effects`). The B-2 transitional
  `on-state-changed-tier1-effects` parallel was collapsed into the
  renamed `on-state-changed-effects`. WIT is a single canonical copy
  in `kasane-wasm/wit/plugin.wit` with `kasane-plugin-sdk{,-macros}`
  consuming it via symlink (the ADR's "triplicated" framing
  pre-dated B-1's symlink consolidation). All 13 in-tree `.wasm`
  blobs rebuilt against the new SDK; ABI 4.x rejected at load with
  a pointer to [`docs/migration/0.6-to-0.7.md`
  §8.3](migration/0.6-to-0.7.md).
- **`kak_lint!` / `KakCommand` tier flag (originally listed under
  Implications)** — *No-op.* `KakCommand` (in
  `kasane-plugin-sdk/src/kak_cmd.rs`) renders to Kakoune-side
  eval-command strings; its variant set
  (`DeclareUserMode` / `DefineCommand` / `Map` / `DeclareOption` /
  `Hook` / `Echo` / …) is structurally tier-1 by construction with
  no spawn-side variants, so a tier flag would always read tier-1.
  The ADR's Implications line was speculative; nothing to add.



**Tracked in:** [Issue #102](https://github.com/Yus314/kasane/issues/102).
Builds on the sprout-dogfooding tracker
[#81](https://github.com/Yus314/kasane/issues/81).

**Context:**

[#100](https://github.com/Yus314/kasane/issues/100) (Phase 0) and
[#101](https://github.com/Yus314/kasane/issues/101) (Phase 1) close the
**accidental silent drop** of process commands emitted from
`on_state_changed_effects`: the dispatcher now logs the drop reason
when source attribution is missing, and `EffectsBatch` preserves
per-plugin attribution across the multi-plugin merge so the dispatcher
sees the right `PluginId` for each command. The bug as reported by
sprout dogfooding can no longer ship silently.

Those two phases do not address a second, deeper question: **should
`on_state_changed_effects` be allowed to spawn processes at all?** The
sprout incident exposes three structural concerns that survive Phase
1's fix:

1. **Re-entrance risk.** `on_state_changed_effects` fires every time
   state changes. A handler that spawns a process from inside that
   handler creates a feedback loop bounded only by
   `MAX_COMMAND_CASCADE_DEPTH`. Phase 1 makes such loops *attributed*,
   not *prevented*.
2. **Performance.** State-changed is per-tick. Heavy commands
   (spawn, HTTP, EditBuffer) are not appropriate to issue per-tick by
   default — most plugins that try this are misusing the handler.
3. **Conceptual cleanness.** State-changed reads as "observe state and
   adjust output"; Kakoune-side write-back through `eval-command` is
   fine, but full process side effects feel like an anti-pattern.

The current `session-ready-command` WIT variant already encodes a
similar narrowing — at session ready, only `send-keys`, `eval-command`,
`paste-clipboard`, and `plugin-message` are allowed. That carve-out is
empirically motivated. This ADR generalizes the pattern: each handler
returns an effect type whose admissible command set encodes the
handler's contract, making misuse a **compile error** in both Rust
(host plugins) and WIT (WASM plugins) — not a runtime warning that
relies on attribution.

The vocabulary collision: the existing `KakouneSafeCommand` /
`KakouneSafeEffects` types ([ADR-030 Level 3 / Level 5
enforcement](./adr-030-observedpolicy-separation-staged-projection-rollout.md))
already use the word "Safe", but with a different meaning — *Kakoune
transparency* (the projection excludes `SendToKakoune`, `InsertText`,
`EditBuffer`). The new tier hierarchy uses "Safe" in the sense of
*re-entrance safety for high-frequency handlers* (no process spawn).
These are orthogonal cuts:

| Concern (existing ADR-030) | Concern (this ADR)                  |
|----------------------------|-------------------------------------|
| Does the command write to Kakoune state? | Does the command require source attribution / external process I/O? |
| `KakouneSafeCommand` excludes `SendToKakoune` / `InsertText` / `EditBuffer` | New `KakouneSafeCommand` excludes `SpawnProcess` / `WriteToProcess` / `HttpRequest` etc. |

Reusing the name across the two cuts will mislead readers and break
incremental migration tooling. This ADR therefore couples a small
**rename** of the ADR-030 types to the new tier rollout.

**Decision:**

Introduce a three-tier effect type hierarchy at the handler-return
boundary. Handlers' return types pick the tier that matches their
re-entrance / performance / conceptual budget. The tier admissible at
each handler is fixed by the framework; plugin authors cannot widen
it.

```rust
// Tier 0 — observation only. No side-effect commands.
//
// For handlers that observe state and report status (e.g.
// `on_workspace_changed`) without issuing follow-up actions.
pub struct ObservationEffects {
    pub redraw: DirtyFlags,
    pub scroll_plans: Vec<ScrollPlan>,
    pub state_updates: StateUpdates,
}

// Tier 1 — host-local + Kakoune-side commands.
//
// Allowed: SendKeys, EvalCommand, EditBuffer, PasteClipboard,
//          PluginMessage, RequestRedraw, InjectInput, SetConfig,
//          SetSetting, ScheduleTimer, CancelTimer,
//          RegisterThemeTokens, RegisterSurface/Unregister,
//          (full list defined by KakouneSideCommand variants).
//
// Excluded: SpawnProcess, WriteToProcess, CloseProcessStdin,
//           KillProcess, ResizePty, HttpRequest, CancelHttpRequest,
//           Session(_), SpawnPaneClient, ClosePaneClient,
//           StartProcessTask, Workspace(_).
//
// For re-entrance-safe high-frequency contexts.
pub struct KakouneSideEffects {
    pub base: ObservationEffects,
    pub commands: Vec<KakouneSideCommand>,
}

// Tier 2 — full effects, including external process and session
// management. Requires source attribution (the handler context
// produces it; type-system-enforced — the adapter layer fills
// `source: PluginId` from `self.plugin_id` so authors never touch
// the field).
pub struct ProcessCapableEffects {
    pub base: KakouneSideEffects,
    pub process_commands: Vec<ProcessCommand>,
}
```

**Command variant split:**

```rust
pub enum KakouneSideCommand {
    SendToKakoune(KasaneRequest),
    InsertText(String),
    EditBuffer(Vec<BufferEdit>),
    PasteClipboard,
    PluginMessage { target: PluginId, payload: Box<dyn Any + Send> },
    RequestRedraw(DirtyFlags),
    InjectInput(InputEvent),
    SetConfig { key: String, value: String },
    SetSetting { plugin_id: PluginId, key: String, value: SettingValue },
    ScheduleTimer { timer_id: u64, delay: Duration, target: PluginId, payload: Box<dyn Any + Send> },
    CancelTimer { timer_id: u64 },
    RegisterThemeTokens(Vec<(String, Style)>),
    RegisterSurface { surface: Box<dyn Surface>, placement: Placement },
    RegisterSurfaceRequested { surface: Box<dyn Surface>, placement: SurfacePlacementRequest },
    UnregisterSurface { surface_id: SurfaceId },
    UnregisterSurfaceKey { surface_key: String },
    BindSurfaceSession { surface_id: SurfaceId, session_id: SessionId },
    UnbindSurfaceSession { surface_id: SurfaceId },
    ExposeVariable { name: String, value: Value },
    SetStructuralProjection(Option<ProjectionId>),
    ToggleAdditiveProjection(ProjectionId),
    ProjectionOff,
    DismissDiagnosticOverlay,
    SetClipboard(String),
    TriggerPluginReload,
    Quit,
}

pub enum ProcessCommand {
    SpawnProcess { job_id: u64, program: String, args: Vec<String>, stdin_mode: StdinMode },
    WriteToProcess { job_id: u64, data: Vec<u8> },
    CloseProcessStdin { job_id: u64 },
    KillProcess { job_id: u64 },
    ResizePty { job_id: u64, rows: u16, cols: u16 },
    HttpRequest { job_id: u64, config: HttpRequestConfig },
    CancelHttpRequest { job_id: u64 },
    Session(SessionCommand),
    SpawnPaneClient { pane_key: String, placement: Placement },
    ClosePaneClient { pane_key: String },
    StartProcessTask { task_name: String },
    Workspace(WorkspaceCommand),
}
```

`From<KakouneSideCommand> for Command` and
`From<ProcessCommand> for Command` lift each tier into the
type-erased `Command` enum at the dispatcher boundary, so the
existing event loop pipeline is unchanged.

**Handler return-type mapping:**

| Handler                              | Returns                       | Rationale                                                  |
|--------------------------------------|-------------------------------|------------------------------------------------------------|
| `on_workspace_changed`               | `ObservationEffects`          | Read-only by design                                        |
| `on_state_changed_effects`           | `KakouneSideEffects`          | High-frequency; no process spawn                           |
| `on_active_session_ready_effects`    | `KakouneSideEffects`          | Already narrow at WIT; aligns the model                    |
| `on_command_error_effects`           | `KakouneSideEffects`          | Avoid command-error → error loops                          |
| `on_init_effects`                    | `KakouneSideEffects`          | Startup; narrow                                            |
| `on_subscription`                    | `KakouneSideEffects`          | Pub/sub callback                                           |
| `handle_key` / `handle_mouse`        | `ProcessCapableEffects`       | User-driven; spawn is appropriate                          |
| `handle_drop` / `handle_text_input`  | `ProcessCapableEffects`       | Same                                                       |
| `on_io_event_effects`                | `ProcessCapableEffects`       | Process chain spawns                                       |
| `update_effects`                     | `ProcessCapableEffects`       | Command-handler pattern                                    |

**ADR-030 type rename (resolving the "Safe" collision):**

The existing `KakouneSafeCommand` / `KakouneSafeEffects` /
`KakouneSafeKeyResult` types ([ADR-030
§Level-3/§Level-5](./adr-030-observedpolicy-separation-staged-projection-rollout.md))
are renamed to `KakouneTransparentCommand` /
`KakouneTransparentEffects` / `KakouneTransparentKeyResult`. The new
name names what the ADR-030 projection actually proves: the handler
cannot write to Kakoune state. "Transparent" matches the ADR-030
vocabulary (`Transparency` flag, `kakoune-safe-command` WIT variant
becomes `kakoune-transparent-command`).

The freed names are reused by this ADR — `KakouneSafeCommand` /
`KakouneSafeEffects` from #102 become `KakouneSideCommand` /
`KakouneSideEffects` to avoid lingering ambiguity (see Rationale §3
below). The freed `KakouneSafe*` namespace is left unused so a future
ADR can repurpose it without overloading either prior meaning.

**WIT-level split:**

```wit
// Tier 0
record observation-effects {
    redraw: u16,
    scroll-plans: list<scroll-plan>,
    state-updates: state-updates,
}

variant kakoune-side-command {
    send-keys(list<string>),
    eval-command(string),
    edit-buffer(list<buffer-edit>),
    paste-clipboard,
    plugin-message(message-config),
    request-redraw(u16),
    inject-input(input-event),
    set-config(config-entry),
    set-setting(setting-entry),
    schedule-timer(timer-config),
    cancel-timer(u64),
    register-theme-tokens(list<theme-token-default>),
    register-surface(register-surface-config),
    unregister-surface(u32),
    unregister-surface-key(string),
    expose-variable(variable-entry),
    set-structural-projection(option<u32>),
    toggle-additive-projection(u32),
    projection-off,
    dismiss-diagnostic-overlay,
    set-clipboard(string),
    quit,
}

// Tier 1
record kakoune-side-effects {
    base: observation-effects,
    commands: list<kakoune-side-command>,
}

variant process-command {
    spawn-process(spawn-process-config),
    write-to-process(write-process-config),
    close-process-stdin(u64),
    kill-process(u64),
    resize-pty(resize-pty-config),
    http-request(http-request-config),
    cancel-http-request(u64),
    session(session-cmd),
    spawn-pane-client(spawn-pane-client-config),
    close-pane-client(string),
    start-process-task(string),
    workspace-command(workspace-cmd),
}

// Tier 2
record process-capable-effects {
    base: kakoune-side-effects,
    process-commands: list<process-command>,
}
```

Handler imports / exports reference the appropriate type. WASM plugins
that try to return `process-command` from a Tier-1 export hit a
**wit-bindgen compile error** that points to this ADR.

**ABI version:** the WIT split is a backward-incompatible change to
the plugin contract. ABI version bumps from `4.1.0` → `5.0.0`
(see [`docs/abi-versioning.md`](abi-versioning.md)).

**Migration:**

*Native (kasane-core, in-tree plugins):*

1. Define the three tier types in `kasane-core/src/plugin/effects.rs`.
2. Define `KakouneSideCommand` / `ProcessCommand` enums with
   `From<…> for Command` lifts.
3. Rename ADR-030 types: `KakouneSafe*` → `KakouneTransparent*`.
4. Add tier-specific `Plugin::on_state_changed` /
   `Plugin::on_init` etc. signatures returning the right tier; the
   bridge converts each tier to the unified `Effects` for the
   internal dispatcher pipeline.
5. Migrate all in-tree backends (`BuiltinDragPlugin`,
   `BuiltinFoldPlugin`, `BuiltinShadowCursorPlugin`,
   `SemanticZoomPlugin`, etc.) handler by handler.

*WASM (kasane-wasm + examples/wasm/):*

1. WIT bump: write a new `kasane:plugin@5.0.0` package alongside the
   existing 4.1.0 (no in-place edit during transition).
2. Update `kasane-plugin-sdk` and `kasane-plugin-sdk-macros` to emit
   tier-typed bindings; `define_plugin!` infers tier per handler.
3. Migrate each `examples/wasm/*` plugin one at a time. Plugins that
   put `SpawnProcess` in `on_state_changed_effects` cannot be lifted
   blindly — the spawn must move to `handle_key` (a Tier-2 handler)
   or chain through `update_effects` from a Tier-2 message.
4. Migration guide at `docs/migration/0.X-to-0.Y.md` documents the
   rewrite patterns (handle_key intercept, pub/sub chain to
   `update_effects`).
5. Drop ABI 4.1.0 support after all in-tree plugins migrate.

*External (`Yus314/sprout`):*

Out of scope for this ADR but the migration the issue ties to. The
sprout picker moves from a Kakoune-side `set-option` →
`on_state_changed_effects` → `SpawnProcess` chain to a
plugin-internal `handle_key` state machine: the plugin captures
`r` / `l` / `n` directly when in its own "expecting pick kind"
sub-state, then spawns from `handle_key` (Tier 2 — admissible). The
user-mode infrastructure shifts from Kakoune-side to plugin-internal,
which also tightens UX (the plugin can re-prompt, show hints, etc.
without Kakoune round-trips).

**Re-entrance bonus:**

Tier 1 handlers structurally cannot emit process commands → cannot
trigger external I/O that cascades back into state-changed. The
`MAX_COMMAND_CASCADE_DEPTH` runtime guard becomes a backstop for
exotic cycles via `PluginMessage`, not the primary defence — the
common case is statically impossible.

**Rationale:**

1. **Mistakes become compile errors, not silent drops.** Phase 0
   logs the drop. Phase 1 routes attribution through the merge. Phase
   2/3 removes the misuse from the API surface entirely. A plugin
   author who writes `SpawnProcess` in `on_state_changed_effects`
   never gets a runtime error; they get a build error with a pointer
   to the migration path.

2. **The host stops being responsible for legality checks at the
   wrong layer.** Today, `handle_process_command` checks
   `command_source_plugin.is_some()` to gate spawn. After Phase 2/3,
   the type system enforces "can emit `ProcessCommand` ⇒ has
   `PluginId`" at the call site, and the dispatcher's `None`-branch
   becomes unreachable.

3. **Naming sharpens.** `KakouneSafe` meant two things; that hurts
   readers and migration tooling. Renaming the ADR-030 types to
   `KakouneTransparent` better describes what they actually witness
   — the inability to write Kakoune state. Naming Tier 1 as
   `KakouneSideEffects` instead of reusing `KakouneSafe` underlines
   that the tier admits *Kakoune-side* effects (writes through
   `eval-command`, key injection, redraw) rather than claims a
   blanket "safe" status. The two names then describe disjoint
   axes of the same `Command` variant space.

4. **Composition with `EffectsBatch`.** The Phase 1 batch shape (per-
   plugin commands; #101) extends cleanly: each plugin's per-handler
   tier output lifts into the shared `Command` Vec when the batch
   collects across plugins. No batch-layer changes needed.

**Alternatives considered:**

1. **Single `Effects` type, runtime check.** Leave the API as it is,
   add a debug-assert in the dispatcher when a Tier-1 handler tries
   to emit a process command. Rejected: the silent-drop bug existed
   for months precisely because there was no compile-time signal;
   runtime asserts have to be hit on the right code path, often in
   plugin code authors do not exercise.

2. **Tag commands instead of splitting the enum.** Mark each
   `Command` variant with a `tier()` method; reject at the dispatch
   boundary. Rejected: same runtime-shape problem as alt 1, plus the
   tag function gets out of sync with the actual variant list every
   time a new command is added.

3. **Two tiers (collapse Tier 0 into Tier 1).** `ObservationEffects`
   is structurally a `KakouneSideEffects` with empty commands.
   Rejected: read-only handlers (`on_workspace_changed`) benefit
   from a type signature that *refuses* to allocate a command vec —
   tells reviewers "this handler does not perform side effects" at
   the API surface.

4. **Keep the existing `KakouneSafeCommand` name; rename the new
   tier.** Less churn for ADR-030 consumers. Rejected because the
   reverse — renaming the existing types — produces clearer names
   on net: the existing types' "Safe" was always shorthand for
   "transparent in the ADR-030 sense", and ADR-030 already calls
   that property "Transparency".

5. **One mega-ADR covering Phase 0 + Phase 1 + Phase 2/3.**
   Rejected at the issue level. The phases are independently
   shippable; mega-ADRs collapse review cost and conflict
   resolution. Phase 0 / 1 already landed.

**Implications:**

- **Plugin authors (Rust host):** handler signatures change; the
  bridge converts tier output into the unified `Effects` internally,
  so consumers downstream of the bridge are unaffected.
- **Plugin authors (WASM):** WIT bindgen regen required.
  `define_plugin!` updated to emit per-handler tier types.
- **CHANGELOG:** Breaking-change entry on ABI 5.0.0 wire bump and
  Rust handler signature changes.
- **`docs/abi-versioning.md`:** new row for 5.0.0 with the tier split
  rationale.
- **`docs/plugin-api.md` / `docs/plugin-development.md`:** updated
  handler signature tables, migration cookbook section.
- **`docs/migration/0.X-to-0.Y.md`:** new migration entry covering
  the rewrite patterns and the `KakouneSafe* → KakouneTransparent*`
  rename.
- **`kak_lint!` / `KakCommand` (ADR-043):** updated to know which
  tier each command belongs to so structural builders refuse to
  produce a `ProcessCommand` for a Tier-1 context.
- **Roadmap:** the WIT split is an item under R2.x's compliance work;
  scheduling tracked in [Issue
  #102](https://github.com/Yus314/kasane/issues/102).
- **`kasane-plugin-sdk` / `kasane-plugin-sdk-macros`:** major bump.
- **`kasane-wasm`:** bridge layer updates per-handler import dispatch
  to the right tier export.
- The runtime `command_source_plugin.is_some()` guard in
  `handle_process_command` (added in Phase 0) becomes structurally
  unreachable, but kept as a defence-in-depth backstop with a
  `tracing::error!`.
