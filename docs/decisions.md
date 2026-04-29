# Architecture Decision Records (ADR)

This document is a historical record of the technical decisions made in Kasane, including subsequent updates and revocations.
For the current authoritative specification, refer to [semantics.md](./semantics.md) and each Current document.
The summary table in this chapter is a summary for current readers; each ADR body preserves the context at the time of decision. Where a subsequent ADR overrides an earlier one, the status field and notes in each section take precedence.

## Decision Summary (for current readers)

Legend: `Current` = still in effect, `Proposed` = future design. The Notes column indicates overrides by subsequent ADRs or implementation notes.

| Item | Status | Current Treatment | Notes |
|------|--------|-------------------|-------|
| Implementation language | Current | **Rust** | Performance and safety |
| Target platforms | Current | **Linux + macOS** | Kakoune's primary user base |
| Scope | Current | **Complete frontend replacement** | Replaces Kakoune's terminal UI, adds frontend-native capabilities |
| Rendering approach | Current | **TUI + GUI hybrid** | TUI for SSH/tmux, GUI for native window |
| TUI library | Current | **crossterm direct** | Full rendering control |
| GUI toolkit | Current | **winit + wgpu + Parley + swash** | cosmic-text + glyphon retired in [ADR-031](#adr-031-text-stack-migration--cosmic-text--parley--swash-with-protocol-style-redesign). Window/GPU layer unchanged. Original ADR-014 selection of glyphon is superseded for the text stack only. |
| Configuration format | Current | **Unified KDL + ui_options** | Single `kasane.kdl` for config + widgets. Supersedes ADR-003 (TOML + separate widgets.kdl) |
| Crate structure | Current | **Cargo workspace** | `kasane-core` / `kasane-tui` / `kasane-gui` / `kasane` / `kasane-macros` / `kasane-wasm` / `kasane-wasm-bench` |
| Kakoune version | Current | **Latest stable only** | Leverages new protocol features |
| kak-lsp integration | Current | **Pure JSON UI frontend** | No special handling for kak-lsp |
| Development environment | Current | **Nix flake + direnv** | Reproducible development environment |
| Async runtime | Current | **Synchronous + threads** | Compatible with backend / event loop |
| Kakoune process management | Current | **Child process spawn + session connection** | Supports `-c` / `-s` |
| Unicode width calculation | Current | **unicode-width + compatibility patches** | Corrects Kakoune mismatch cases |
| Error handling | Current | **anyhow + thiserror** | Structured in core, aggregated in bin |
| Logging | Current | **tracing + file output** | Filter control via `KASANE_LOG` |
| Testing strategy | Current | **Unit + snapshot + property-based tests** | Combined use of `insta` and `proptest` |
| CI/CD | Current | **GitHub Actions + Nix** | Build / test / lint on Linux/macOS |
| Rust edition | Current | **Edition 2024 / no MSRV** | Toolchain pinned via Nix |
| JSON parser | Current | **simd-json** | serde-compatible API |
| License | Current | **MIT OR Apache-2.0** | Standard Rust dual license |
| Declarative UI | Current | **Element tree + TEA** | Details in [ADR-009](#adr-009-declarative-ui-architecture--transition-to-plugin-infrastructure) |
| Plugin execution model | Current | **WASM Component Model as first choice, native proc-macro path coexists** | The native-only assumption of 9-2 was updated by [ADR-013](#adr-013-wasm-plugin-runtime--component-model-adoption) |
| Element memory | Current | **Owned** | No lifetimes |
| State management | Current | **TEA (The Elm Architecture)** | Unidirectional data flow |
| Plugin extension | Current | **Slot + Decorator + Replacement** | Three-tier extension mechanism |
| Layout | Current | **Flex + Overlay + Grid** | Basic layout + layering + tabular |
| Event propagation | Current | **Central dispatch + InteractiveId** | Keys centralized, mouse uses hit test |
| Compiler-driven optimization | Current | **Salsa incremental computation + SceneCache (GPU)** | ViewCache/PaintPatch superseded by Salsa (ADR-020) |
| CLI design | Current | **kak drop-in replacement** | Non-UI flags delegated via exec |
| Three-layer responsibilities | Current | **Upstream / Core / Plugin** | Criteria in [ADR-012](#adr-012-layer-responsibility-model) |
| WASM plugin runtime | Current | **Component Model (wasmtime)** | Detailed performance figures in [ADR-013](#adr-013-wasm-plugin-runtime--component-model-adoption) and [performance.md](./performance.md) |
| Pipeline equivalence testing | Current | **Trace-Equivalence axiom + proptest** | Current harness generates DirtyFlags at coarse granularity |
| SurfaceId-based invalidation | Proposed | **Per-surface dirty / cache design** | For multi-pane, not yet implemented |
| Plugin I/O infrastructure | Current | **Hybrid model (WASI direct + host-mediated)** | Design foundation for Phase P. Details in [ADR-019](#adr-019-plugin-io-infrastructure--hybrid-model) |
| Salsa incremental computation | Current | **Stage 1 (Salsa tracked) + Stage 2 (imperative plugins)** | Mandatory dependency (feature flag removed). Details in [ADR-020](#adr-020-salsa-incremental-computation--stage-12-split) |
| Plugin trait naming | Current | **`Plugin` (state-externalized, primary) + `PluginBackend` (mutable, internal)** | Renamed from `PurePlugin`/`Plugin`. Details in [ADR-022](#adr-022-plugin-trait-rename--pureplugin--plugin-plugin--pluginbackend) |
| Session management boundaries | Current | **Mechanism (core) / Policy (plugin) split** | Session lifecycle in core; session UI in plugins. Details in [ADR-023](#adr-023-session-management-boundaries--mechanism--policy-split) |
| Display transformation | Current | **DisplayMap + DisplayDirective** | Plugin-declared directives (Fold/InsertAfter/Hide) → core builds O(1) bidirectional mapping. Single-plugin constraint initially. Virtual text proof artifact in `examples/virtual-text-demo/`. Kakoune viewport control limits true folding |
| Performance policy | Current | **Three-layer perceptual framework** | Perceptual compass + engineering ratchets + optimization accountability. Details in [ADR-024](#adr-024-perception-oriented-performance-policy) |
| Plugin registration model | Current | **HandlerRegistry + Plugin trait (2 methods + 1 associated type)** | Plugins register handlers declaratively; capabilities auto-inferred. Details in [ADR-025](#adr-025-handlerregistry-plugin-architecture) |
| Declarative transforms | Current | **ElementPatch algebra** | Composable, normalizable, Salsa-memoizable. Custom escape hatch for imperative transforms. Details in [ADR-026](#adr-026-elementpatch-declarative-transforms) |
| Annotation decomposition | Current | **4 annotation extension points + render_ornaments** | Gutter, background, inline, virtual text (annotation), plus render_ornaments (physical decoration). Details in [ADR-027](#adr-027-lineannotation-decomposition) |
| WASM capability inference | Current | **`register-capabilities` WIT export** | WASM plugins declare capabilities as a bitmask; host skips non-participating dispatch. Details in [ADR-028](#adr-028-wasm-capability-inference) |
| Inter-plugin communication | Current | **Topic-based pub/sub + plugin-defined extension points** | Two-phase evaluation with cycle prevention; typed extension points with composition rules. Details in [ADR-029](#adr-029-topic-based-pubsub-and-plugin-defined-extension-points) |
| GPU rendering strategy | Proposed | **Vello evaluation framework (spike + trait abstraction)** | Re-evaluation of [ADR-014](#adr-014-gui-technology-stack--winit--wgpu--glyphon) §14-1 in light of 2026 Q1 changes (Glifo, Vello Hybrid). Details in [ADR-032](#adr-032-gpu-rendering-strategy--vello-evaluation-framework). |

## ADR-001: Rendering Approach — TUI + GUI Hybrid

**Status:** Decided

**Context:**
Four options were evaluated as the rendering approach for Kasane: TUI (in-terminal), GUI (native window), GPU-embedded terminal, and TUI + GUI hybrid.

**Evaluation of options:**

| Approach | Resolvable Issues | MVP Timeline | SSH/tmux |
|----------|-------------------|-------------|----------|
| TUI (Kitty-based) | ~71/80 | ~2 months | Supported |
| GUI | ~80/80 | ~4-5 months | Not supported |
| GPU-embedded terminal | ~80/80 | ~5-6 months | Not supported |
| TUI + GUI hybrid | TUI: ~71 / GUI: ~80 | TUI: ~2 months | TUI: Supported |

**Decision:** Adopt the TUI + GUI hybrid approach.

**Rationale:**
- Maintaining SSH/tmux workflows is necessary → TUI backend is required
- GUI benefits (subpixel rendering, D&D, font size adjustment, etc.) are also desired → GUI backend is needed
- Abstract core logic via the `RenderBackend` trait, making TUI and GUI interchangeable
- Release MVP quickly with TUI, add GUI backend in Phase 4

## ADR-002: TUI Library — crossterm Direct

**Status:** Decided

**Context:**
Three options were evaluated as the TUI backend library: ratatui + crossterm, crossterm direct, and termwiz.

**Evaluation of options:**

| Library | Dev Speed | Performance | GUI Abstraction Compatibility |
|---------|-----------|-------------|-------------------------------|
| ratatui + crossterm | Fastest | Medium (framework constraints) | Medium |
| crossterm direct | Slow | Best (full control) | High |
| termwiz | Moderate | High | Medium |

**Decision:** Adopt crossterm direct.

**Rationale:**
- Enables custom optimization of the cell grid diff rendering algorithm
- Facilitates abstraction with the GUI backend — cell grid diff computation can be placed in core
- Avoids ratatui's widget rebuild overhead
- Aligns with the performance-focused design philosophy

**Trade-offs:**
- Border drawing, popup clipping, and layout computation all need custom implementation
- Cost of reimplementing ~2,000–3,000 lines of code that ratatui provides

## ADR-003: Configuration Format — TOML + ui_options Combined

**Status:** Superseded — migrated to unified KDL (`kasane.kdl`) for both config and widgets. The ui_options dynamic channel remains.

**Context:**
Three formats plus a combination were evaluated for configuration: TOML, KDL, Kakoune commands only (ui_options only), and TOML + ui_options combined.

**Decision:** Adopt TOML + ui_options combined.

**Rationale:**
- **TOML (static config):** `~/.config/kasane/config.toml` — theme, font, GUI settings, default behavior. Type-safe deserialization via `serde`
- **ui_options (dynamic config):** Kakoune `set-option global ui_options kasane_*=*` — UI behavior that can be changed at runtime. Can be combined with Kakoune hooks and conditionals
- Achieves both type-safe static configuration and dynamic configuration integrated with Kakoune

**Update:** Configuration and widget definitions are now unified in a single `~/.config/kasane/kasane.kdl` file using KDL v2 syntax. The dual-file system (`config.toml` + `widgets.kdl`) has been retired. The ui_options dynamic channel is unchanged.

## ADR-004: kak-lsp Integration — Pure JSON UI Frontend

**Status:** Decided

**Context:**
kak-lsp makes heavy use of info/menu and thus benefits most from Kasane's floating windows. The question was whether to provide special handling specific to kak-lsp.

**Decision:** As a pure JSON UI frontend, no kak-lsp-specific handling is provided.

**Rationale:**
- Protocol compliance alone naturally provides the main benefits (scrollable popups, placement customization, borders)
- Depending on kak-lsp implementation details risks breakage on version upgrades
- Maintains fairness with other plugins (parinfer.kak, kak-tree-sitter, etc.)
- Future integration via `ui_options` can be considered if needed

## ADR-004A: Standard Frontend Compatibility as the Primary Constraint

**Status:** Decided

**Context:**
Kasane on one hand aims for existing Kakoune users to adopt it seamlessly as `kak = kasane`, and on the other hand wants to provide plugin authors with powerful UI extension capabilities. Trying to satisfy both at the same layer risks eroding either standard user compatibility or plugin platform freedom.

**Decision:** As a product, Kasane treats standard frontend compatibility as the primary concern, with plugin platform capabilities layered on top. That is, Default Frontend Semantics are the primary constraint, and Extended Frontend Semantics are positioned as additional capabilities.

**Concrete principles:**
- `kak = kasane` means semantic compatibility, not bitwise-identical UI
- In the default state, compatibility with existing `kakrc`, autoload, existing plugins, and existing workflows is prioritized
- Kasane-specific plugins, surfaces, and restructured UI are added value, not prerequisites for normal use
- Plugin-defined UI does not falsify protocol truth; it participates in core semantics as display policy
- Strong restructuring or observed-eliding transformations belong to opt-in extended semantics

**Rationale:**
- For broad adoption in the Kakoune community, low adoption friction is more important than advanced features
- For existing users, the value lies in improving the UI without breaking existing workflows, rather than joining a new ecosystem
- If plugin platform is the product's primary concern, bundled plugins and a proprietary ecosystem tend to erode standard frontend semantics
- Making the Default/Extended two-tier explicit allows maintaining conservative defaults and strong extensibility simultaneously

## ADR-005: Development Environment Management — Nix flake + direnv

**Status:** Decided

**Context:**
A consistent environment for the Rust toolchain (rustc, cargo, clippy, rustfmt) and system-dependent libraries (various C libraries used by crossterm, Phase 4 wgpu dependencies, etc.) needed to be provided across developers.

**Decision:** Manage the development environment with `flake.nix` + `.envrc` (`use flake`).

**Rationale:**
- `nix develop` / `direnv allow` provides the toolchain and dependency libraries in one step
- `flake.lock` guarantees build reproducibility
- A single `flake.nix` can support both macOS (darwin) and Linux platforms
- Using the same Nix environment in CI avoids "works locally but fails in CI" problems
- The Rust toolchain is managed via `rust-overlay` or `fenix`, kept consistent with `rust-toolchain.toml`

## ADR-006: Async Runtime — Synchronous + Threads

**Status:** Decided

**Context:**
Kasane has 5 I/O streams: (1) Kakoune stdout reading, (2) crossterm input event reception, (3) Kakoune stdin writing, (4) terminal output, and (5) timers. The question was how to handle these concurrently.

**Evaluation of options:**

| Approach | Implementation Cost | crossterm Compatibility | Binary Size | Debuggability |
|----------|--------------------|-----------------------|-------------|---------------|
| Synchronous + threads | Low | Best | Smallest | High |
| tokio | Medium | Medium (EventStream spawns a separate thread internally) | +1-2MB | Medium |
| polling / mio direct | High | Low (dual management with crossterm) | Smallest | Medium |

**Decision:** Adopt synchronous + threads.

**Rationale:**
- crossterm's `read()` is a synchronous blocking API, more reliable than the async `EventStream`
- Kasane's I/O pattern is simply merging 3 streams, making most of tokio's features unnecessary
- Helix, Alacritty, and Zellij also use similar thread-based architectures for input processing
- `std::sync::mpsc` or `crossbeam-channel` for inter-thread message passing
- Timers realized via `crossbeam-channel::select!` timeout

## ADR-007: Kakoune Process Management — Child Process Spawn + Session Connection

**Status:** Updated (daemon separation)

**Context:**
The question was how Kasane should launch and manage Kakoune.

**Decision:** By default, separate the Kakoune server into a headless daemon (`kak -d`) and connect the primary client via `-c`, matching pane clients. The `-c` option continues to support connection to an externally managed daemon session.

**Startup patterns:**
- `kasane file.txt` → spawns daemon `kak -d -s kasane-<pid> file.txt` + client `kak -ui json -c kasane-<pid>`
- `kasane -s myses file.txt` → spawns daemon `kak -d -s myses file.txt` + client `kak -ui json -c myses`
- `kasane -c mysession` → connects to existing daemon session via `kak -ui json -c mysession` (no daemon spawned)

**Rationale:**
- Kakoune's daemon mode (`kak -d -s` / `kak -c`) is an important multi-client workflow
- Not supporting `-c` would be a major limitation for Kakoune users
- JSON UI connection uses a `kak -ui json -c` process for both new and existing sessions, so the pipe mechanism is identical
- Daemon separation ensures that `:q` on the primary pane produces an EOF on its stdout, so `KakouneDied` fires correctly in multi-pane configurations. Without separation, the co-located server keeps stdout open even after the client portion exits

## ADR-008: JSON Parser — simd-json

**Status:** Decided

**Context:**
`draw` messages deliver JSON with rows × atoms per frame, so parser performance directly impacts rendering latency (NF-001: under 16ms).

**Decision:** Adopt simd-json.

**Rationale:**
- High-speed parsing leveraging SIMD instructions (SSE4.2/AVX2/NEON)
- serde-compatible API (same `Deserialize` derive as `serde_json`) for type-safe deserialization
- `draw` messages can be large JSON containing tens to hundreds of atoms, making parser performance differences more apparent
- Fallback to `serde_json` is easy if needed (API compatible)

## ADR-009: Declarative UI Architecture — Transition to Plugin Infrastructure

**Status:** Decided

**Context:**
Transform kasane from a mere Kakoune frontend into a UI infrastructure for plugin authors. Prioritize extensibility and configurability over direct feature delivery. Migrate from an imperative rendering pipeline to a declarative Element tree base.

**Decision:** Adopt the following 7 design decisions as a package.

See [plugin-development.md](./plugin-development.md) for detailed design.

### 9-1: Protocol Coupling — Kakoune-specific

**Status:** Revoked (originally decided as "gradual decoupling." Reconfirmed that Kasane is a Kakoune-specific UI infrastructure, and generalization is out of scope)

**Decision:** Design with tight coupling to the Kakoune protocol. No decoupling into a general-purpose UI infrastructure.

**Rationale:**
- Kasane is a UI infrastructure for Kakoune plugin authors; generalization to other editors is out of scope
- Unnecessary abstraction increases code complexity and degrades the Kakoune plugin developer experience
- Specializing in Kakoune's JSON UI protocol enables optimal design decisions

### 9-2: Native Plugin Development Path — trait + proc macro

**Status:** Partially updated (the first choice for runtime loading is WASM per [ADR-013](#adr-013-wasm-plugin-runtime--component-model-adoption). The native path itself remains current)

**Decision:** Native plugins are implemented as Rust crates. Direct implementation of the `Plugin` trait is maintained as the primary path, while `#[kasane::plugin]` / `#[kasane::component]` proc macros are used alongside for boilerplate reduction and verification assistance.

**Rationale:**
- Maximum type safety. Invalid Msg sends cause compile errors
- Zero-cost abstraction. No runtime overhead due to monomorphization
- Proc macro benefits: compile-time structural validation, boilerplate reduction, layout optimization (Svelte-like approach)
- Plugins distributable via the Rust ecosystem (crates.io, semver)

**Trade-offs:**
- Rebuilding required to add plugins. Users need a Rust toolchain
- Plugin authors need to write Rust

**Subsequent updates:**
- [ADR-013](#adr-013-wasm-plugin-runtime--component-model-adoption) added the WASM Component Model, and the recommended distribution path is now WASM
- The native path continues for registration via `kasane::run()`, full access to `&AppState`, and features such as `Surface`
- Hook parity of the `#[kasane_plugin]` macro is being expanded incrementally; currently some hooks still require direct trait implementation
- [ADR-022](#adr-022-plugin-trait-rename--pureplugin--plugin-plugin--pluginbackend) renamed the traits: the `Plugin` trait referenced above is now called `PluginBackend` (internal), and the primary user-facing trait is the new `Plugin` (state-externalized, formerly `PurePlugin`)

### 9-3: Element Memory Model — Owned

**Decision:** `Element` has no lifetime parameters and owns all its data.

**Rationale:**
- Lifetimes do not propagate throughout the API. Lowest cognitive load for plugin authors
- No lifetime insertion needed in proc macro generated code
- Ownership transfer allows free transformation when Decorators receive and process Elements
- TUI Element trees are small (20-50 nodes), and clone cost is in the microsecond range, negligible

**Trade-offs:**
- Data copies from State occur (not zero-copy)
- Mitigated by Svelte-like optimization via proc macros (direct rendering bypassing the Element tree)

### 9-4: State Management — TEA (The Elm Architecture)

**Decision:** Adopt global TEA + per-plugin nested TEA.

**Rationale:**
- The existing `AppState::apply(KakouneRequest)` is already TEA-like. Low migration cost
- The Kakoune protocol itself is TEA-like (Kakoune→Frontend: Msg, Frontend→Kakoune: Command)
- Aligns with Rust's ownership model (`&State` for view, `&mut State` for update)
- Plugins have their own State/Msg/update/view, composed by the framework. No inter-plugin interference
- High testability. update() is testable as a pure function
- Component-local state is fundamentally incompatible with Rust's borrowing rules

### 9-5: Plugin Extension Model — Slot + Decorator + Replacement

**Decision:** Provide all three tiers of extension mechanisms.

- **Slot:** Insert Elements at predefined extension points
- **Decorator:** Receive and wrap existing Elements
- **Replacement:** Completely replace existing components

**Rationale:**
- Slots alone provide insufficient extensibility (extensions not anticipated by the framework are impossible)
- Decorators enable extending existing elements (adding line numbers, changing borders, etc.)
- Replacements enable fundamental UI changes (replacing menus with fzf-style, etc.)
- Having levels of freedom allows plugin authors to choose the appropriate level

**Risk mitigation:**
- Decorator application order managed via priority + user settings
- Replacement targets limited to components with low risk of protocol inconsistency
- Explicit opt-in for Replacement (something like an `#[unsafe_replace]` marker) is being considered

**Three-tier composition rules:**
- When a Replacement is registered for a target, the default Element construction is skipped and the Replacement's Element is used
- Decorators are applied even to Replacement output. Replacements handle content substitution, Decorators handle styling (borders, shadows, etc.), achieving separation of concerns. This allows theme plugins (Decorator) and custom menu plugins (Replacement) to coexist naturally
- Decorators must not assume the internal structure of the Element they receive (since the structure may change due to Replacement composition). Only the pattern of wrapping the Element in a Container as-is is safe
- Ignoring the input Element in a Decorator and returning an entirely different Element is discouraged as it overlaps with Replacement's intent. If substitution is the goal, Replacement should be used

**Key event routing:**
- No explicit focus concept; all plugins' `handle_key()` are queried in priority order
- Each plugin refers to `AppState` to self-determine whether it should handle the event (e.g., a Menu Replacement plugin processes when `state.menu.is_some()`)
- Aligns with TEA principles (state is the source of truth), avoiding the complexity of implicit focus state transitions
- See the event propagation section in [plugin-development.md](./plugin-development.md) for details

### 9-6: Layout Model — Flex + Overlay + Grid

**Decision:** A hybrid model with a simplified Flexbox as the base, plus Stack/Overlay and Grid.

**Rationale:**
- Flexbox (Direction + flex-grow + min/max) can express nearly all TUI layouts
- Overlay is essential for Kakoune's menu/info popup positioning (compute_pos). Flexbox alone cannot express layering
- Grid is needed for tabular formats such as column alignment in completion menus
- Constraint-based (Cassowary) is overkill for TUI. Ratatui has precedent moving from constraint-based to a Flexbox-like approach
- Computable in O(n). Can be implemented incrementally (first Flex, then Overlay, finally Grid)

### 9-7: Event Propagation — Hybrid (Central Dispatch + InteractiveId)

**Decision:** Key events are centralized in TEA's update(). Mouse events use InteractiveId attached to Elements for hit testing, then pass the identified target to update().

**Rationale:**
- In kasane, most key inputs are forwarded to Kakoune. "Default behavior for most, exceptional plugin handling" is optimal for central dispatch
- Elements remain pure data structures without closures (consistent with Owned Elements)
- The framework automatically performs mouse hit testing using layout results, so plugins need no coordinate calculations
- InteractiveId is lightweight (enum or integer) with natural Clone/Debug/PartialEq implementations

## ADR-010: Compiler-Driven Optimization — Svelte-like Two-Layer Rendering

**Status:** Partially implemented, partially under continued research

**Context:**

Svelte's design philosophy is summarized as "the framework is not shipped. The compiler is shipped." It compiles components into efficient imperative code that surgically updates the DOM, eliminating virtual DOM diffing costs. The question was how to incorporate this philosophy into kasane's declarative UI plan (ADR-009).

**Analysis: TEA vs Svelte-like Reactivity**

TEA's model is "State change → view() rebuilds entire Element tree → layout → paint → CellGrid → diff → terminal." Svelte's model is "State change → compiler-generated code directly updates only the changed nodes."

kasane's Element tree is extremely small at 20-50 nodes, orders of magnitude different from web UI's thousands of nodes. Performance analysis shows view() + layout() cost totals ~2 μs (0.1% of frame time), with the bottleneck being terminal I/O (~1,500 μs, 75%). The problem Svelte aims to solve (virtual DOM diffing cost) does not exist in kasane.

Furthermore, Rust's ownership model naturally aligns with TEA (`&State` for view, `&mut State` for update). Component-local state is fundamentally incompatible with Rust's borrowing rules; importing Signals/Runes would result in a storm of `Cell<T>` / `RefCell<T>` / `Rc<T>`, undermining Rust's static safety.

**Decision:** Maintain TEA as the runtime model, and adopt a "two-layer rendering" approach that incrementally introduces optimizations achievable through proc macros (`#[kasane::component]`) and policy-driven cache / patches.

**Adopted:**

| Concept | Implementation Approach | Timing |
|---------|------------------------|--------|
| Compile-time dependency analysis | Proc macro analyzes view() AST to identify input parameters each Element depends on | Phase 2 |
| Static layout cache | Calculate layout once for parts whose structure does not depend on input | Phase 2 |
| Fine-grained update code generation | Per-Element dependency tracking to directly update only changed cells in CellGrid | Phase 2 |
| Two-layer rendering model | Compiled path (proc macro generated) + interpreter path (generic Element tree) | Phase 2 |

**Not adopted:**

| Concept | Reason |
|---------|--------|
| Component-local state | Incompatible with Rust's borrowing rules. TEA's central state management is optimal for Rust |
| Signals / Runes | Undermines Rust's static safety with `Cell<T>` / `RefCell<T>`. TEA's `&T` / `&mut T` is superior |
| JSX / template syntax | Poor IDE support, unclear error messages. Rust's builder pattern is better for type checking and completion |
| `$derived` (derived state) | Manual is sufficient. Formalizing it greatly increases proc macro complexity |

**Two-layer rendering model:**

```
                  +---------------------+
                  |   Declarative API   |  ← what plugin authors interact with
                  |  (Element, view())  |
                  +------+--------------+
                         |
             +-----------+----------+
             v                      v
  +------------------+   +----------------------+
  | Compiled path    |   | Interpreter path     |
  | (proc macro gen) |   | (generic Element     |
  |                  |   |  tree)               |
  | Static structure |   | Element → layout()   |
  |  → direct        |   |  → paint() → CellGrid|
  |   CellGrid update|   |                      |
  +------------------+   +----------------------+
    ^ #[kasane::component]    ^ Plugin::contribute()
    ^ static parts of         ^ dynamic Slot/Decorator/Replacement
      core_view
```

- **Compiled path**: Parts that `#[kasane::component]` can statically analyze. Updates CellGrid directly, bypassing the Element tree. Same structure as Svelte compiling results to imperative code
- **Interpreter path**: Parts where plugins dynamically provide Elements. The full Element → layout → paint path. Always present as a correctness guarantee
- **Fallback safety**: Code written without `#[kasane::component]` runs via the interpreter path. Optimization is opt-in; the interpreter path guarantees correctness

**Rationale:**
- Svelte's true benefit is not "changing the runtime model" but the philosophy of "letting the compiler do the work"
- Positioned as a natural extension of ADR-009's proc macro plan (9-2)
- Achieves the same duality as Svelte: maintaining a declarative API while making execution code imperative
- Comes into its own as plugins increase from Phase 2 onward. Only design considerations in Phase 1, no implementation

**2026-03 note:** The "two-layer rendering" in this section is the name for the overall vision. Stages 1-4 below were originally implemented with manual caching (ViewCache, PaintPatch, etc.), but have since been **superseded by Salsa incremental computation** (ADR-020). `SceneCache` remains as a GPU-path auxiliary cache. The historical implementation record is preserved below for reference.

### Implementation Record

> **Superseded by ADR-020.** ViewCache, ComponentCache, LayoutCache, and PaintPatch have been removed. Salsa is now the sole caching layer for element tree construction and layout. SceneCache remains for GPU-path DrawCommand reuse.

Original 4 stages: (1) DirtyFlags-based view memoization, (2) verified dependency tracking via `#[kasane::component(deps(...))]`, (3) SceneCache for DrawCommand-level caching, (4) compiled PaintPatch with StatusBarPatch / MenuSelectionPatch / CursorPatch.

### Implementation Status (Historical)

#### Stage 1: DirtyFlags-Based View Memoization — Superseded by Salsa

| Metric | Value |
|---|---|
| view() cost | 5.0 us (0 plugins) / 10.4 us (10 plugins) |
| Implementation | ~~ViewCache, ComponentCache\<T\>~~ → Salsa tracked functions. DirtyFlags u16, MENU→MENU_STRUCTURE+MENU_SELECTION split retained |
| Result | view() sections skipped entirely when Salsa inputs are unchanged (PartialEq early-cutoff) |

#### Stage 2: Verified Dependency Tracking — Superseded by Salsa

| Metric | Value |
|---|---|
| Implementation | ~~`#[kasane::component(deps(FLAG, ...))]` proc macro, AST-based field access analysis, FIELD_FLAG_MAP~~ → Salsa structural dependency tracking |
| Note | `#[kasane::component]` now validates purity only (return type + no &mut). Deps/field-access analysis removed |

#### Stage 3: SceneCache (DrawCommand-Level Caching) — Active (GPU only)

| Metric | Value |
|---|---|
| Implementation | Per-section DrawCommand caching (base, menu, info) |
| Invalidation | DirtyFlags-based: BUFFER\|STATUS\|OPTIONS→base, MENU→menu, INFO→info |
| GPU benefit | Cursor-only frames reuse cached scene (0 us pipeline work) |
| Cold/Warm ratio | 22.8 μs cold → 7.0 μs warm (3.3x speedup) |

#### Stage 4: Compiled Paint Patches — Superseded by Salsa

| Metric | Value |
|---|---|
| ~~StatusBarPatch~~ | Removed — Salsa handles status section memoization |
| ~~MenuSelectionPatch~~ | Removed — Salsa handles menu section memoization |
| ~~CursorPatch~~ | Removed — Salsa handles cursor-related memoization |
| ~~LayoutCache~~ | Removed — Salsa handles layout memoization |

#### Overall Result

Salsa incremental computation replaced the manual multi-layer caching. The pipeline relies on Salsa's automatic dependency tracking and PartialEq early-cutoff for memoization, with SceneCache providing an additional GPU-specific optimization layer.

### Component Macro Details

The `#[kasane::component]` macro follows Svelte's "let the compiler do the work" philosophy, progressively generating optimized code from declarative `view()` functions:

**Stage 1: Input Memoization** — Retains previous input parameter values and skips Element construction when all inputs are identical:

```rust
#[kasane::component]
fn file_tree(entries: &[Entry], selected: usize) -> Element { ... }
// → If entries and selected are unchanged, returns cached Element
```

**Stage 2: Static Layout Cache** — The proc macro detects structurally static parts and calculates layout only once.

**Stage 3: Fine-Grained Update Code Generation** — The proc macro statically analyzes each Element's input parameter dependencies at the AST level and generates code that directly updates only the changed cells in CellGrid.

**Two-Layer Rendering Model:**

```
              +---------------------+
              |  Declarative API    |  ← Plugin authors work here
              |  (Element, view())  |
              +------+--------------+
                     |
         +-----------+----------+
         v                      v
  Compiled path          Interpreter path
  (proc macro gen)       (generic Element tree)
         |                      |
  Static structure →     Element → layout()
    direct CellGrid        → paint() → CellGrid
    update
```

- **Compiled path**: Parts that `#[kasane::component]` can statically analyze. Updates CellGrid directly, bypassing the Element tree.
- **Interpreter path**: Parts where plugins dynamically provide Elements via `Plugin::contribute()`. Uses the full pipeline.
- **Fallback**: Code without `#[kasane::component]` runs through the interpreter path. Optimization is opt-in; correctness is always guaranteed by the interpreter path.

**Stage 5: Compiled rendering for plugins (design analysis)**

(Status: Analysis ongoing. Plugins themselves already exist and L1-equivalent caching is implemented. Partial layout / automatic patch generation for generic plugin views is not yet implemented)

*Problem redefinition:*

Built-in views (StatusBar, Menu, Info, Buffer) are finite in number with known structure, so hand-written PaintPatches are sufficient for optimization. Compiler-driven auto-generation becomes necessary for **plugin authoring** — as the number of plugins increases, individual manual optimization does not scale. Requiring plugin authors to hand-write PaintPatches is unrealistic.

*Auto-generation approach analysis results:*

Five approaches were examined, all with fundamental barriers when applied to built-in views:

| Approach | Overview | Barrier |
|----------|----------|---------|
| A: Macro code generation | Extend `#[kasane_component]` to auto-derive patch code from view function AST | proc_macro operates on single-item local AST transformation. Cannot expand external functions or statically resolve layout |
| B: Runtime tracking | Record cell provenance during paint, identify affected cells via dirty flags | Can identify affected cells but **cannot compute new values** — view → layout → paint still required |
| C: Incremental diffing (React-style) | Redraw only changed parts via Element tree diffing | Already covered by Salsa memoization + section splitting. Additional diff layer not worth the complexity |
| D: Patch templates | Define repaintable slots, partially re-execute view + paint | **Most realistic**. Sub-section granularity pipeline execution |
| E: Declarative DSL | Describe patches in a DSL, macro generates PaintPatch impl | Paint logic still hand-written. Gap between DSL expressiveness and Rust expressiveness is problematic |

Root cause: Rust view functions contain mixed algorithmic computation (word wrap, bin-packing, truncation, obstacle-avoidance positioning) that a compiler cannot statically analyze or transform.

Fundamental difference from Svelte:

```
[Svelte]
Template → Compiler → DOM API calls
                         ↓
              Browser's layout engine (implicit, automatic)
                         ↓
                    Screen pixels

[Kasane]
view() → Element tree → place() → LayoutResult → paint() → CellGrid → diff() → Terminal
           ↑               ↑                        ↑
        Self-built       Self-computed             Self-rendered
```

On the web, `element.textContent = "new"` causes the browser to automatically recalculate layout and repaint. The Svelte compiler relies on this **implicit layout engine** — the compiler only needs to specify "what to change," and the browser resolves "where to place it." Kasane has no equivalent mechanism; writing to CellGrid requires coordinates computed by the application itself.

Detailed barriers for Approach A (7 compilation passes):

1. **Element construction tracking**: Requires symbolic execution of `Vec::push()` sequences. Pattern space grows exponentially with conditional pushes
2. **External function expansion**: proc_macro can only operate on a single item and cannot reference other function bodies
3. **Static layout resolution**: `measure` is recursive and always computed at runtime. Unicode width of Text is statically undecidable
4. **Specialized paint code generation**: Mechanically possible if Element variants are statically known
5. **DirtyFlags conditional insertion**: A single view function uses fields depending on different DirtyFlags in a mixed fashion
6. **GPU path (DrawCommand) generation**: Must also generate DrawCommand sequences in addition to CellGrid, doubling code volume
7. **Correctness verification code generation**: Full pipeline comparison code for debugging

Difficult aspects of DSL (Approach E):

1. **Mixed algorithmic computation**: word wrap, bin-packing, truncation are inseparable from Element construction
2. **Content-dependent layout**: Info popup size depends on word wrap results (circular)
3. **Inter-component position dependency**: Info overlay position depends on Menu Rect + preceding overlay Rect
4. **Structural variations**: Menu 4-way branching, Info 3-way branching cause combinatorial explosion
5. **Layout result propagation to paint**: LayoutResult tree's recursive structure must be flattened to inline code
6. **DSL and Rust dual-world problem**: Helper functions need to be redefined as DSL primitives
7. **Stack + Overlay self-referential structure**: Non-monotonic draw order breaks the assumption that "each Element can be patched independently"

*Why plugins have an advantage:*

| Barrier | Built-in view | Plugin Slot function |
|---------|---------------|---------------------|
| Algorithmic computation | word_wrap, packing, truncate | **Almost none** — mainly raw data display |
| Content-dependent layout | measure → place circular | **Slot Rect is externally provided** — no self-positioning needed |
| Inter-component position dependency | Info avoids Menu | **Slot positions are fixed** — no inter-Slot interference |
| Structural variations | MenuStyle 4-way branching | **Usually 1 pattern** |
| Nesting depth | 5+ levels | **1-2 levels typical** |
| External function calls | Many internal helpers | **Self-contained** |
| Stack + Overlay | Info prompt self-referential structure | **No Overlay in Slots** (Overlay is a separate Slot) |

Fundamental reason: Plugin Slot contributions are a **constrained task** — "insert a small Element at a known position." Built-in views are an **unconstrained task** — "construct the structure of the entire screen." This difference determines the feasibility of DSL/compilation.

*5-stage roadmap (L0-L5):*

Recommended introduction order: L0 → L1 → L3 → L2 → L4 → L5 (maximum effect at minimum cost)

- **L0: Initial state (historical)** — Plugin contributions were rebuilt via the full pipeline
- **L1: Plugin state cache (implemented)** — `PluginSlotCache` in `PluginRuntime` caches `contribute_to()` results per slot, invalidating only when `state_hash()` changes
- **L3: Explicit DirtyFlags dependencies (removed)** — `contribute_deps()` / `transform_deps()` / `annotate_deps()` were removed; Salsa handles dependency tracking automatically
- **L2: Slot position cache (not implemented)** — Per-slot Rect cache for partial repaint when plugin state changes
- **L4: Automatic patch code generation (not implemented)** — Auto-generate `apply_grid()` equivalent for simple plugin views, falling back to L2 for unsupported patterns
- **L5: Decorator pattern recognition (not implemented)** — Recognize typical Decorator patterns and inject style overrides at the end of existing patches

## ADR-011: CLI Design — kak Drop-in Replacement

**Status:** Decided

**Context:**
kasane is a Kakoune UI frontend, not "a different editor." The goal is to minimize friction when kak users migrate to kasane, achieving a state where `alias kak=kasane` works completely.

**Decision:** Design kasane as a drop-in replacement for kak. Adopt the following 10 items.

### 11-1: Basic Policy — Drop-in Replacement

**Decision:** Guarantee that when kak is replaced with kasane via `alias kak=kasane` or PATH manipulation, all kak workflows work correctly.

**Rationale:**
- kasane is "a different UI" for Kakoune; users should perceive they are "using Kakoune"
- Same pattern as Neovide (GUI frontend for nvim): launched by frontend name, passing arguments to the backend
- When `$EDITOR=kasane` is set, kasane UI is used in git commit, ranger, and everything else

### 11-2: Non-UI Operation Delegation — exec

**Decision:** When non-UI operations (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`) are detected, replace the kasane process with kak via `exec`. `-ui json` is not appended.

**Rationale:**
- exec completely replaces the kasane process with kak, resulting in zero overhead
- The most Unix-correct approach (no unnecessary parent process left behind)
- Resolves the current inaccuracy of appending `-ui json` to non-UI operations

**Non-UI flag detection:** Hardcoded explicit list (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`). Manually added when kak adds new flags.

### 11-3: Flag System — Pre/Post `--` Separation

**Decision:** kasane-specific flags use GNU-convention `--long-option` format. kak flags are passed through as-is. `--` provides explicit separation.

**kasane-specific flags:**
- `--ui {tui|gui}` — Backend selection (one-shot override)
- `--version` — Display both kasane and kak versions
- `--help` — Display kasane help

**Parsing rules:**
1. Before `--`: Extract kasane-specific flags (`--ui`, `--version`, `--help`). Everything else is accumulated as kak arguments
2. After `--`: Everything is accumulated as kak arguments
3. Error rejection if kasane-specific flags and non-UI flags are mixed

**Rationale:**
- Clear separation: `--` (double dash) for kasane, `-` (single dash) for kak
- Avoids collision with kak's `-ui` (`kasane -ui gui` passes `-ui` and `gui` to kak)
- Safe for future flag additions (`--config`, `--log-level`, etc.)

### 11-4: Session Name Interception — Both `-c` and `-s`

**Decision:** Intercept both `-c` (session connect) and `-s` (session create) to have kasane retain the session name. Arguments are also passed through to kak.

**Rationale:**
- Display session name in GUI window title (`kasane — project`)
- Log with `[session=project]`
- Future session-specific config (`~/.config/kasane/sessions/project.toml`) extension
- Extremely small additional cost (a few lines of change)

### 11-5: Default UI Mode — Configurable via kasane.kdl

**Decision:** Make the default UI mode (TUI/GUI) configurable via `ui { backend }` in `kasane.kdl`. The `--ui` flag serves as a one-shot override.

**Rationale:**
- Users who want GUI as default no longer need to include `--ui gui` in their alias
- Practically eliminates the mixed kasane-specific/non-UI flag error
- Full migration possible with just `alias kak=kasane`

### 11-6: `--version` Output — Both kasane + kak

**Decision:** `kasane --version` displays both kasane and kak versions.

```
kasane 0.1.0 (kakoune vXXXX.XX.XX)
```

**Rationale:**
- Useful to know both versions when debugging
- `kasane -version` is exec-delegated to kak, displaying only kak's version (clear distinction)

### 11-7: Mixed Flag Behavior — Error Rejection

**Decision:** When kasane-specific flags (`--ui`, `--version`, `--help`) and non-UI flags (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`) are specified simultaneously, reject with an error.

```
kasane --ui gui -l
→ error: --ui cannot be combined with -l (non-UI operation)
```

**Rationale:**
- Backend selection is meaningless for non-UI operations; early detection of user mistakes
- Making the default UI configurable via `kasane.kdl` removes the motivation to include `--ui` in aliases, so this error practically never occurs
- Explicit errors over silent ignoring follows Rust ecosystem conventions

### 11-8: Native kak UI Fallback — Not Provided

**Decision:** No means is provided to fall back to the native kak terminal UI via kasane.

**Rationale:**
- Users who want the native UI can run kak directly
- kasane's raison d'être is "providing a different UI," and a fallback to the native UI would be contradictory

### Processing Flow

```
parse_cli_args(args)
├── 1. Extract kasane-specific flags (--ui, --version, --help)
├── 2. Extract interception targets (-c, -s → retain session name + also pass to kak)
├── 3. Detect non-UI flags (-l, -f, -p, -d, -clear, -version, -help)
├── 4. Mixed check (kasane-specific ∩ non-UI ≠ ∅ → error)
└── Result:
    ├── CliAction::KasaneVersion        ← --version
    ├── CliAction::KasaneHelp           ← --help
    ├── CliAction::DelegateToKak(args)  ← non-UI flag detected → exec kak
    └── CliAction::RunKasane { session, ui_mode, kak_args }  ← UI startup
```

### Examples

```bash
# Basic usage (drop-in)
kasane file.txt                    # → kak -ui json file.txt
kasane -c project                  # → kak -ui json -c project (session name retained)
kasane -s myses file.txt           # → kak -ui json -s myses file.txt (session name retained)
kasane -e "buffer-next"            # → kak -ui json -e "buffer-next"
kasane -n -ro file.txt             # → kak -ui json -n -ro file.txt

# kasane-specific flags
kasane --ui gui file.txt           # → Launch with GUI backend
kasane --version                   # → "kasane 0.1.0 (kakoune vXXXX.XX.XX)"
kasane --help                      # → Display kasane help

# Non-UI operations (delegated to kak via exec)
kasane -l                          # → exec kak -l
kasane -f "gg"                     # → exec kak -f "gg"
kasane -p session                  # → exec kak -p session
kasane -d -s daemon                # → exec kak -d -s daemon
kasane -version                    # → exec kak -version
kasane -help                       # → exec kak -help

# Error case
kasane --ui gui -l                 # → Error: --ui cannot be combined with -l

# Explicit separation with --
kasane --ui gui -- -e "echo hello" # → kak -ui json -e "echo hello" (GUI launch)
```

## ADR-012: Layer Responsibility Model

**Status:** Decided (revised from four layers to three)

**Context:**
During Phase 4a/4b item classification, a systematic criterion for determining which layer a feature belongs to was needed. The existing "resolution layer" was a classification of implementation mechanisms (HOW) and insufficient as a criterion for responsibility boundaries (WHERE).

Originally four layers (upstream / core / built-in plugin / external plugin), but since built-in plugins (`kasane-core/src/plugins/`) were migrated to WASM bundles and removed, the distinction between built-in and external became unnecessary. Revised to a three-layer model.

**Decision:** Adopt the three-layer responsibility model.

### 12-1: Three-Layer Definitions

| Layer | Definition | Criteria |
|-------|-----------|----------|
| Upstream (Kakoune) | Protocol-level concerns | Does it require protocol changes? |
| Core (kasane-core) | Faithful protocol rendering + frontend-native capabilities | Does a single correct implementation exist? |
| Plugin | Features where policy can diverge | Everything else |

The Plugin layer is subdivided by distribution form: bundled WASM (default UX) / FS-discovered WASM / native (`kasane::run()`).

### 12-2: Core Criteria — "A Single Correct Implementation"

Determined by "whether policy divergence exists."

- **Single policy:** Multi-cursor rendering (R-050) — there is only one way to parse faces → Core
- **Multiple policies:** Cursor line background color — color choice is user preference → Plugin
- **Frontend-native:** Focus detection (R-051), D&D (`P-023` proof-of-concept use case) — OS/window system specific → Core

### 12-3: API Parity

WASM plugins use a subset of the Plugin trait API via WIT interface. `contribute_to`, `transform`, `annotate_line_with_ctx`, `contribute_overlay_with_ctx`, `transform_menu_item`, and `render_ornaments` are available in WASM (WIT v0.4.0+). `Surface` and `Pane` APIs are available only in native plugins.

### 12-4: Upstream Criteria

Heuristic workarounds for information absent from the protocol are not constructed in principle.

**Exceptions:** Existing high-reliability heuristics are maintained:
- Cursor detection via FINAL_FG+REVERSE (R-064) — de facto standard behavior
- Estimation of auxiliary region contributions via face name pattern matching (`P-010` / `P-011` partial proof) — full version depends on upstream

**Rationale:**
- Heuristics risk breaking on upstream implementation changes
- Maintains motivation to encourage upstream toward formal solutions
- Features based on unreliable guesses degrade user experience

**Trade-offs:**
- Some features are unavailable while waiting for upstream changes
- Existing heuristics (FINAL_FG+REVERSE, etc.) are reliable and practical, so maintained as exceptions
- New heuristics are evaluated individually for reliability

### 12-5: Phase 4 Shared Plugin API Validation (Completed)

Proof artifacts for extension points reachable from WASM:

| Shared Extension Point | Proof Artifact | Status |
|------------------------|----------------|--------|
| `contribute_to(SlotId::BUFFER_LEFT)` | color_preview (gutter swatch) | Proven |
| `contribute_to(SlotId::STATUS_RIGHT)` | sel-badge (selection count badge) | Proven |
| `annotate_line_with_ctx()` | cursor_line (line background highlight), color_preview (gutter swatch) | Proven |
| `contribute_overlay_with_ctx()` | color_preview (color picker) | Proven |
| `handle_mouse()` | color_preview (color value editing) | Proven |
| `handle_key()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform_menu_item()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `contribute_to(SlotId::OVERLAY)` | Internal use (info/menu) | Implemented (external plugin proof pending) |
| `contribute_to(SlotId::BUFFER_RIGHT)` | — | Unproven (full version deferred due to upstream blocker) |
| `contribute_to(SlotId::ABOVE_BUFFER / BELOW_BUFFER)` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform(TransformTarget::BUFFER)` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform(TransformTarget::STATUS_BAR)` | prompt-highlight (status bar wrap in prompt mode) | Proven |
| `render_ornaments()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `contribute_to(SlotId::Named(...))` | `surface_probe` hosted surface E2E in `kasane-wasm/src/tests.rs` | Proven |
| `OverlayAnchor::Absolute` | `fuzzy_finder` overlay test in `kasane-wasm/src/tests.rs` | Proven |

## ADR-013: WASM Plugin Runtime — Component Model Adoption

**Status:** Decided

**Context:**
While evaluating runtime loading approaches for external plugins in Phase 5b, it was necessary to quantitatively assess the performance feasibility of WASM sandboxing. The current compile-time binding approach (`kasane::run()` + `#[kasane::plugin]`) is type-safe but requires rebuilding to add plugins. WASM would enable install-and-activate without rebuilds, expanding the plugin ecosystem.

**Benchmark environment:** `kasane-wasm-bench` crate (wasmtime 43, criterion)

**Evaluation method:** A 4-stage gate approach with pass criteria predefined for each gate, evaluated incrementally.

### 13-1: Benchmark Results

A 4-stage gate approach was used. All gates passed (Gate 3 conditionally — ratio criterion fails for lightweight functions, but absolute values fit within frame budget):

- **Gate 1 (Raw WASM overhead):** ~25 ns/call boundary crossing. Pass.
- **Gate 2 (Data crossing):** 59 ns–4.50 μs depending on payload. Pass.
- **Gate 3 (Component Model overhead):** ~500 ns fixed overhead from canonical ABI. 4.1x–23.7x ratio vs raw module, but absolute values < 7 μs. Conditional pass.
- **Gate 4 (Realistic simulation):** ~1.8 μs/plugin (linear), 5 plugins = 8.91 μs (18.2% of frame budget). Pass.

For detailed benchmark tables, see [performance.md — WASM Plugin Benchmarks](./performance.md#wasm-plugin-benchmarks).

### 13-2: Frame Budget Analysis

5 plugins consume 18.2% of the ~49 μs frame budget; 10 plugins consume 36.7%. L1 cache (DirtyFlags) completely skips WASM calls on frames with no state change (cache hit: 0.26 ns).

For detailed budget breakdown, see [performance.md — WASM Plugin Benchmarks](./performance.md#realistic-plugin-simulation).

### 13-3: Decision

**Adopt Component Model (wasmtime) as the foundation for the plugin runtime.**

**Rationale:**

1. **Sufficient absolute performance**: 18% of budget with 5 plugins, 37% even with 10 plugins. Ample headroom remains for the host-side pipeline.
2. **DX superiority**: Type-safe interface definitions via WIT, automatic serialization (canonical ABI), no manual memory management. Overwhelmingly superior compared to the maintenance cost of raw module's binary protocol.
3. **Language independence**: Plugins can be written in any language supporting the wasm32-wasip2 target, including Rust, C/C++, Go, JavaScript (wasm-bindgen), etc.
4. **Sandbox safety**: WASM's linear memory model prevents plugins from corrupting host memory.
5. **Acceptable startup cost**: Compilation 10 ms + 10 instances 280 μs ≈ 10 ms. Imperceptible to users.
6. **Synergy with caching**: The existing DirtyFlags + PluginSlotCache (L1/L3) mechanism completely avoids WASM calls on frames with no state change.

**Trade-offs:**

- Component Model adds 13-21x overhead for lightweight functions. However, the absolute value is ~550 ns, only 1.1% of the frame budget (~49 μs).
- Raw module approach has 10-20x lower overhead, but manual memory management, binary protocol, and lack of type safety significantly degrade DX.
- Native plugins (current approach) still offer the best performance, but the rebuild requirement limits ecosystem scalability.

**Future direction:**

- Phase W1: WIT interface design (define kasane's Plugin trait equivalent in WIT)
- Native plugins maintained as escape hatches for Decorator/Replacement and other WIT-unexposed APIs
- Host function pattern (guest→host calls for state retrieval) established as the primary data flow
- Leverage Component Model compile result caching (`Engine::precompile_component`) to speed up subsequent startups

## ADR-014: GUI Technology Stack — winit + wgpu + glyphon

**Status:** Decided

**Context:**
After adopting the TUI + GUI hybrid approach in ADR-001, the specific technology stack and event loop design for the GUI backend were evaluated.

### 14-1: Rendering Stack — winit + wgpu + glyphon

**Decision:** Adopt winit for window management, wgpu for GPU rendering, and glyphon for text rendering.

| Library | Role |
|---------|------|
| winit | Window management, input events, IME |
| wgpu | GPU rendering API (Vulkan/Metal/DX12/GL abstraction) |
| glyphon | Text rendering (cosmic-text + swash + etagere atlas) |

**Selection rationale:** cosmic-term (the official terminal of COSMIC Desktop) uses the same stack in production, with proven track record for monospace grid rendering. glyphon integrates cosmic-text's font shaping (rustybuzz) + swash rasterization + etagere atlas packing into the wgpu pipeline.

**Rejected alternatives:**

| Candidate | Reason for rejection |
|-----------|---------------------|
| OpenGL (glutin + glow) | macOS has deprecated OpenGL. wgpu internally has an OpenGL ES backend |
| Native API (Metal/Vulkan direct) | Requires a separate renderer per platform. Doubles maintenance cost |
| CPU only (softbuffer + tiny-skia) | Insufficient as the main path for 60fps smooth scrolling. Considered as fallback but not implemented |
| egui | Immediate mode conflicts with TEA retained mode. Not specialized for monospace grids |
| Vello (Linebender) | No glyph cache (vector path rendering every frame), unstable API (breaking changes every 3-5 months), requires compute shaders |

### 14-2: Event Loop — run_tui/run_gui Branching

**Decision:** Adopt the approach of switching the entire event loop via the `--ui gui` CLI argument (run_tui/run_gui branching).

**Rationale:**
- winit's `run_app()` completely occupies the main thread, so it cannot coexist with TUI's existing `recv_timeout` loop
- GUI side places the winit event loop (`ApplicationHandler`) on the main thread, Kakoune Reader on a separate thread, and merges them via `EventLoopProxy`

**Rejected:** `pump_events` approach — does not work on macOS (Cocoa/AppKit constraints. winit documentation explicitly states "not supported on iOS, macOS, Web").

---

## ADR-015: Rendering Pipeline Performance Improvements

**Decision:** Incrementally resolve 4 structural inefficiencies in the rendering pipeline.

### Background

The CPU pipeline was ~49 μs (80×24) within the frame budget, but the following inefficiencies were wasting scaling potential and resources:

1. **Per-frame allocation**: `grid.diff()` allocates a `Vec<CellDiff>` every frame (~196 KB on full redraw, 71% of per-frame heap allocation)
2. **Inefficient escape sequence generation**: `TuiBackend::draw()` emits `MoveTo` for every cell and resets+reapplies all SGR attributes on each Face change
3. **Narrow line_dirty optimization coverage**: Only exact match of `dirty == DirtyFlags::BUFFER`. Ineffective for `BUFFER|STATUS` (the most common batch)
4. **Container fill overhead**: `paint_container` executes per-cell `put_char(" ")` with wide character cleanup checks

### Implementation

All 4 stages implemented: (P4) container fill → `clear_region()`, (P1) zero-allocation diff via `diff_into()` / `iter_diffs()`, (P3) line_dirty coverage extension via `selective_clear()`, (P2) `draw_grid()` with cursor auto-advance + incremental SGR diff.

Key results: TUI backend 2.4–3x faster, diff allocation eliminated (196 KB → 0), common editing pattern 57% CPU reduction.

### Implementation Status

#### Stage P4: Container Fill Bulk Optimization — Implemented

Replaced per-cell `put_char(" ")` loop in `paint_container` with `clear_region()`. Eliminates per-cell bounds checking, wide-char cleanup branches, and CompactString construction. ~0.5–2 μs savings per container paint.

#### Stage P1: Zero-Allocation Diff Path — Implemented

| Method | Description | Allocation |
|---|---|---|
| `diff_into(&mut buf)` | Reuses caller-provided `Vec<CellDiff>` | 0 (warm buffer) |
| `iter_diffs()` | Zero-copy iterator yielding `(u16, u16, &Cell)` | 0 |
| `is_first_frame()` | Returns `self.previous.is_empty()` | N/A |

#### Stage P3: Line-Dirty Coverage Expansion — Implemented

Extended line-dirty optimization from `dirty == DirtyFlags::BUFFER` (exact match) to `dirty.contains(DirtyFlags::BUFFER)`. The common case of `BUFFER|STATUS` (Draw + DrawStatus in same batch) now benefits from per-line dirty tracking via `selective_clear()`.

| Scenario | Before | After | Savings |
|---|---|---|---|
| BUFFER\|STATUS, 1 line changed | ~57 μs (full pipeline) | ~17 μs | −70% |

#### Stage P2: Direct-Grid Backend Draw + Incremental SGR — Implemented

`draw_grid()` on `RenderBackend` trait iterates `grid.iter_diffs()` directly, with two optimizations:
1. **Cursor auto-advance**: Skip `MoveTo` for consecutive cells on the same row (terminal auto-advances after Print)
2. **Incremental SGR**: `emit_sgr_diff()` compares faces field-by-field, emitting only changed attributes/colors

| Benchmark | Legacy `draw()` | Optimized `draw_grid()` | Speedup |
|---|---|---|---|
| Full redraw 80×24 | 138 μs | 44.5 μs | 3.1x |
| Full redraw 200×60 | 782 μs | 228 μs | 3.4x |

#### Overall ADR-015 Impact

- **TUI backend I/O**: 3.1–3.4x faster escape sequence generation
- **Per-frame allocation**: 196 KB → 0 (diff phase)
- **Common editing pattern** (BUFFER|STATUS, 1 line): ~70% CPU pipeline reduction
- **Container paint**: ~0.5–2 μs savings per container

### Resolved Bottlenecks

#### Buffer Line Cloning — Resolved

Element tree uses owned types, which would require cloning all buffer lines every frame.

**Resolution: BufferRef pattern (implemented)**. `Element::BufferRef { line_range }` eliminates clone cost.

#### Container Fill Loop — Resolved

`paint.rs` previously performed O(w*h) `put_char(" ")` calls for container background fill.

**Resolution (P4):** Replaced with `clear_region()` bulk operation, eliminating per-cell overhead.

#### diff() Allocation Dominance — Resolved

diff() previously allocated 196 KB per frame (71% of total) due to CellDiff owning cloned Cell data.

**Resolution (P1+P2):** `diff_into()` reuses a caller-provided buffer. `iter_diffs()` provides zero-copy iteration. The TUI event loop now uses `draw_grid()` directly, eliminating all CellDiff allocation.

#### grid.diff() Exceeds Target — Accepted

diff() at 12.2 us (incremental) exceeds the original 10 us target. Cell comparison involves CompactString (24B) + Face (16B) + u8 per cell. The `dirty_rows` optimization helps but the per-cell comparison cost is inherently higher than estimated.

### 240Hz Analysis

The CPU pipeline uses <2% of the 4.17 ms budget at 80×24, making 240fps achievable for the GPU backend with animation path separation:

```
Content frame:    parse → apply → view → place → paint → diff → draw  (~57 μs + I/O)
Animation frame:  ──────────── skip ────────────── → scroll offset → GPU draw
```

TUI is not meaningful at 240fps (terminal emulators refresh at 60-120Hz). GUI (wgpu) has 4-8x CPU headroom. Per-frame diff allocation has been eliminated by `iter_diffs()` zero-copy path. Salsa integration further improves large-screen headroom by 30-36%.

For current benchmark data, see [performance.md](./performance.md).

## ADR-016: Pipeline Equivalence Testing — Trace-Equivalence Axiom

**Status:** Decided

### Background

Kasane's rendering pipeline has multiple optimization variants:

1. `render_pipeline()` — full pipeline (reference implementation)
2. `render_pipeline_cached()` — Salsa-backed memoization (previously `render_pipeline_direct()` with ViewCache)
3. ~~`render_pipeline_sectioned()`~~ — removed (Salsa handles section-level memoization)
4. ~~`render_pipeline_patched()`~~ — removed (PaintPatch superseded by Salsa)
5. `scene_render_pipeline_cached()` — GPU path with SceneCache

Currently, inter-variant equivalence is verified through `debug_assert` (debug builds only) and manual tests (`cache_soundness.rs`), with the following issues:

- `cache_soundness.rs` tests only one fixed state (`rich_state()`)
- `debug_assert` is disabled in release builds
- The combination space of DirtyFlags and state mutations is wide, risking missed edge cases

### Decision

Define as a formal invariant that all pipeline variants are **observationally equivalent** for any valid `AppState` and `DirtyFlags` combination, verified through property-based testing with proptest.

**Equivalence axiom:**
```
∀ S ∈ ValidAppState, ∀ D ∈ DirtyFlags:
  render_pipeline(S) ≡ render_pipeline_direct(S, D, warm_cache(S))
                     ≡ render_pipeline_sectioned(S, D, warm_cache(S))
                     ≡ render_pipeline_patched(S, D, warm_cache(S))
```

Here `warm_cache(S)` is the cache after a full render with ALL flags on state S.

### Testing Strategy

1. **Mutation-based fuzzing**: Apply random state mutations (cursor movement, line changes, menu toggle, etc.) to `rich_state()` as a base
2. **Random DirtyFlags**: Current harness randomly generates combinations of 6 coarse categories (`BUFFER`, `STATUS`, `MENU_STRUCTURE`, `MENU_SELECTION`, `INFO`, `OPTIONS`)
3. **Warm → Mutate → Render**: Warm the cache, then apply mutations and compare rendering results with partial flags against a full render

Full Arbitrary implementation is unnecessary — the mutation-based strategy efficiently covers the combination space.

### Implementation Notes (2026-03)

- `DirtyFlags` currently has `BUFFER_CONTENT`, `BUFFER_CURSOR`, `STATUS`, `MENU_STRUCTURE`, `MENU_SELECTION`, `INFO`, `OPTIONS`
- The current `trace_equivalence.rs` strategy does not generate `BUFFER_CURSOR` independently, folding it into the coarse-grained `BUFFER` category
- Therefore, this ADR's axiom is a requirement on current semantics, and the verification harness granularity has not yet reached complete enumeration

### Preservation Mechanism

```
DirtyFlags → Salsa input sync (PartialEq early-cutoff) → per-section rebuild decision
          → SceneCache invalidation → DrawCommand regeneration decision (GPU only)
```

> **Note:** The original diagram referenced ViewCache/LayoutCache, which have been superseded by Salsa (ADR-020).

If each cache's invalidation is correct, all variants are equivalent to the reference implementation.

## ADR-017: SurfaceId-Based Invalidation (Design)

**Status:** Proposed (implementation to be evaluated when Phase 5 begins)

### Background

The current `DirtyFlags` are global: Draw messages from Kakoune invalidate all Salsa inputs and SceneCache. In Phase 5 (multi-pane), pane A's Draw would unnecessarily invalidate pane B's cache.

### Proposed Design

1. **`SurfaceDirtyMap`**: Replace global `DirtyFlags` with `HashMap<SurfaceId, DirtyFlags>`
2. **Per-surface Salsa inputs**: Per-surface input structs for per-surface memoization
3. **`apply()` return type change**: `DirtyFlags` → `Vec<(SurfaceId, DirtyFlags)>`
4. **Global events**: Refresh, SetUiOptions broadcast `ALL` to all surfaces
5. **BUFFER_CURSOR split integration**: Per-surface `BUFFER_CONTENT` for inter-pane isolation

### Surface ↔ DirtyFlags Mapping

| Surface | Primary DirtyFlags |
|---------|-------------------|
| `SurfaceId::BUFFER` (per-pane) | `BUFFER_CONTENT`, `BUFFER_CURSOR` |
| `SurfaceId::STATUS` | `STATUS` |
| `SurfaceId::MENU` | `MENU_STRUCTURE`, `MENU_SELECTION` |
| `SurfaceId(INFO_BASE + i)` | `INFO` |
| Plugin surfaces | `OPTIONS` (config change) + custom |

### Compatibility with Existing Mechanisms

- RenderOrnaments surface anchor — per-surface ornament targeting. Consistent with the design
- `EffectiveSectionDeps` — extendable to per-surface deps
- `PluginSlotCache` — independent cache entries per surface

### Migration Path

1. Introduce `SurfaceDirtyMap` internally while maintaining global `DirtyFlags` as a fallback
2. In `apply()`, set flags only for the target surface for Draw; broadcast to all surfaces for others
3. Gradually migrate Salsa inputs to per-surface
4. Testing: existing `cache_soundness.rs` + `trace_equivalence.rs` guarantee single-surface equivalence

### Risks

- Plugin API compatibility: `on_state_changed(dirty: DirtyFlags)` is safest to keep as global (OR aggregation)
- Increased complexity: premature before multi-pane is implemented. Re-evaluate when Phase 5 begins

## ADR-018: Display Policy Layer and Display Transformation / Display Unit Model

**Status:** Decided

### Background

While organizing Kasane's requirements framework, it became necessary to make the following distinctions explicit:

- Core features that Kasane itself directly guarantees
- Capabilities that Kasane provides as an extension infrastructure
- Proof-of-concept use cases realized on top of that infrastructure

In particular, to handle overlay, folding, auxiliary region UI, display-line navigation, workspace UI, etc. consistently, it became clear that simply "drawing the Observed State as-is" is insufficient, and a display policy layer is needed on the frontend side.

Previously, `Overlay`, `Decorator`, `Replacement`, `Transform`, and `Surface` existed individually, but it was unclear what theory they were part of. As a result, issue-driven requirements tended to flow into enumeration of individual features, and "what Kasane directly implements" vs. "what Kasane enables" became conflated.

### Decision

Kasane adopts the `Display Policy Layer` as a first-class design concept.

This layer determines "what display structure to project into" before passing Observed State to rendering, and includes at least the following:

- overlay composition
- contributions to auxiliary regions
- display transformation
- surrogate display
- display unit grouping
- interaction policy

### 18-1: Permit Display Transformation

Kasane permits plugins and future standard UI to restructure Observed State using `Display Transformation`.

Display Transformation may include:

- elision
- surrogate display
- additional display
- restructuring

However, this is **display policy**, not falsification of protocol truth.

### 18-2: Permit Observed-Eliding Transformation

Kasane permits not only `Observed-preserving transformation` but also `Observed-eliding transformation`.

Examples:

- Summary display of multiple lines via fold summary
- Restructuring into a different structure via outline view
- Relocation of content to auxiliary UI

However, elided Observed State must not be treated as "a fact sent by upstream as such." Elision is a display-level omission, not deletion of truth.

### 18-3: Introduce Display Unit Model

Kasane introduces `Display Unit` as the smallest operable unit of the restructured UI.

A Display Unit is not merely a layout box; it may have at least the following:

- geometry
- semantic role
- source mapping
- interaction policy
- navigation relationships with other units

This enables meaningful hit test, focus, navigation, and selection even for UI that has undergone display restructuring.

### 18-4: Handling When Source Mapping Is Weak

When a Display Unit does not have a complete inverse mapping to its source, Kasane may treat that unit as read-only or with restricted interaction.

The important thing is not to leave undefined operations implicit. Kasane should be able to explicitly represent units where interaction is impossible or restricted.

### 18-5: Core and Plugin Responsibility Allocation

What plugins are responsible for:

- Defining transformation policy
- Introducing display units
- Interaction policy for plugin-specific UI

What core is responsible for:

- Separation of protocol truth and display policy
- Placing plugin-defined UI under the same composition rules as standard UI
- Infrastructure for representing display units as targets for hit test, focus, and navigation
- Semantics for degraded mode when source mapping is weak

### 18-6: Relationship with Existing APIs

In the current API, dedicated abstractions for `Display Transformation` and `Display Unit` are incomplete.

Current proof-of-concept means:

- `Overlay`
- `Decorator`
- `Replacement`
- `Transform`
- `LineDecoration`
- `Surface`

These are fragmentary representations of the future Display Policy Layer, not complete equivalents. In particular, source mapping and display-oriented navigation are subjects for future infrastructure development.

### 18-7: Non-goals

This ADR does not mean immediately becoming a general-purpose UI framework.

Kasane continues to be a Kakoune-specific frontend runtime, and the Display Policy Layer is also designed with the assumption of Observed State received from Kakoune's JSON UI.

### 18-8: Consequences

With this decision, the requirements documents are organized as follows:

- Core requirements
- Extension infrastructure requirements
- Proof-of-concept targets and representative use cases
- Upstream dependencies and degraded behavior

Additionally, the semantics document treats `Display Policy State`, `Display Transformation`, and `Display Unit` as first-class concepts.

The next implementation steps are to incrementally introduce the following in Phase 5:

1. display transformation hook
2. display unit model
3. display-oriented hit test / navigation
4. source mapping and interaction policy development

## ADR-019: Plugin I/O Infrastructure — Hybrid Model

**Status:** Decided

**Context:**

During Phase P, the design for granting I/O capabilities to plugins was evaluated. Currently, WASM plugins are given no capabilities with `WasiCtxBuilder::new().build()`, and cannot use filesystem, process execution, or network communication. This prevents building plugins such as fuzzy finders, file browsers, and linter integrations.

wasmtime is linked in synchronous mode (`add_to_linker_sync`), and all Plugin trait methods, all WASM calls in adapter.rs, and both event loops (TUI/GUI) operate synchronously.

### 19-1: I/O Architecture Model — Hybrid Model

**Options evaluated:**

| Option | Overview |
|--------|----------|
| A: Host-mediated only | All I/O proxied through `Command` + `update()` |
| B: WASI direct only | Grant all capabilities via `WasiCtxBuilder`, convert wasmtime to async |
| C: Hybrid | Synchronous I/O via WASI direct, asynchronous I/O via host-mediated |

**Decision:** Adopt the hybrid model (C).

**Separation criterion:** "Can it block indefinitely?"

| I/O Operation | Blocking Characteristics | Model |
|---------------|------------------------|-------|
| Filesystem read/write | Typically μs–ms | WASI direct (`preopened_dir`) |
| Environment variable retrieval | ns | WASI direct (`env`) |
| Monotonic clock / random | ns | WASI direct (`inherit_monotonic_clock`) |
| External process execution | Indefinite | Host-mediated (`Command::SpawnProcess`) |
| Network communication | Indefinite | Host-mediated (future) |

**Rationale:**

1. **Avoiding wasmtime async conversion:** Option B (WASI direct only) would require changing from `add_to_linker_sync` → `add_to_linker_async`. This is a large-scale refactoring affecting all 19 methods in adapter.rs, all Plugin trait method signatures, registry.rs, both event loops, and the rendering pipeline — disproportionate in effort and design impact. The hybrid model maintains `add_to_linker_sync` while enabling only the synchronous subset of WASI (`wasi:filesystem`, `wasi:clocks`, `wasi:random`).

2. **WASI specification constraints:** `wasi:cli/command` is a specification for "executing a WASM component as a command," not for "launching arbitrary programs on the host from a guest." Even with option B, process spawning would require a custom host-side WIT interface, arriving at the same structure as host-mediated.

3. **Hot path protection:** With option B, plugins could call `std::process::Command` within `contribute_to()`, causing the rendering thread to block indefinitely. The hybrid model structurally excludes process execution and network communication from the hot path.

4. **Streaming and backpressure:** With host-mediated process execution, the host delivers stdout in 16ms batches and manages buffer size limits and cancellation. These controls are difficult with synchronous pipe processing inside WASM.

5. **Incremental migration path:** The hybrid model is on the incremental migration path toward B. If wasmtime async conversion becomes necessary in the future, the existing `Command::SpawnProcess` + `IoEvent` patterns can be maintained as-is, with additional enabling of `wasi:sockets`, etc.

**Security model:**

| Layer | Mechanism | Control |
|-------|-----------|---------|
| WASI layer (synchronous I/O) | `preopened_dir` | Plugin-dedicated directory only. Manifest declaration + user approval |
| Host layer (asynchronous I/O) | Host-side validation of `Command::SpawnProcess` | Program allow list, argument validation |

**Trade-offs:**

- Plugin authors need to use 2 I/O patterns (files via `std::fs`, processes via `Command`)
- File I/O can still be called within hot paths, requiring documentation warnings and runtime measurement
- NFS / FUSE mounts and other exceptionally slow filesystems risk blocking with synchronous I/O

### 19-2: I/O Event Delivery Method — Unified IoEvent Type

**Options evaluated:**

| Option | Overview |
|--------|----------|
| A: Reuse existing `update(Box<dyn Any>)` | Wrap ProcessEvent in `Box<dyn Any>` and deliver via `deliver_message()` |
| B: Dedicated method `on_process_event()` | Add a ProcessEvent-specific method to Plugin trait |
| C: Unified type `on_io_event(IoEvent)` | Add 1 method to Plugin trait that receives an IoEvent enum |

**Decision:** Adopt the unified IoEvent type (C).

```rust
enum IoEvent {
    Process(ProcessEvent),
    // Future: Http(HttpResponse), FileWatch(FileWatchEvent), ...
}

enum ProcessEvent {
    Stdout { job_id: u64, data: Vec<u8> },
    Stderr { job_id: u64, data: Vec<u8> },
    Exited { job_id: u64, exit_code: i32 },
}

// 1 method added to Plugin trait
fn on_io_event(&mut self, _event: IoEvent, _state: &AppState) -> Vec<Command> {
    vec![]
}
```

**WIT:**

```wit
variant io-event {
    process(process-event),
}

record process-event {
    job-id: u64,
    kind: process-event-kind,
}

variant process-event-kind {
    stdout(list<u8>),
    stderr(list<u8>),
    exited(s32),
}

on-io-event: func(event: io-event) -> list<command>;
```

**Rationale:**

1. **Type safety:** Option A's `Box<dyn Any>` + downcast risks silent ignoring. Option C's structured type enables IDE completion and compile-time verification.

2. **Scalability:** Option B (dedicated methods) would add methods to Plugin trait for each future I/O type (`on_http_response()`, `on_file_changed()`, ...). Option C only adds `IoEvent` variants, leaving Plugin trait unchanged.

3. **Role clarity:** `update()` is dedicated to inter-plugin messages, `on_io_event()` to host I/O completion notifications, `on_state_changed()` to Kakoune protocol state change notifications. Three asynchronous input paths are clearly separated.

4. **WASM compatibility:** Defining `io-event` as a variant type in WIT allows the WASM guest side to receive structured data without tag bytes or serialization conventions.

### 19-3: Sub-phase Structure

Reflecting the decisions of 19-1 and 19-2, the Phase P sub-phases are restructured.

**Problems with the old structure (P-a / P-b / P-c):**

- P-a (async task infrastructure) and P-b (SpawnProcess) have inseparable delivery destination (`IoEvent` type) and delivery source (`ProcessManager`), making separate implementation impossible
- P-c (WASI capabilities) becomes independent of process execution under the hybrid model, enabling earlier implementation

**New structure:**

| Sub-phase | Content | Dependencies |
|-----------|---------|-------------|
| P-1 | WASI capability infrastructure: capability declarations in manifest, per-plugin `WasiCtxBuilder` configuration injection (`preopened_dir`, `env`, `inherit_monotonic_clock`) | None |
| P-2 | Process execution infrastructure: `IoEvent` / `ProcessEvent` type definitions, `Plugin::on_io_event()` + WIT addition, `Command::SpawnProcess` + `ProcessManager`, event loop integration, 16ms batch delivery, job ID / cancel | P-1 (program allow list in manifest) |
| P-3 | Proof-of-concept and stabilization: fuzzy finder reference implementation (WASM guest), runtime frame time measurement, backpressure tuning | P-2 |

**Key changes:**

- Merged P-a/P-b into P-2 (they were inseparable)
- Moved P-c earlier as P-1 (can be implemented independently of process execution)
- Added P-3 (proof-of-concept phase)

**Implementation status:** All sub-phases (P-1, P-2, P-3) are done. For implementation status, see [roadmap.md](./roadmap.md).

## ADR-020: Salsa Incremental Computation — Stage 1/2 Split

**Status:** Decided

### Background

Kasane's rendering pipeline previously used a multi-layer caching system (ViewCache, LayoutCache, SceneCache, PaintPatch) driven by manual `DirtyFlags` bitmask tracking. While effective — achieving ~49μs CPU per frame at 80×24 — the system had accumulated complexity:

1. **Manual invalidation bookkeeping**: Each view function had to declare its `DirtyFlags` dependencies (BUILD_BASE_DEPS, BUILD_MENU_SECTION_DEPS, etc.), verified at compile time by the `#[kasane::component(deps(...))]` macro. Adding new state fields required updating both `DirtyFlags` and all dependency declarations.

2. **Cache coherence by convention**: `ViewCache`, `SceneCache`, and `LayoutCache` each duplicated the invalidation logic (which flags invalidate which cache section), with correctness relying on manual alignment rather than structural guarantees.

3. **Plugin interaction complexity**: `PluginSlotCache` used its own two-level cache (L1: state_hash, L3: slot_deps) independent of the view caching system, requiring separate `prepare_plugin_cache()` calls before rendering.

The Salsa incremental computation framework (v0.26.0) offers automatic dependency tracking and memoization, potentially replacing the manual invalidation bookkeeping while preserving the pipeline's performance characteristics.

### Decision

Adopt a **Stage 1 / Stage 2 split** architecture where:

- **Stage 1 (Salsa tracked)**: Pure Element generation from protocol state. Salsa automatically tracks dependencies and memoizes results. No plugin interaction.
- **Stage 2 (hybrid)**: Plugin contributions, transforms, and annotations collected imperatively from `PluginRuntime` (which uses `RefCell` interior mutability), then stored as Salsa inputs. Pure transform patches (`ElementPatch`) and per-plugin contribution results are set as Salsa inputs with `PartialEq` early-cutoff, enabling downstream memoization when plugin outputs are stable across frames. Impure patches (Custom, ModifyAnchor) fall back to imperative application.

Salsa is a mandatory dependency. The legacy Surface-based pipeline (`pipeline_surface.rs`, `SurfaceViewSource`) has been removed; all rendering uses the Salsa path exclusively.

### Architecture

Stage 1 uses 7 Salsa input structs: 6 grouped by protocol message boundary (Buffer, Cursor, Status, Menu, Info, Config) plus `TransformPatchesInput` for pre-collected pure transform patches. Four tracked view functions produce Element trees from these inputs. Stage 2 collects plugin outputs imperatively and writes them into additional Salsa inputs (`SlotContributionsInput`, `AnnotationResultInput`, `PluginOverlaysInput`, `DisplayDirectivesInput`, `TransformPatchesInput`) via `sync_plugin_contributions()`, `sync_display_directives()`, and `sync_transform_patches()`. Each input uses `PartialEq` early-cutoff for fine-grained memoization. The `ContributionCache` (per-plugin per-slot caching) is owned by `SalsaInputHandles`, consolidating all sync-phase state. The legacy manual caching infrastructure (ViewCache, LayoutCache, PaintPatch) has been removed; `SalsaViewSource` is the sole implementation. `SceneCache` remains as a GPU-path auxiliary cache.

For implementation details (input structs, tracked functions, pipeline variants, file mapping), see the source code in `kasane-core/src/salsa_sync.rs`, `kasane-core/src/salsa_inputs.rs`, and `kasane-core/src/render/pipeline_salsa.rs`.

### Trade-offs

1. **Now fully replacive**: The Salsa layer adds ~11-13μs of cache-hit overhead (5-6 tracked functions × ~2.2μs each), which is negligible relative to the 4167μs frame budget at 240fps. The legacy caching infrastructure (`ViewCache`, `LayoutCache`, `PaintPatch`) has been fully removed. Only `SceneCache` remains as a GPU-path auxiliary cache for per-section `DrawCommand` reuse.

2. **Plugin boundary is hybrid**: Plugins with `RefCell` interior mutability cannot participate directly in Salsa's dependency graph. The epoch-based bridge detects when plugin outputs *might* have changed, and the sync phase re-collects outputs into Salsa inputs. However, pure transform patches (`ElementPatch` with `PartialEq`) and contribution results benefit from Salsa's early-cutoff: when a plugin's output is unchanged across frames, downstream revalidation is skipped.

3. **Legacy pipeline removed**: The `salsa_pipeline_comparison.rs` test suite verifies correctness of the Salsa path against reference outputs.

4. **`no_eq` on all view functions**: Although `Element` implements `PartialEq`, the tracked view functions use `no_eq` because no downstream tracked functions depend on their outputs. Output-level equality checks would add comparison cost without benefit. This means a cache miss on any input *will* propagate to all callers, even if the output happens to be identical. This is acceptable because the tracked functions are leaf-level (no further tracked functions depend on their Element output).

### Testing

`kasane-core/tests/salsa_pipeline_comparison.rs` — 15 tests verifying cell-by-cell grid equivalence between legacy and Salsa pipelines across scenarios including:

- Base states (empty, buffer content, status bar, menu variants, info popups)
- Plugin contributions (slot, transform, annotation, gutter)
- Combined plugin scenarios

### Future Considerations

- If the pipeline is deepened (e.g., layout or composition as tracked functions), remove `no_eq` annotations to enable output-level early-cutoff (`Element` already implements `PartialEq`)
- When Phase 5 (multi-pane) introduces `SurfaceDirtyMap`, the Salsa input sync can be extended to per-surface granularity
- Plugin purity contracts (future): plugins that opt into pure `fn(&AppState) -> Element` could become tracked functions, eliminating the epoch bridge for those plugins

## ADR-021: PurePlugin State Externalization

**Status:** Decided — **Note:** The traits introduced here have been renamed in [ADR-022](#adr-022-plugin-trait-rename--pureplugin--plugin-plugin--pluginbackend): `PurePlugin` → `Plugin`, `Plugin` → `PluginBackend`, `PurePluginBridge` → `PluginBridge`, `IsPurePlugin` → `IsBridgedPlugin`. The body below preserves the original names at the time of decision.

### Background

Kasane's rendering pipeline uses a Stage 1/2 split (ADR-020): Stage 1 is Salsa-tracked pure functions, Stage 2 is imperative plugin application. The split exists because plugins hold mutable internal state (`&mut self` methods), making them incompatible with Salsa's pure function model.

The `Plugin` trait has 15+ `&mut self` methods for state transitions and 11+ `&self` methods for view generation. Plugin state caching relies on manual `state_hash() -> u64` (L1) combined with `DirtyFlags`-based slot dependency tracking (L3). This has two weaknesses:

1. Hash collisions can cause stale cache hits (hash-based, not structural equality)
2. Plugin state changes are opaque to the framework (no `PartialEq`, no direct state access)

### Decision

Introduce `PurePlugin` as an alternative to `Plugin` where the framework owns the state:

- **State externalization**: `PurePlugin::State` is a framework-owned `Clone + PartialEq + Debug + Default` type
- **Pure functions**: All methods are `(&self, &State, ...) → (State, effects)` — no `&mut self`
- **Automatic change detection**: State changes detected via `PartialEq` comparison, eliminating manual `state_hash()`
- **Adapter pattern**: `PurePluginBridge` wraps `PurePlugin` into `Plugin`, allowing coexistence

### Trade-offs

| For | Against |
|-----|---------|
| Automatic, collision-free state change detection | State clone cost on every transition (negligible for small states) |
| Pure functions enable future Salsa memoization of Stage 2 | `Plugin` cannot use `Surface` or workspace observation |
| Framework-owned state enables snapshotting and diffing | Blanket `PluginState` impl causes method resolution ambiguity with `Box<dyn PluginState>` (mitigated by using `&mut dyn PluginState` in erased interface) |
| Zero boilerplate for state types (blanket impl) | WASM plugins cannot externalize state to host without serialization overhead |
| Opt-in migration — existing plugins unchanged | Two plugin models to maintain during transition |

### Implementation

- `PluginState` trait with blanket impl for `T: Clone + PartialEq + Debug + Send + 'static`
- `PurePlugin` trait with explicit `State` associated type
- `ErasedPurePlugin` (object-safe, `pub(crate)`) erases the `State` type parameter
- `PurePluginBridge` adapts erased pure plugin to `Plugin` trait with generation-counter `state_hash()`
- `DirtyFlags::PLUGIN_STATE` (bit 7) added for explicit plugin state change signaling
- `IsPurePlugin` marker trait for runtime detection of pure-plugin-backed `dyn Plugin` objects

## ADR-022: Plugin Trait Rename — PurePlugin → Plugin, Plugin → PluginBackend

**Status:** Accepted

### Background

Since ADR-021, Kasane has had two native plugin models: `Plugin` (mutable, `&mut self`) and `PurePlugin` (state-externalized, pure functions). In practice, `PurePlugin` became the recommended model for the vast majority of plugins — it provides automatic cache invalidation, a path to Salsa memoization, and a simpler mental model.

However, the naming was a source of confusion:

- New plugin authors encountered `Plugin` first (the natural name) but it was the lower-level, internal-facing trait
- `PurePlugin` was the recommended API but its name suggested it was a specialized alternative
- The "Pure" prefix implied a secondary, academic variant rather than the primary API
- Documentation repeatedly had to explain that `PurePlugin` was preferred despite `Plugin` being the more obvious name

### Decision

Rename the traits to reflect their actual roles:

| Before | After | Role |
|--------|-------|------|
| `PurePlugin` | `Plugin` | Primary user-facing plugin trait (state-externalized) |
| `Plugin` | `PluginBackend` | Internal framework trait (mutable, full access) |
| `PurePluginBridge` | `PluginBridge` | Adapter: `Plugin` → `PluginBackend` |
| `IsPurePlugin` | `IsBridgedPlugin` | Marker trait for runtime detection |
| `register_pure()` | `register()` | Registration method for `Plugin` |
| `register()` (old, took `Box<dyn Plugin>`) | `register_backend()` | Registration method for `PluginBackend` |

### Rationale

- The primary API should have the simplest, most discoverable name
- `PluginBackend` clearly communicates that it is an internal/framework-level trait, not the first thing plugin authors should reach for
- `PluginBridge` and `IsBridgedPlugin` are more descriptive of what they actually do (bridging between models)
- `register()` for the common case, `register_backend()` for the advanced case follows the principle of progressive disclosure

### Trade-offs

| For | Against |
|-----|---------|
| Primary API has the natural name | Breaking change for existing native plugin code |
| Reduces confusion in documentation and onboarding | ADR-021 historical references now use old names |
| `PluginBackend` signals "internal, not your first choice" | Two renames in the plugin system's lifetime |

### Migration

- All `impl PurePlugin` → `impl Plugin`
- All `impl Plugin` (old mutable) → `impl PluginBackend`
- `registry.register_pure(x)` → `registry.register(x)`
- `registry.register(Box::new(x))` → `registry.register_backend(Box::new(x))`
- Historical ADR text (ADR-021) preserved with original names; current documentation updated

## ADR-023: Session Management Boundaries — Mechanism / Policy Split

**Status:** Current

### Context

Kasane's `SessionManager` manages multiple Kakoune processes, with `SessionStateStore` preserving `AppState` snapshots for inactive sessions. Prior to this decision, session information was invisible to plugins: there was no query API, no lifecycle event notification, and no command for plugins to switch sessions.

The roadmap identifies two active workstreams: Session/Surface parity (automatic surface generation per session) and Multi-session UI parity (session switcher/list). The question is which parts of these belong to core and which to plugins.

### Decision

Apply the principle of "mechanism, not policy" to session management:

- **Core owns mechanism**: process lifecycle, state snapshots, session-bound surface generation, switching mechanics
- **Plugins own policy**: session UI presentation, switching keybindings, status indicators, list decoration

Core additionally provides **infrastructure for plugin observability**:

1. Session descriptors exposed in observable state (session list, active session ID)
2. Session lifecycle dirty flag (`DirtyFlags::SESSION`) for cache invalidation
3. Session switch command exposed to plugins (including WIT)

### Rationale

The decision criterion is "Does a single correct implementation exist?":

- Process management, snapshot atomicity, and surface binding have single correct implementations → Core
- Session UI presentation varies by user preference → Plugin
- Observation and command infrastructure is owned by core (source of truth) but exists to enable plugins

This separation means the default session UI can ship as a bundled WASM plugin, replaceable by users. Core remains minimal and policy-free.

### Alternatives Considered

| Alternative | Rejected because |
|---|---|
| All-core (session UI in core) | Session UI is display policy; hardcoding it prevents customization and contradicts the layer model |
| All-plugin (session lifecycle in plugins) | Process management requires backend-specific wiring (reader/writer streams) that cannot be safely exposed to plugins |

### Implementation Order

1. ~~Core infrastructure: session descriptors in observable state, `DirtyFlags::SESSION`, `SessionCommand::Switch`~~ — Done
2. Session/Surface parity: automatic surface generation and deterministic switching
3. Session UI plugin: bundled WASM providing default session switcher

## ADR-024: Perception-Oriented Performance Policy

**Status:** Current

### Context

- vision.md declares "the most perceptive user on the best hardware should be unable to perceive any difference from native Kakoune"
- performance.md operationalizes performance as SLOs and benchmarks, but the values lack perceptual derivation
- Without a stopping condition, optimization becomes self-justifying
- Principle 3 (jitter) was T3 despite being the most perceptually salient artifact
- The document doesn't position Kasane within the full input-to-photon chain

### Decision

Adopt a three-layer performance policy:

**Layer 1 — Perceptual Compass** (strategic direction):
- Goal: Kasane's overhead vs native Kakoune imperceptible to most perceptive user on best current hardware (240 Hz, experienced typist)
- Order-of-magnitude guide, not precise threshold (perception is probabilistic and context-dependent)
- Imperceptibility = stopping condition for optimization

**Layer 2 — Engineering Guardrails** (tactical defense):
- Quantitative SLOs prevent sub-threshold regression accumulation (ratchets, not perceptual thresholds)
- Plugin budgets (< 3 μs) ensure ecosystem scalability (separate from perception)
- CI 115% alert threshold operationalizes the ratchet

**Layer 3 — Optimization Accountability** (justification requirement):
- Below-threshold optimization must state justification:
  (a) Headroom for planned features (multi-pane, plugin growth, larger terminals)
  (b) Structural improvement side effects (e.g., Salsa's primary value is maintainability)
  (c) Regression budget preservation
- Unjustified optimization is over-engineering

### Input-to-Photon Model

Keypress-to-pixel chain for TUI path:

```
keypress → terminal emulator → Kakoune → JSON-RPC → [Kasane] → terminal emulator render → display scanout
```

- Kasane controls only the bracketed segment
- Kasane's steady-state overhead (~59 μs CPU + ~49 μs backend) ≈ 0.1 ms — roughly 2-3% of the 240 Hz scanout period (4.17 ms)
- Even worst practical case (large viewport ~413 μs + backend I/O) stays under 1 ms
- The comparison baseline is native Kakoune, not zero latency — Kasane must not add perceptible overhead on top

### Challenges and Mitigations

| Challenge | Mitigation |
|---|---|
| Perception is probabilistic, not a sharp threshold | Layer 1 provides order-of-magnitude guidance; Layer 2 provides precise ratchets |
| Sub-threshold regressions accumulate invisibly | SLOs as ratchets + CI 115% threshold catch drift |
| Non-perceptual costs (power, resource contention) | Acknowledged as secondary considerations; do not override the perceptual compass |
| "Best hardware" is a moving target | Scope to current + next generation (240-480 Hz); revisit when display technology shifts |
| Composition problem (each component claims imperceptibility, sum is perceptible) | Kasane's budget defined as share of total chain (≤10-25%), not in isolation |

### Implications

- performance.md Principles restructured: Principle 3 (jitter) promoted T3→T1; Principles 9, 10 added at T2
- SLO values unchanged — they coincidentally align with the perceptual derivation
- Historical ADRs (010, 013, 015, 020) not retroactively reframed; policy applies prospectively
- Origin: vision.md line 68. This ADR develops it; performance.md operationalizes it.

## ADR-025: HandlerRegistry Plugin Architecture

**Status:** Current

### Context

- The original `Plugin` trait grew to 20+ methods, requiring every plugin to interact with the full trait surface even when most methods used defaults
- `PluginBridge` contained 343 lines of mechanical type-erasure boilerplate
- `PluginCapabilities` had to be manually declared, creating a maintenance burden and risk of stale declarations
- Adding a new extension point required touching the Plugin trait, PluginBackend trait, PluginBridge adapter, and all test doubles

### Decision

Replace the monolithic trait with a 3-method `Plugin` trait + `HandlerRegistry`:

```rust
pub trait Plugin: Send + 'static {
    type State: PluginState + PartialEq + Clone + Default;
    fn id(&self) -> PluginId;
    fn register(&self, registry: &mut HandlerRegistry<Self::State>);
}
```

Plugins call registration methods on `HandlerRegistry` (e.g., `r.on_annotate_background(...)`, `r.on_contribute(...)`, `r.on_key(...)`) to declare only the handlers they implement. The registry produces a `HandlerTable` — a type-erased dispatch table consumed by `PluginBridge`.

`PluginCapabilities` are auto-inferred from which handlers are registered: if `on_annotate_background` is called, `ANNOTATOR` is set; if `on_key` is called, `INPUT_HANDLER` is set; etc.

### Implications

- Entry barrier reduced: a minimal plugin (e.g., line numbers) needs only `register()` with `on_annotate_gutter()`
- New extension points are additive: add a registration method to `HandlerRegistry` and a field to `HandlerTable`; no existing trait methods change
- `PluginBackend` remains as the internal dispatch interface; `PluginBridge` adapts `Plugin` → `PluginBackend` via `HandlerTable`
- The `#[kasane_plugin(v2)]` proc macro generates `impl Plugin` with `register()` body from annotated module items

## ADR-026: ElementPatch Declarative Transforms

**Status:** Current

### Context

- `transform()` was an opaque `fn(TransformSubject) -> TransformSubject`, blocking Salsa memoization of transform results
- Debug-mode conflict detection required manual `TransformDescriptor` declarations that could diverge from actual behavior
- No algebraic simplification: an Identity transform still incurred dispatch overhead

### Decision

Introduce `ElementPatch` as a declarative transform algebra:

- Variants: `Identity`, `WrapContainer`, `Prepend`, `Append`, `Replace`, `ModifyFace`, `Compose`, `ModifyAnchor`, `Custom`
- `normalize()` — algebraic simplification (Identity removal, Replace absorption, Compose flattening)
- `apply()` — execute the patch against a `TransformSubject`
- `is_pure()` — true when no `Custom` variants are present (Salsa-memoizable)
- `scope()` — auto-infer `TransformScope` from variant (replaces manual `TransformDescriptor`)
- `impl Composable` — monoid with `Identity` as identity element

The transform chain collects `ElementPatch` from all plugins, composes them, normalizes, and applies. The `Custom` variant wraps `Arc<dyn Fn(TransformSubject) -> TransformSubject>` as an escape hatch for transforms that cannot be expressed declaratively.

### Implications

- Pure patches (no `Custom`) are data, enabling future Salsa memoization of composed transform results
- `TransformDescriptor` can be auto-derived from `ElementPatch::scope()` instead of manual declaration
- `Replace` algebraically absorbs all preceding patches, matching intuition
- Legacy `PluginBackend` transforms are wrapped in `Custom` for backward compatibility

## ADR-027: LineAnnotation Decomposition

**Status:** Current

### Context

- `annotate_line_with_ctx()` returned a `LineAnnotation` struct combining 5 independent concerns (gutter, background, inline decoration, virtual text, cell decoration) into one return value
- A plugin that only provided background highlighting still had to construct the full struct
- Composition rules differed per concern but were applied monolithically

### Decision

Decompose annotations into 4 independent annotation extension points, each with its own handler type and composition rule. Cell decoration was later consolidated into `on_render_ornaments` (see render ornament unification):

1. **Gutter** (`on_annotate_gutter`): `(GutterSide, priority, Fn(&S, usize, &AppView, &AnnotateContext) -> Option<Element>)` — priority-sorted, left/right placement
2. **Background** (`on_annotate_background`): `Fn(&S, usize, &AppView, &AnnotateContext) -> Option<BackgroundLayer>` — z-order-sorted, last wins
3. **Inline** (`on_annotate_inline`): `Fn(&S, usize, &AppView, &AnnotateContext) -> Option<InlineDecoration>` — first-wins with warning
4. **Virtual text** (`on_virtual_text`): `Fn(&S, usize, &AppView, &AnnotateContext) -> Vec<VirtualTextItem>` — merged
5. ~~**Cell decoration** (`on_cell_decoration`)~~ — consolidated into `on_render_ornaments` (physical decoration path unification)

`LineAnnotation` is retained for `PluginBackend` (Legacy/WASM backward compatibility); the bridge decomposes it into individual concerns.

### Implications

- Plugins register only the annotation types they produce — simpler API surface
- Per-plugin invalidation is granular: a plugin's background handler can be skipped when its relevant `DirtyFlags` haven't changed, even if another plugin's gutter handler is stale
- Each concern can evolve independently (e.g., adding multi-line gutter spans) without affecting the others

## ADR-028: WASM Capability Inference

**Status:** Current

### Context

- WASM plugins previously reported `PluginCapabilities::all()`, causing the host to dispatch every extension point call across the WASM boundary even for non-participating plugins
- Each unnecessary boundary crossing costs ~6-8 μs (measured in kasane-wasm-bench), significant for the per-frame budget

### Decision

Add `register-capabilities() -> u32` to the WIT interface. WASM plugins return a bitmask of `PluginCapabilities` bits for the extension points they actually implement. The host calls this once at plugin construction and caches the result.

The SDK macro (`define_plugin!`) auto-generates the bitmask by inspecting which handler functions the plugin provides, matching the native `HandlerRegistry` capability inference.

### Implications

- WASM plugins that only provide annotations skip transform, overlay, input, and display directive dispatch
- Fallback for plugins not implementing the export: `PluginCapabilities::all()` (safe, conservative)
- Bit layout matches the native `PluginCapabilities` bitflags exactly

## ADR-029: Topic-Based Pub/Sub and Plugin-Defined Extension Points

**Status:** Current

### Context

- Inter-plugin communication was limited to `PluginMessage` (untyped, point-to-point) and `ConfigEntry` (string key-value, delayed by one frame)
- Plugins could not define new extension points without framework source changes
- Common patterns (e.g., "broadcast current git branch to all interested plugins") had no clean expression

### Decision

Introduce two complementary mechanisms:

**Topic-based Pub/Sub** (`TopicBus`):
- `TopicId` identifies a topic (e.g., `"git.branch"`)
- Publishers register via `r.publish::<T>(topic, handler)`; subscribers via `r.subscribe::<T>(topic, handler)`
- Two-phase evaluation: (1) collect all publications, (2) deliver to subscribers
- Cycle prevention: publishing during delivery panics in debug, returns error in release
- Type-erased via `Box<dyn Any + Send>` with downcast at delivery

**Plugin-defined Extension Points** (`ExtensionPointId` + `CompositionRule`):
- `ExtensionPointId` identifies an extension point (e.g., `"lint.diagnostics"`)
- Defining plugin: `r.define_extension::<I, O>(id, rule)` with optional own handler
- Contributing plugins: `r.on_extension::<I, O>(id, handler)`
- `CompositionRule`: `Merge` (collect all), `FirstWins` (first non-empty), `Chain` (sequential pipe)
- Results collected via `PluginRuntime::evaluate_extensions()` returning `ExtensionResults`

### Implications

- Plugins can define new extension points without framework changes, enabling ecosystem-driven extensibility
- Pub/sub enables broadcast communication patterns without point-to-point message routing
- Type safety is runtime-enforced (downcast), not compile-time — mismatched types are silently filtered
- Both mechanisms integrate with the existing `PluginBackend` trait via default methods, keeping backward compatibility

## ADR-030: Observed/Policy Separation — Staged Projection Rollout

**Status:** Current (Levels 1–6 shipped)

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

Phase 5b — `EffectCategory` + `EffectFootprint`:
- `EffectCategory` bitflags (14 categories) with exhaustive
  `Command::effect_category()` classification method.
- `CASCADE_TRIGGERS` composite constant: `PLUGIN_MESSAGE | TIMER | INPUT_INJECTION`.
- `EffectFootprint` per-plugin footprint (local + transitive).
- `compute_transitive_footprints()` — least fixed point iteration on
  `(𝒫(EffectCategory), ⊆)`. Conservative approximation: plugins with
  `PLUGIN_MESSAGE` or `INPUT_INJECTION` inherit the global footprint union.
- Theoretical note: the design analysis found that T12's "free monad"
  claim is algebraically a free monoid (list). The correct framework is
  a graded monad `(𝒫(EffectCategory), ∪, ∅)` where each handler
  carries a grade (set of effect categories it may produce).

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

## ADR-031: Text Stack Migration — cosmic-text → Parley + swash, with Protocol Style Redesign

**Status:** Accepted — Parley + swash is the production stack as of
2026-04-26. The protocol-side `Style` redesign and plugin ABI break
landed across April 28–29 (Phase A.4 split `7fca4784`, B-wide
`98592a47`, Phase 4 Tier A `a5ef9f56`, Phase 5 Tier B `8f281f52` +
binaries `f4df0762`). All 50 workspace test suites and the full 188
`kasane-wasm` cases now pass against `kasane:plugin@1.0.0`.

**Landed:** Phases 0, 1a, 1b–d (B-wide), 2 (kasane-core type cascade
via Phase A.3), 4 (WIT 1.0.0 brush/style/inline-box), 5 (10 example
plugins + 6 bundled + 11 fixtures rebuilt + SDK 0.5.0 + HOST_ABI_VERSION
1.0.0), 6, 7, 8, 9, 9b (Step 4a–g + 4c L2 cache fix + frame-epoch
eviction guard), 10 (rich underlines via `RunMetrics::underline_*`,
glyph-accurate hit_test via Parley `Cluster::from_point`), and 11
(cosmic-text removal).

**Landed (continued, design-δ migration round):** Phase 3 design-δ —
`TerminalStyle` migrated from `kasane-tui` to `kasane-core::render::terminal_style`,
`Cell.face: Face` replaced by `Cell.style: TerminalStyle` (Copy, ~50 bytes,
SGR-emit-ready). The TUI backend reads `cell.style` directly, retiring
the per-cell `TerminalStyle::from_face(&cell.face)` projection that was
paid every frame on every visible cell. The GUI cell renderer
(`kasane-gui/src/gpu/cell_renderer.rs`) likewise reads `cell.style.fg/bg/reverse`
directly. `Face` survives only at the API surface (paint.rs, decoration,
theme, plugin API) and is bridged via `Cell::face()` / `Cell::with_face_mut`;
removing those bridges is Phase B3, tracked separately. atom→wire
`Style::from_face(&a.face())` round-trip in `kasane-wasm/src/convert/mod.rs`
also retired (now `style_to_wit(&a.style_resolved_default())` direct).
Phase 10 host-side InlineBox paint extension landed earlier (Phase 10
Step 2-renderer A–D, commits `26e392a8`–`a019a169`); this round added
the `define_plugin!` `paint_inline_box(box_id) { body }` macro section
parser and host-side recursion-depth (≤ 8) + cycle detection in
`PluginView::paint_inline_box`, so bundled WASM plugins can override
paint and the host is robust to malicious / buggy reentrancy. Phase 10
hit_test coverage extended with RTL Arabic / combining-mark /
ZWJ-emoji / trailing-position cases. L1 LayoutCache negative tests
added for decoration colour, decoration thickness, and strikethrough
colour (paint-time invariants). ShadowCursor × InlineBox boundary
condition pinned in `docs/semantics.md`.

**Landed (Phase B3, commits 1-5/7):** Plugin extension points
de-Faced. `KakouneRequest` enum fields migrated from `Face` to
`Arc<UnresolvedStyle>` (commit `bca4d5b5`); `element::Style` enum
renamed to `ElementStyle` and its `Direct(Face)` variant replaced by
`Inline(Arc<UnresolvedStyle>)` (commits `930d1132` + `2c56f610`);
`Element::plain_text(s)` + `Atom::plain(s)` introduced and 316
`Face::default()` boilerplate references collapsed
(`11c5ddea`); `ElementPatch::ModifyFace`/`WrapContainer{face}` →
`ModifyStyle`/`WrapContainer{style}` with `Arc<UnresolvedStyle>`
field types and Salsa-friendly content-based `Hash`/`Eq`
(`b4445770`); `BackgroundLayer.face` and `CellDecoration.face` migrated
to `style: Style` so plugin annotation/decoration extension points
expose only the post-resolve `protocol::Style`
(`844fff10` + `846ca960`); `Cell::with_face_mut`/`set_face` retired
in favour of `Cell::with_style_mut<F: FnOnce(&mut TerminalStyle)>`
operating directly on the cell-grid representation, eliminating the
`TerminalStyle ↔ Face ↔ bitflags` round-trip on every decoration /
ornament merge (`05c0be16`). Performance (post-merge): warm 64.4 µs
(−1.0 % vs Phase 11 case A baseline), one_line_changed 81.6 µs
(−3.3 %) — both directions improvement, neither metric regresses
the Phase 11 closure framework.

**Landed (Phase B3 Style-native cascade, branch `feat/parley-color-emoji-test`):**
A five-PR sequence pushed `Style` / `TerminalStyle` end-to-end through
the menu, info, status, buffer, and cursor render paths:

- `54a466b7` (PR-1) — retired the `ColorResolver` `Style → Face → Style`
  round-trips on the GPU `FillRect` / `DrawBorder` / `DrawBorderTitle`
  / `DrawPaddingRow` paths and the dead-code `scene_graph.rs`
  scaffold. The 817b61da migration in Phase A had only covered the
  paragraph paths; this commit closed the remaining four matchers and
  the `dummy_resolve` test fixture.
- `34f30e54` (PR-2) — `Theme` API became `Style`-native. `set` / `get`
  / `resolve` (Face fallback) / `resolve_with_protocol_fallback`
  retired in favour of `set_style` / `get_style` / `resolve(_, &Style)
  → Style`. The four production callers (`view/info.rs`,
  `view/menu.rs`, `view/mod.rs ×2`) all already held a `Style` ready
  (`info.face`, `menu.menu_face`, `state.observed.status_default_style`),
  so the migration eliminated a Style→Face→Style round-trip on every
  status / menu / info repaint. `AppView::theme_face` →
  `theme_style(token) -> Option<&Style>`.
- `7815e3c2` (PR-3a) — `view/info` / `view/menu` / `view/mod` /
  `salsa_views/{info,menu,status}` / `render::builders` helpers
  (`truncate_atoms`, `wrap_content_lines`, `build_content_column`,
  `build_scrollbar`, `build_styled_line_with_base`) consume `&Style`.
  ~12 `Style::from_face(&face)` round-trips collapsed to direct
  `style.clone()` ownership; the docstring portion of split menu
  items now uses `resolve_style(&atom.style, &style)` instead of
  `Style::from_face(&resolve_face(&atom.face(), &face))`.
- `eba04c4a` (PR-3b) — `CellGrid` mutation API takes `&TerminalStyle`
  (`clear` / `clear_region` / `fill_row` / `fill_region` / `put_char`),
  matching the internal `Cell.style: TerminalStyle` storage.
  `put_line_with_base(_, _, _, _, base_style: Option<&Style>)` uses
  `resolve_style` on the atom's existing `Arc<UnresolvedStyle>` and
  converts to `TerminalStyle` once per atom rather than once per
  grapheme. `paint_text` / `paint_shadow` / `paint_border` / 
  `paint_border_title` cache one `TerminalStyle` per call site.
- `6ce6e75b` (PR-3c) — `process_draw_text` / `emit_text` / `emit_atoms`
  / `emit_decorations` consume `&Style`. `emit_decorations`
  reads `style.underline.style: DecorationStyle` and
  `style.strikethrough` directly instead of the
  `face.attributes.contains(Attributes::*UNDERLINE*)` bitflag cascade.
  Underline / strikethrough thickness now also honour the per-decoration
  `TextDecoration.thickness: Option<f32>` override (previously only
  the metrics-derived default was used).

The `Atom::from_face` test cascade noted as ~250 refs in the previous
status was already complete pre-branch: Block E commits `75439f1f` +
`3724556f` migrated all post-resolve sites; the 13 remaining
`Atom::from_face` callsites are correctly wire-aware (cursor_face with
`FINAL_FG`, detect_cursors fixtures, parser, `test_support::wire`).

**Pending Phase B3 commits 6-7/7 (bridge deletion):** The remaining
bridge cleanup covers internal types only (no further plugin-visible
API change): retire `Style::from_face` / `to_face` /
`to_face_with_attrs`, `UnresolvedStyle::to_face`, `Atom::face`,
`Cell::face()`, `From<Face> for Style` / `for ElementStyle`,
`TerminalStyle::from_face`, and the
`kasane-tui::sgr::emit_sgr_diff(Face)` shim. `UnresolvedStyle::from_face`
and `Atom::from_face` (wire-aware) stay — they are the construction
shape used by the Kakoune protocol parser and the `final_*`-preserving
cursor fixtures. Estimated cascade: ~30 production callsites (mostly
inline `Face { ... }` literals in `render/cursor`, `inline_decoration`,
`markup`, `walk_scene`, `ornament`, `halfblock`, plugin / widget
defaults) plus ~150 test refs. Best executed compile-error-driven
after deletion — uniform migration shape avoids per-site stylistic
divergence. After those retire, `Face` / `Color` / `Attributes`
downgrade from `pub` to `pub(in crate::protocol)` so wire-format types
are physically isolated to the parse boundary.

**Other pending items.** Phase 10 — bundled `color-preview` WASM plugin
upgraded to use real `paint_inline_box` (ergonomics demonstration,
moves the variable-font / inline-box features from "contracted but
unused" to "exercised end-to-end"). Phase 12 golden image coverage
beyond the 80×24 ASCII baseline pinned at `a2ca6834` (CJK / cursor /
selection — recommended path: move under ADR-032 W2 since that
work pays off regardless of Vello adoption). cosmic-text element
regression tests for `2f7c0ab9` (RTL cursor double-render) and
`4d48bbd9` (CJK cursor width clamp) — not blocking ADR-031 closure
but hardens the motivation cited in §動機 (1).

**Supersedes (text stack only):** [ADR-014](#adr-014-gui-technology-stack--winit--wgpu--glyphon) §14-1's selection of glyphon (cosmic-text + swash + etagere). Window management (winit) and GPU API (wgpu) are unchanged. The atlas allocator (etagere) and the swash rasterizer are retained — only cosmic-text's layout/buffer abstraction and the glyphon-derived text pipeline are replaced.

### Context

ADR-014 selected glyphon in 2024 because cosmic-term (the COSMIC Desktop terminal) demonstrated proven monospace grid rendering on the same stack, and Vello was rejected for lacking a glyph cache, having unstable APIs, and requiring compute shaders.

Operational experience since then has surfaced four limitations of the cosmic-text portion of the stack:

1. **Internal layout maintenance velocity.** cosmic-text implements its own bidi/script-segmentation layout layer in safe Rust. Recent fixes for RTL cursor double-rendering (`2f7c0ab9`) and CJK cursor width clamping (`4d48bbd9`) were symptomatic patches over the layout layer; an ICU4X-based layout would have eliminated the underlying class of bug.
2. **No inline widget primitive.** `DisplayDirective::InsertInline` currently materialises virtual text as cell-grid-level atoms, which interacts awkwardly with display column accounting. Parley exposes `inline_box(width, height)` as a first-class layout primitive, dissolving the impedance mismatch.
3. **Decoration expressiveness.** The current pipeline hard-codes four underline styles via `quad_pipeline.rs::DECO_*` quads with `cell_h * 0.2` amplitude. cosmic-text does not surface per-font underline metrics; Parley's `LineMetrics::underline_offset/size` does.
4. **Variable font support.** cosmic-text exposes weight as a discrete enum (`Weight::BOLD` etc.). Parley accepts continuous `FontWeight(u16)` and arbitrary `FontVariations`, opening LSP semantic highlighting use cases that the current API cannot represent.

The Linebender ecosystem has matured during 2025-2026: Parley v0.5 ships with full UAX#9 bidi via ICU4X, Bevy migrated from cosmic-text to Parley, an egui PR is in flight, and CuTTY (Alacritty fork ported to Vello + Parley) demonstrates that Parley handles terminal-class workloads. The ADR-014 critique of Vello (no glyph cache, compute shader requirement) does **not** apply to Parley used directly with swash and an existing atlas — that combination preserves Kasane's L1/L2/L3 caching architecture.

A user-facing constraint reinforces the timing: any new feature added to the text path (rich underline, variable font, inline boxes) requires plugin authors to update plugins regardless of the choice of layout engine. Bundling the migration with these features amortises the disruption into a single ABI break instead of three sequential ones.

### Decision

Adopt the full Linebender text stack: **Parley** (layout) + **HarfRust** (shaping, internal to Parley) + **Skrifa** (font analysis) + **Fontique** (font discovery) + **ICU4X** (bidi/segmentation) + **swash** (rasterization, called directly). Remove `cosmic-text` from the workspace.

Concurrently redesign the protocol-level text representation across `kasane-core`, `kasane-tui`, `kasane-wasm`/WIT, and all bundled plugins. **No backward compatibility is preserved** for internal types or the WIT plugin ABI; the Kakoune wire format (which Kasane does not control) is the only invariant.

| Library | Role | Replaces |
|---------|------|----------|
| Parley | Rich text layout, line breaking, bidi runs, glyph positioning, inline boxes | cosmic-text `Buffer` / `LayoutRun` |
| HarfRust | Shaping engine (called by Parley) | rustybuzz (called by cosmic-text) |
| Skrifa | Font table parsing | swash internal (overlapping) |
| Fontique | Font discovery, fallback chains | cosmic-text `FontSystem` + fontdb |
| ICU4X | Unicode bidi / grapheme / line break | cosmic-text custom implementation |
| swash | Glyph rasterization (called directly, not via SwashCache) | cosmic-text `SwashCache` |
| etagere | Texture atlas packing (retained) | — |
| wgpu, winit | GPU and window (retained) | — |

### Type Redesign

A canonical `Style` type replaces the two coexisting representations (`Face` + `cosmic_text::Attrs`):

```rust
// kasane-core/src/protocol/style.rs (new)
pub struct Style {
    pub fg: Brush,
    pub bg: Brush,
    pub font_weight: FontWeight,                       // u16, 100..=900
    pub font_slant: FontSlant,                         // Normal | Italic | Oblique
    pub font_features: FontFeatures,                   // bitset
    pub font_variations: SmallVec<[FontVariation; 2]>,
    pub underline: Option<TextDecoration>,
    pub strikethrough: Option<TextDecoration>,
    pub letter_spacing: f32,
    pub bidi_override: Option<BidiOverride>,
    pub blink: bool,
    pub reverse: bool,
}
pub enum Brush { Default, Solid([u8; 4]), Named(NamedColor) }
pub struct TextDecoration {
    pub style: DecorationStyle,    // Solid | Curly | Dotted | Dashed | Double
    pub color: Brush,
    pub thickness: Option<f32>,    // None = font metrics
}

// kasane-core/src/protocol/message.rs (redesigned)
pub struct Atom { pub text: CompactString, pub style: Style }
```

The TUI backend consumes a `TerminalStyle` projection of `Style` (continuous `FontWeight` collapses to bool, `FontVariations` are dropped, `Brush` resolves to the closest 24-bit / 256-colour / 16-colour value). The WIT plugin ABI mirrors `Style` / `Brush` / `TextDecoration` and bumps to a major version. Old plugin binaries are rejected at load time; bundled plugins (`examples/wasm/*`, `examples/line-numbers/`, `examples/virtual-text-demo/`, `examples/image-test/`) are rewritten against the new SDK as part of the migration.

### GPU Pipeline Redesign

```
StyledLine                              kasane-gui/src/gpu/parley_text/styled_line.rs
   │                       (atom stream + base style + InlineBoxSlot table)
   ▼
[L1 LayoutCache]            line_idx → Arc<ParleyLayout>           parley_text/layout_cache.rs
   ▼                       Wholesale invalidate on font/metrics change.
ParleyLayout                                                         parley_text/layout.rs
   │
   ▼
[GlyphRasterizer]           swash::scale::ScaleContext (1 per app)  parley_text/glyph_rasterizer.rs
   │                       Subpixel x quantised to 4 levels (0,1/4,2/4,3/4).
   ▼                       Color emoji via Source::ColorOutline.
[L2 GlyphRasterCache]       (font_id, glyph_id, size_q, subpx_x,    parley_text/raster_cache.rs
   │                        var_hash, hint) → bitmap + atlas slot.
   ▼                       L2 ↔ L3 bidirectional evict link.
[L3 AtlasShelf]             etagere allocator + LRU (retained)      parley_text/atlas.rs
   ▼
GlyphInstance → wgpu pipeline (vertex layout retained)              parley_text/text_pipeline.rs
```

L1 invalidation triggers (font_size / metrics / max_width / context generation) match the existing `LineShapingCache` semantics, so cursor-only frame hit-rate (≥ 90%) is preserved. The 3-tier separation gives sharing across lines for hot glyphs, which the existing 2-tier (Buffer slot + SwashCache) cannot.

### Phased Execution

13 phases, ~14 weeks, each terminating in a `parley-phase-N` git tag for partial revert capability. Detail in `/home/kaki/.claude/plans/majestic-bubbling-planet.md` (planning artefact, not a project file).

| Phase | Scope | Duration |
|---|---|---|
| 0 | Capture pre-parley benchmark baselines (4 scenarios), draft this ADR | 3-4 days |
| 1 | Design and implement `Style` / `Atom` / `Brush`; rewrite Kakoune protocol parser; update kasane-core unit tests | 2 weeks |
| 2 | Migrate kasane-core internal types (DrawCommand, BufferParagraph, CellGrid, DisplayDirective, widgets, state) | 2 weeks |
| 3 | Update kasane-tui (`emit_sgr_diff` → TerminalStyle) and TUI bench | 1 week |
| 4 | Redesign WIT plugin ABI (style record, decoration record, brush variant), regenerate SDK bindings | 1 week |
| 5 | Rewrite all 10 bundled WASM plugins, native example, virtual-text-demo, image-test against new SDK | 1 week |
| 6 | Add Parley/swash/fontique/skrifa/icu deps to kasane-gui, scaffold ParleyText facade | 0.5 week |
| 7 | Implement StyledLineBuilder, ParleyShaper, ParleyLayout, L1 LayoutCache, port line_cache.rs tests | 1.5 weeks |
| 8 | Implement GlyphRasterizer (swash direct), L2 GlyphRasterCache, L2↔L3 evict link, new text pipeline | 1 week |
| 9 | Switch SceneRenderer to Parley path (cosmic-text retained behind `KASANE_TEXT_BACKEND` for A/B verification only) | 1 week |
| 10 | Implement 5 features: RTL hit-test (ICU4X cluster), InlineBox (parley `push_inline_box`), Variable font, Subpixel positioning, rich underline (Parley `LineMetrics`) | 1 week |
| 11 | Remove cosmic-text from Cargo.toml, delete legacy text_pipeline / line_cache, hot-path optimisation, baseline comparison | 1 week |
| 12 | Documentation finalisation, golden image test skeleton (4 scenarios), CHANGELOG | 1 week |

### Performance Targets

Captured at Phase 0 against current main; verified at Phase 11 against the same machine.

| Metric | pre-parley baseline | Phase 11 target |
|---|---|---|
| 80×24 mean (`gpu/cpu_rendering`) | ~57 μs | ≤ 70 μs (+23 %) |
| 200×60 mean | TBD@Phase 0 | pre-parley + 25 % |
| 95p latency | TBD@Phase 0 | regression ≤ +30 % |
| iai-callgrind instructions | TBD@Phase 0 | pre-parley + 10 % |
| L1 hit-rate (cursor-only frame) | (existing `LineShapingCache`) | ≥ 90 % |
| Atlas memory @1080p | (current) | ≤ 1.5× (4-step subpixel cost) |

The +23 % CPU budget reflects the deliberate trade: Parley's richer feature set (variable font axes per glyph, ICU4X bidi runs, inline box layout, real subpixel positioning) is paid for in steady-state cost. ADR-024 (perception-oriented performance policy) governs the acceptability of the new absolute number — 70 μs at 80×24 remains comfortably below the 16 ms frame budget.

### Rejected Alternatives

| Alternative | Reason for rejection |
|---|---|
| Parley with the existing `Atom { face: Face }` retained | Forces a permanent `Face → parley::StyleProperty` adapter layer with bitflags-to-structured-style translation on every line. Doubles the impedance mismatch instead of resolving it. |
| Phase-0-only spike with no full migration commitment | The user explicitly opted out of backward compatibility; partial commitment leaves two parallel face systems indefinitely. |
| Vello (full compute path) | Per ADR-014: requires compute shaders, no glyph cache, no API stability. Mismatched with cell-grid + glyph workload. Re-evaluation possible after Vello 1.0; orthogonal to this ADR. |
| Migrate text layout only, defer protocol/WIT redesign | Plugin authors face two ABI breaks (one for Parley features, one for protocol cleanup) instead of one. Worse for the plugin ecosystem. |
| Stay on cosmic-text and patch around limitations | Layout-layer maintenance velocity is the primary motivator; patching extends the velocity gap rather than closing it. |

### Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Parley v0.5 → v0.6 introduces breaking changes mid-migration | `Cargo.lock` pinned for the entire 14-week window; version bump deferred to a follow-up issue |
| Performance target unmet at Phase 11 | Phase 9 abort gate (>50 % regression triggers Phase 11 micro-opt block to be pulled forward); follow-up issue for residual regression |
| ICU4X binary size increase | Release build strip + LTO; tolerated +15 MB |
| Parley shape/raster differences vs cosmic-text on niche fonts | Issue tracker for per-font reports; minimum repros required |
| WASM plugin authors disrupted | Bundled plugins all rewritten in Phase 5 as worked examples; ADR-031 referenced from CHANGELOG |
| 14-week schedule overruns | Each phase tag is an interruptable boundary; partial merges acceptable |
| Subpixel atlas growth (4× entries per glyph) | Strict L2 LRU bound; profiling-driven cap adjustment |

### Implications

- **ADR-014 §14-1 is partially superseded.** The text rendering portion of the GUI stack is replaced; the ADR-014 row in the Decision Summary is updated to point here. ADR-014's window/GPU portion (winit + wgpu) and the shared etagere atlas remain authoritative.
- **WIT plugin ABI breaks.** Plugin authors must rebuild against the new SDK; the host rejects the old binary format at load time.
- **Kakoune wire format is unchanged.** The Kakoune ↔ Kasane interaction is invariant under this ADR; only the internal Kasane-side representation of styled atoms is redesigned.
- **TUI behaviour preserved within representable limits.** The Style → TerminalStyle projection is lossy (continuous weight → bold bool, variations dropped, brushes quantised to terminal palette). Where the current TUI displays a face, the new TUI displays the same approximation.
- **Five new features ship together.** RTL/Bidi hit-testing, inline boxes, variable font axes, subpixel positioning, and rich underline (curly/dotted/dashed/double with font-metric-driven amplitude) become available to plugins via the redesigned WIT and Style API.
- **Vello adoption is unblocked, not committed.** Migrating text to Parley reduces the integration cost of a future Vello evaluation, but Vello adoption itself is out of scope for this ADR.

### Phase 10 Wire Shape (paper design, 2026-04-28)

This sub-section freezes the wire-shape decisions for the five Phase 10 features so Phase 4 (WIT redesign) can be implemented in one pass. Phase 4 may not introduce features beyond what is listed here without a follow-up ADR; doing so would re-create the "two ABI breaks" trap that ADR-031 §動機-5 was written to prevent.

#### Decision summary

| Feature | Plugin visibility | WIT additions | Host plumbing |
|---|---|---|---|
| 1. RTL/Bidi hit_test | host-internal | none | Already done (Phase 7 / `parley_text/hit_test.rs`) |
| 2. InlineBox | plugin-visible | new `inline-box-directive` variant | Type exists (`StyledLine::inline_boxes`); plumbing TBD |
| 3. Variable font axes | plugin-visible | `font-variations: list<font-variation>` field on `style` | Already in `Style::font_variations`; plumbing TBD |
| 4. Subpixel positioning | host-internal | none | Already in pipeline (4-step quantisation in `glyph_rasterizer.rs`) |
| 5. Rich underline (font-metric thickness) | plugin-visible | `text-decoration` record (replaces `attribute-flags`-based underline encoding) | Already in `TextDecoration::thickness`; plumbing done |

#### 1. RTL/Bidi hit_test (host-internal — no WIT change)

Glyph-accurate paragraph hit testing was completed in Phase 7 (`fd8995c7 feat(gui): glyph-accurate paragraph hit_test + L1 layout cache wiring`). Plugins do not need to express bidi semantics — Parley + ICU4X handles run direction inference from strong characters in the atom text. The `bidi_override` field on `Style` (already present, host-internal) covers the rare case where a plugin wants to force a direction; it is **not** exposed through WIT in Phase 4 because no current plugin needs it. A future ADR may surface it if a use case appears.

#### 2. InlineBox (`inline-box-directive`)

WIT addition:

```wit
record inline-box-directive {
    line: u32,
    byte-offset: u32,
    /// Width in display columns (cell units). The host converts to pixels
    /// using the current cell metrics. Plugins do not see physical pixels.
    width-cells: f32,
    /// Height in fractional lines. 1.0 = single line; 2.0 = double-tall.
    height-lines: f32,
    /// Stable identifier — typically a hash of `(plugin-id, content-id)` —
    /// the host uses this to look up the actual paint content via a
    /// separate plugin extension point. Phase 5 wires the lookup; for now
    /// the directive only declares the slot.
    box-id: u64,
    /// Baseline alignment within the inline box. `Center` matches what
    /// Parley's `push_inline_box` produces by default; `Top` and `Bottom`
    /// are exposed for plugins that paint glyphs (e.g. tall icons) that
    /// have explicit baseline expectations.
    alignment: inline-box-alignment,
}

enum inline-box-alignment { center, top, bottom }
```

Decisions:

- **Width is in cell units, not pixels.** Plugins reason in display columns (the same unit Kakoune uses for column positions). The host applies cell-size multiplication so that font-size changes do not break plugin code. This matches the rest of the WIT API (e.g. `cursor-pos` uses display columns).
- **Height is in lines (f32).** Allows fractional values for sub-line decorations while keeping `1.0` as the obvious default. Most plugins (color preview, image preview) will use `1.0`.
- **No `content` field on the directive.** The directive only *declares the slot*. Painting the inside of the box happens through a separate `paint-inline-box(box-id) -> element-handle` extension point added in Phase 5. This keeps the directive small (no nested element trees in the protocol) and lets the renderer query content lazily on first paint.
- **`box-id` is plugin-supplied.** Plugins are responsible for choosing identifiers that are stable across re-runs (`(plugin-id, content-fingerprint)` is the canonical recipe). The host uses `box-id` as part of the L2 cache key for inline-box paint output so re-renders with identical boxes are zero-cost.
- **Rejected: nested `Vec<atom>` content.** The current `DisplayDirective::InsertInline { content: Vec<Atom>, .. }` host-internal shape is *kept* for non-WIT plugins (native plugins) but **not** mirrored to WIT. WASM plugins that want to render text inline use the regular atom system (`StyleInline` for span colouring); the `inline-box-directive` is reserved for content that does not fit the atom model (color swatches, images, custom widgets).

#### 3. Variable font axes

WIT addition (extension to existing `style` record):

```wit
record font-variation {
    /// 4-byte OpenType axis tag (e.g. `wght`, `wdth`, `slnt`).
    /// Encoded as a u32 with bytes in big-endian order so `wght` is
    /// `0x77676874`. Plugins typically use a helper constant.
    tag: u32,
    value: f32,
}

record style {
    fg: brush,
    bg: brush,
    font-weight: u16,
    font-slant: font-slant,
    font-features: u32,            // bitset (existing)
    font-variations: list<font-variation>,
    underline: option<text-decoration>,
    strikethrough: option<text-decoration>,
    letter-spacing: f32,
    blink: bool,
    reverse: bool,
}
```

Decisions:

- **`tag` is `u32`, not `string` or `tuple<u8,u8,u8,u8>`.** A `u32` is canonical OpenType encoding, fits in a single WIT primitive, and is what `parley::FontVariation` already accepts. Plugins that prefer tag literals can wrap with an SDK helper (`tag!("wght") = 0x77676874`).
- **`list<font-variation>` is allowed to be empty.** Empty list is the "no variations" default; common case stays free. The list is bounded by the OpenType spec at 64K entries, but Kasane's host enforces a practical cap of 16 (asserted at deserialisation time).
- **No `min`/`max`/`default` axis metadata on the WIT side.** Plugins are expected to know valid axis ranges for the fonts they target; the host does not validate. Out-of-range values produce visually-clamped output (font-engine behaviour). Documented in `docs/plugin-development.md`.
- **`font-weight: u16` stays continuous (100..=900).** Replaces the legacy boolean BOLD bit. Plugins that only want bold/normal use the existing constants (`WEIGHT_BOLD = 700`, `WEIGHT_NORMAL = 400`) exposed in the SDK.

#### 4. Subpixel positioning (host-internal — no WIT change)

Subpixel positioning is a property of the *renderer*, not of the *style*. Plugins specify glyphs and positions in display-column space; the host renders them with whatever subpixel quantisation the GPU pipeline supports (currently 4 steps, set in `glyph_rasterizer.rs`). No WIT exposure.

#### 5. Rich underline (font-metric thickness)

WIT addition:

```wit
record text-decoration {
    style: decoration-style,
    color: brush,
    /// Stroke thickness in physical pixels. `None` means "use the font's
    /// recommended thickness from its metrics" — this is the behaviour
    /// that replaces the legacy hard-coded `cell_h * 0.2` in
    /// `quad_pipeline.rs`. Phase 10 step 1 already wires
    /// `RunMetrics::underline_offset/size`; this WIT field exposes the
    /// same control to plugins.
    thickness: option<f32>,
}

enum decoration-style { solid, curly, dotted, dashed, double }
```

Decisions:

- **`thickness: option<f32>`.** `None` is the strongly-preferred default — plugins should let the font engine pick. Explicit thickness is for special cases (LSP error pulse, draft markers) where the visual loudness must be controlled independently of font metrics.
- **`color: brush` not `option<brush>`.** A `Brush::Default` already encodes "inherit from text foreground", so wrapping in `option` would be redundant. Plugins that want the underline colour to follow `fg` set `color: brush::default-color`.
- **Decoration colour vs. underline colour at the directive level.** The legacy `face` record has a single `underline: color` field; the new `text-decoration` separates `style`, `color`, `thickness`. The legacy field is dropped from WIT in Phase 4 with no compatibility shim — bundled plugins are rewritten in Phase 5.

#### Out of scope for Phase 4

- **`bidi_override`** (forced direction) — host-internal field on `Style` only. Surfaced if a plugin requests it.
- **`letter_spacing`** — already in WIT (`f32`), but not exercised by any bundled plugin; documented as low-priority.
- **`final_*` resolution flags** — never exposed to plugins. Plugins receive the post-resolve `Style` (per ADR-031 Phase A.4 split, `7fca4784`); resolution semantics are a host concern.

#### Phase 4 implementation gates

A Phase 4 PR is acceptable when:

1. The WIT file at `kasane-wasm/wit/plugin.wit` declares all five additions above with the exact field names and types specified.
2. WIT version bumps from `0.25.0` to `1.0.0` (major bump signalling ABI break).
3. The host implementations in `kasane-wasm/src/host/*` deserialise the new shapes without a Face-bridge fallback path (compile-time-only support; old WASM binaries must reject at load time).
4. The generated bindings in `kasane-plugin-sdk/src/*` expose the new types as Rust idioms (e.g. `font_variation!("wght", 350.0)` macro).
5. Phase 5 (bundled WASM rewrite) starts immediately after — Phase 4 PR landing with old plugins still in `bundled/` is a known broken state and must not last across a calendar day.

### Phase 11 perf-tune — closure framework (accepted, 2026-04-29)

This sub-section applies [ADR-024](#adr-024-perception-oriented-performance-policy) to the Phase 11 typing-pattern gap so the perf-tune workstream has a defined stopping condition rather than open-ended pursuit of the original 70 µs target.

**Measurement (2026-04-29, post Phase 11 case A):**

| Bench | Time | Phase 11 target | Δ vs target |
|---|---|---|---|
| `parley/frame_warm_24_lines` | 64.9 µs | ≤ 70 µs | ✓ −7.3% |
| `parley/frame_one_line_changed_24_lines` | 83.8 µs | ≤ 70 µs | +19.7% |
| `parley/shape_warm` | 13.58 µs | (component) | — |

**Re-measurement (post Phase B3 commits 1-5, 2026-04-29):** the cell
hot-path consolidation in Phase B3 commit 5 (`05c0be16`) eliminates
the `TerminalStyle ↔ Face ↔ bitflags` round-trip on every decoration
/ ornament merge:

| Bench | Time | Δ vs Phase 11 case A |
|---|---|---|
| `parley/frame_warm_24_lines` | 64.4 µs | −0.8% |
| `parley/frame_one_line_changed_24_lines` | 81.6 µs | −2.6% |

Both directions improve — the warm-frame win is small because the
default rendering path is decoration-light, but the typing-pattern
metric (which the Phase 11 closure framework treated as structurally
bounded) shrinks by 2.2 µs, narrowing the gap toward the 70 µs target
without crossing it. The closure framework remains in force (the
remaining ~12 µs is still bounded by `shape_warm` + L1-miss raster);
nothing about the ADR-024 Layer 3 acceptance changes.

**Structural lower bound.** The typing-pattern measurement decomposes as:

```
83.8 µs ≈ 23 hits × (64.9 / 24 µs) + 1 miss × (2.7 + shape_warm + new_glyph_raster)
       ≈ 62.2 + 2.7 + 13.58 + ~5
       ≈ 83.5 µs
```

Closing the residual ~14 µs requires reducing `shape_warm` itself (Parley-internal optimisation, upstream-dependent) or eliminating the L2 raster lookup for newly introduced glyphs. Neither is reachable through structural rewrites in `kasane-gui`.

**Layer 1 (perceptual compass) evaluation.** Per ADR-024 §Input-to-Photon Model, Kasane's overhead must be imperceptible against a 240 Hz scanout period (4.17 ms). The 83.8 µs typing-frame total is **2.0 % of the scanout period and 0.5 % of the 16.7 ms / 60 Hz frame budget** — well below any plausible perceptual threshold for a single-line edit. The +19.7 % over the 70 µs *engineering target* does not manifest as +19.7 % over any *perceptual* budget.

**Layer 3 (optimisation accountability) evaluation.** Continuing to push `frame_one_line_changed_24_lines` below 70 µs would require:

- Either a Parley upstream change to reduce `shape_warm` (out of Kasane's control), or
- A structural rewrite of L1 cache key invalidation to share shape state across line-content edits (high complexity, plausibly perf-positive but loses correctness guarantees), or
- Accepting that ADR-031's adoption of Parley has a fixed per-shape cost that the original 70 µs target did not anticipate.

ADR-024 Layer 3 requires below-threshold optimisation to state justification. None of (a) headroom for planned features, (b) structural improvement side effects, or (c) regression budget preservation applies to the residual 14 µs — the gap is bounded, the absolute number is imperceptible, and further work would be unjustified per Layer 3.

**Closure decision.** Phase 11 perf-tune closes when:

1. `parley/frame_warm_24_lines` stays within ≤ 70 µs (the steady-state target). **Met.**
2. `parley/frame_one_line_changed_24_lines` is documented and accepted as structurally bounded by `shape_warm`. The ≤ 70 µs target is reframed from "must achieve" to "warm-baseline-only". **This sub-section is the documentation.**
3. The CI 115% alert threshold (ADR-024 Layer 2) continues to catch regressions on both metrics. **In place.**

**What this does not do.** This closure does not re-baseline the 70 µs target downward, retire the typing-pattern bench, or remove the entry from `docs/performance.md`. The bench remains a regression ratchet (Layer 2). The acceptance is specifically for the **gap between the engineering ratchet and the original Phase 11 target**, on the basis that the gap is structurally bounded and perceptually invisible.

### Next-ADR seeds (open hand-offs after ADR-031 closes)

ADR-031 leaves five distinct workstreams open. Each has been considered during the migration but is out of scope for this ADR; future change here without a follow-up ADR would re-create the "two ABI breaks" trap §動機 (5) was written to prevent. The seeds are recorded so a future engineer (human or automated) can pick them up without re-discovering the constraints.

| Seed | Trigger | Constraint to honour |
|---|---|---|
| **WIT 2.0** | A required feature cannot be expressed under WIT 1.x's "additive only" rule (record / variant change). Candidates: `bidi_override`, `letter_spacing` enrichment, font-variation axis metadata, hierarchical Style cascade. | Bundle multiple breaking shapes into a single major bump; do **not** ship 1.x.y minor breaks like Phase 10's ABI 1.1.0. |
| **Atom interner** | `dhat-rs` measurement shows per-Atom `Arc<UnresolvedStyle>` allocation as the dominant alloc source. The hypothesis is unverified; do not start without measurement. | Thread-local interner with `Weak<UnresolvedStyle>`, per-line flush. Verify cross-thread Salsa correctness; the StyleStore mutex hypothesis was already refuted (B-wide commit `98592a47`). |
| **Display ↔ Parley canonical coordinate utility** | Bundled `color-preview` upgrade to real `paint_inline_box` exposes the third or fourth ad-hoc `display_col → byte → parley_cluster` round-trip in paint sites. | Single canonical utility in `kasane-core/src/display/coord.rs` (or similar). Pin the conversion direction; ad-hoc per-site logic is a bug magnet for inline-box and folded-region edge cases. |
| **Atlas pressure policy** | `glyphs_dropped_atlas_full` counter (`raster_cache.rs:103-107`) fires in production. Currently observability-only with once-only warn; no automatic mitigation. | First action: subpixel quantisation 4 → 2 step under pressure (frame-level, with hysteresis). Document the visible-quality trade in `semantics.md`. |
| **Vello adoption (ADR-032)** | Vello ≥ 1.0 stable + Glifo ≥ 0.2 published + spike `frame_warm_24_lines` ≤ 70 µs at 80×24 (per `roadmap.md` §2.2). | ADR-032 W3 (`GpuBackend` trait) and ADR-032 W2 (golden image harness) land independently as decision-grade artefacts whether or not the spike is positive. The spike crate stays out of `members` until adoption is committed. |

These are also tracked in `docs/roadmap.md` §2.2 where they overlap with backlog entries; the table above is the design-rationale anchor that survives roadmap reorganisations.

## ADR-032: GPU Rendering Strategy — Vello Evaluation Framework

**Status:** Proposed (2026-04-28). This ADR establishes a re-evaluation framework for Vello adoption; it does **not** commit to migration. The decision artefact (§Spike Findings) is filled in by a 5-day timeboxed spike. The current GUI stack (winit + wgpu + Parley + swash; ADR-031) remains the production renderer until and unless this ADR is updated to "Accepted with adoption plan".

**Re-evaluates (does not supersede):** [ADR-014](#adr-014-gui-technology-stack--winit--wgpu--glyphon) §14-1's rejection of Vello. ADR-031's closing note "Vello adoption is unblocked, not committed" is the proximate hand-off into this ADR.

### Context

ADR-014 (2024) rejected Vello with three blockers: (i) no glyph cache (vector path rendering every frame), (ii) requires compute shaders, (iii) unstable API (3-5 month break cycles). ADR-031 (2026-04-26) migrated text from cosmic-text to Parley + swash and explicitly left the door open: *"Vello adoption is unblocked, not committed."*

Two of the three ADR-014 blockers have started to soften during 2025-2026 Q1:

1. **Glyph cache.** The `parley_draw` crate has been renamed **Glifo** and moved into the Vello repository, providing atlas-based glyph caching with `render_to_atlas` / `write_to_atlas` APIs. The "vector-path-per-frame" assumption in ADR-014 no longer holds for the canonical Linebender path.
2. **Compute shader requirement.** **Vello Hybrid** (beta as of 2026 Q1) provides a GPU/CPU mixed path that does not require pure compute shaders, expanding hardware support to GPUs that lack robust compute pipelines.
3. **API stability.** Still unresolved. Vello is at 0.8.0 (pre-1.0); Linebender has not announced a 1.0 timeline. This is the remaining ADR-014 blocker as of this writing.

Independently, the cost of *staying* with the hand-rolled wgpu stack is non-trivial: ~11.5 K LOC (Rust + WGSL), 5 RenderPipeline objects, 8 WGSL shaders, a 3-tier glyph cache (~1.5 K LOC), and **zero golden-image regression tests**. Recent activity shows 16 of 25 GPU-layer commits were bug fixes, indicating ongoing maintenance load.

The strategic question is not *"adopt or not"* in isolation but *"when, at what granularity, behind what abstraction"*. This ADR formalises that framing.

### Decision

Run a four-workstream evaluation framework that produces decision-grade information without committing to adoption:

| Workstream | Output | Adoption-conditional? |
|---|---|---|
| **W1** ADR-032 (this document) | Decision framework + §Spike Findings | No (artifact independent of outcome) |
| **W2** Golden image test infrastructure | Visual regression harness for `kasane-gui` | **No** — pays off regardless of Vello outcome |
| **W3** `GpuBackend` trait | Backend-agnostic boundary, with `BackendCapabilities` for negotiation | **No** — pure additive refactor; current `WgpuBackend` is the only impl |
| **W4** Roadmap entry | Decision triggers visible in `roadmap.md` §2.2 | No |
| **W5** `kasane-vello-spike` (5-day timebox) | Performance, parity, memory data for ADR-032 §Spike Findings | Spike crate stays out of `members` if findings are negative |

W1, W2, W4 run from day 1. W3 must precede W5. W5 has hard halt gates (see §Decision Gates).

**Vector path API surface.** During W3 implementation we discovered that `GpuPrimitive` (`kasane-gui/src/gpu/scene_graph.rs`) is *not* wired into the production rendering path — `SceneRenderer` consumes `&[DrawCommand]` (kasane-core) directly, and `GpuPrimitive` is exercised only by unit tests and the dormant `SceneBuilder::from_draw_commands` helper. Adding a `Path` variant to a non-load-bearing enum would not pin any production-relevant API. The decision is therefore to:

1. Land [`BackendCapabilities::supports_paths`](#) (boolean, currently `false` for `WgpuBackend`) as the negotiation surface for callers that may one day emit vector contributions.
2. Defer the actual `DrawCommand::DrawPath` (or equivalent) variant addition to **the adoption work** that follows a positive W5 spike. This avoids introducing dead code in `kasane-core` and avoids colliding with ADR-031 Phases 2–5, which still churn `DrawCommand`-adjacent types.

### Spike Measurement Matrix

The spike (W5) produces the following data points. Each row has a target and a halt trigger; a halt trigger fires at the day-2 checkpoint (early termination preserves remaining timebox).

| Metric | Target | Halt trigger |
|---|---|---|
| 80×24 warm frame | ≤ 70 µs | > 100 µs at Day 2 |
| Cursor-only frame | ≤ 20 µs | > 60 µs |
| Color emoji DSSIM vs swash | ≤ 0.01 | > 0.05 |
| Variable font axis change cost | ≤ 2× swash | > 5× → flag, continue |
| Resident GPU memory | ≤ 1.5× current atlas | > 3× → flag |
| Vello + Glifo clean build time | ≤ +60 s | > +180 s → flag |

The 80×24 warm-frame target intentionally matches ADR-031's Phase 11 target (≤ 70 µs) — Vello must clear the same bar Parley + swash already cleared.

### Decision Gates

| When | Gate | Action if failed |
|---|---|---|
| W2 Day 3 | Headless wgpu reads back deterministic pixels on CI | Fall back to local-only goldens (`KASANE_GOLDEN=local`); W2 continues |
| W3 Day 2 | `Path` variant doesn't force >50 changed match sites | Move `Path` to a `BackendCapabilities`-gated extension struct |
| W5 Day 2 | Frame ≤ 100 µs **and** Glifo accepts Kasane `font_id` keys | **Halt spike.** Write findings; re-evaluate in 6 months |
| W5 Day 4 | ≤ 2 matrix rows in red | Write `§Spike Findings — Stop`; exit timebox |
| W5 Day 5 | (regardless) | Finalise `§Spike Findings` — Accepted with adoption plan / Accepted as deferred / Rejected. **No production code change.** |

### Spike Findings

*To be filled in by W5. Do not commit downstream code changes (image-pipeline partial adoption, full migration) until this section is complete and ADR-032 is updated to "Accepted with adoption plan".*

### Rejected Alternatives

| Alternative | Reason for rejection |
|---|---|
| Adopt Vello now (full replacement) | API still pre-1.0 (0.8.0); Glifo not yet on crates.io; no measured frame-time data on Kasane's workload. |
| Do nothing until Vello 1.0 | Passive monitoring loses the option value of the trait abstraction and golden tests, both of which pay off independently. Also delays the spike data needed for an informed 1.0-time decision. |
| Add Lyon for vector paths, keep current text stack | Solves only the path-rendering subset; does not address the broader Linebender ecosystem alignment. Adds a third dependency without converging the long-term stack. |
| Fork Glifo into kasane | Premature. Linebender is actively iterating; a fork commits us to maintenance of an upstream-divergent atlas implementation. |
| Partial adoption (images/blur only) without trait or spike | Bypasses the W5 measurement matrix; lacks data to justify the dual-pipeline integration cost. Reconsidered post-spike if W5 findings are positive on a subset. |

### Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Vello 0.8 → 0.9 breaks mid-spike | `Cargo.lock` pinned for the spike branch; version bump deferred to a follow-up issue |
| Glifo not yet on crates.io | Git rev-pin in spike `Cargo.toml`; spike branch isolated from main `Cargo.lock` if path resolution fails |
| W3 collides with ADR-031 Phases 2-5 (`scene_graph.rs` churn) | `Path` variant is purely additive; W3 sequenced after the next ADR-031 phase tag |
| Driver-dependent rasterization breaks W2 on CI | Disable MSAA in test target; DSSIM-based comparison absorbs sub-pixel noise; per-(OS, driver) snapshot tuples permitted |
| Spike timebox exceeded | Hard halt at Day 5 regardless of completion; partial findings still feed §Spike Findings |
| User-visible regression (color emoji, BiDi, complex scripts) discovered post-adoption | Spike matrix gates emoji/variable-font parity; complex scripts (Arabic, Devanagari, CJK ligatures) are tested via golden fixtures in W2 before any adoption decision |
| Strategic divergence from Linebender (Parley adopted, Glifo skipped) | This ADR explicitly weighs convergence vs. divergence; a "Rejected" outcome on W5 is recorded as informed divergence, not avoidance |

### Implications

- **No production code changes** flow from this ADR alone. The current `WgpuBackend` (wrapping `SceneRenderer`) remains the sole production renderer.
- **Two artefacts ship regardless of outcome:** golden image regression tests (W2) and the `GpuBackend` trait abstraction (W3). Both close existing gaps in the kasane-gui codebase independent of any future Vello decision.
- **Plugin contribution surface gains a `BackendCapabilities::supports_paths` negotiation field.** No new enum variant ships in this ADR; the actual `DrawPath` primitive is deferred to adoption work, when it can be added to the live boundary (`DrawCommand` in `kasane-core`) rather than to the dormant `GpuPrimitive`. This keeps the door open without introducing dead code.
- **ADR-014 §14-1 is *not* superseded by this ADR.** Supersession occurs only if ADR-032 is updated to "Accepted with adoption plan" after a positive spike. Until then, ADR-014's GUI-stack decision (winit + wgpu, with the text portion already updated by ADR-031) remains authoritative.
- **`docs/roadmap.md` §2.2 Backlog gains a tracked item** for Vello 1.0 / Glifo crates.io publication / spike result. These are the externalised triggers for re-opening this ADR.

## Related Documents

- [semantics.md](./semantics.md) — Authoritative specification
- [index.md](./index.md) — Documentation entry point and architecture overview
- [index.md](./index.md) — Entry point for all docs
