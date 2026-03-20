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
| GUI toolkit | Current | **winit + wgpu + glyphon** | Details in [ADR-014](#adr-014-gui-technology-stack--winit--wgpu--glyphon) |
| Configuration format | Current | **TOML + ui_options combined** | Static config + Kakoune integration |
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
| Compiler-driven optimization | Current | **deps verification + ViewCache / SceneCache / PaintPatch** | Automatic patch generation for generic plugins not yet implemented |
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

**Status:** Decided

**Context:**
Three formats plus a combination were evaluated for configuration: TOML, KDL, Kakoune commands only (ui_options only), and TOML + ui_options combined.

**Decision:** Adopt TOML + ui_options combined.

**Rationale:**
- **TOML (static config):** `~/.config/kasane/config.toml` — theme, font, GUI settings, default behavior. Type-safe deserialization via `serde`
- **ui_options (dynamic config):** Kakoune `set-option global ui_options kasane_*=*` — UI behavior that can be changed at runtime. Can be combined with Kakoune hooks and conditionals
- Achieves both type-safe static configuration and dynamic configuration integrated with Kakoune

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

**Status:** Decided

**Context:**
The question was how Kasane should launch and manage Kakoune.

**Decision:** By default, spawn `kak -ui json` as a child process, with the `-c` option also supporting connection to an existing daemon session.

**Startup patterns:**
- `kasane file.txt` → internally spawns `kak -ui json file.txt`
- `kasane -- -e 'edit file.txt' -s mysession` → arguments passed through to Kakoune
- `kasane -c mysession` → connects to existing daemon session via `kak -ui json -c mysession`

**Rationale:**
- Kakoune's daemon mode (`kak -d -s` / `kak -c`) is an important multi-client workflow
- Not supporting `-c` would be a major limitation for Kakoune users
- JSON UI connection uses a `kak -ui json` process for both new and existing sessions, so the pipe mechanism is identical

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
- The native path continues for registration via `kasane::run()`, full access to `&AppState`, and escape hatches such as `Surface` / `PaintHook`
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

**2026-03 note:** The "two-layer rendering" in this section is the name for the overall vision. What is currently established is deps verification, `ViewCache`, `SceneCache`, and `PaintPatch`. Automatic patch generation from generic plugin views remains a Stage 5 research topic.

### Implementation Record

All 4 stages completed: (1) DirtyFlags-based view memoization, (2) verified dependency tracking via `#[kasane::component(deps(...))]`, (3) SceneCache for DrawCommand-level caching, (4) compiled PaintPatch with StatusBarPatch / MenuSelectionPatch / CursorPatch.

### Implementation Status

#### Stage 1: DirtyFlags-Based View Memoization — Implemented

| Metric | Value |
|---|---|
| view() cost | 5.0 us (0 plugins) / 10.4 us (10 plugins) |
| Implementation | ViewCache, ComponentCache\<T\>, DirtyFlags u16, MENU→MENU_STRUCTURE+MENU_SELECTION split |
| Result | view() sections skipped entirely when corresponding DirtyFlags are clear |

#### Stage 2: Verified Dependency Tracking — Implemented

| Metric | Value |
|---|---|
| Implementation | `#[kasane::component(deps(FLAG, ...))]` proc macro, AST-based field access analysis, FIELD_FLAG_MAP |
| Compile-time check | Accesses to state fields not covered by declared deps cause compile error |
| Escape hatch | `allow(field, ...)` for intentional dependency gaps |

#### Stage 3: SceneCache (DrawCommand-Level Caching) — Implemented

| Metric | Value |
|---|---|
| Implementation | Per-section DrawCommand caching (base, menu, info) |
| Invalidation | Mirrors ViewCache: BUFFER\|STATUS\|OPTIONS→base, MENU→menu, INFO→info |
| GPU benefit | Cursor-only frames reuse cached scene (0 us pipeline work) |
| Cold/Warm ratio | 22.8 μs cold → 7.0 μs warm (3.3x speedup) |

#### Stage 4: Compiled Paint Patches — Implemented

| Metric | Value |
|---|---|
| StatusBarPatch | STATUS-only dirty → repaint ~80 cells: **6.17 μs** (vs 57 μs full) |
| MenuSelectionPatch | MENU_SELECTION-only dirty → swap face on ~10 cells: **6.80 μs** |
| CursorPatch | Cursor moved, no dirty flags → swap face on 2 cells: **1.01 μs** |
| LayoutCache | base_layout, status_row, root_area cached with per-section invalidation |

#### Overall Result

All four stages are operational. The pipeline automatically selects the minimal repaint path:

1. **PaintPatch** (2-80 cells) → **sectioned repaint** (~1 section) → **full pipeline** (fallback)

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
| C: Incremental diffing (React-style) | Redraw only changed parts via Element tree diffing | Already covered by ViewCache + section splitting. Additional diff layer not worth the complexity |
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
- **L1: Plugin state cache (implemented)** — `PluginSlotCache` in `PluginRegistry` caches `contribute_to()` results per slot, invalidating only when `state_hash()` changes
- **L3: Explicit DirtyFlags dependencies (partially implemented)** — `contribute_deps()` / `transform_deps()` / `annotate_deps()` allow plugins to declare dependencies. Automatic derivation is not yet implemented
- **L2: Slot position cache (not implemented)** — Extend `LayoutCache` with per-slot Rect cache, so only that slot is partially repainted when plugin state changes
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

### 11-5: Default UI Mode — Configurable via config.toml

**Decision:** Make the default UI mode (TUI/GUI) configurable via `[ui] default` in `config.toml`. The `--ui` flag serves as a one-shot override.

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
- Making the default UI configurable via config.toml removes the motivation to include `--ui` in aliases, so this error practically never occurs
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

WASM plugins use a subset of the Plugin trait API via WIT interface. `contribute_to`, `transform`, `annotate_line_with_ctx`, `contribute_overlay_with_ctx`, `transform_menu_item`, and `cursor_style_override` are available in WASM (WIT v0.4.0+). `Surface`, `PaintHook`, and `Pane` APIs are available only in native plugins.

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
| `transform(TransformTarget::Buffer)` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `transform(TransformTarget::StatusBar)` | prompt-highlight (status bar wrap in prompt mode) | Proven |
| `cursor_style_override()` | test plugin in `kasane-core/tests/plugin_integration.rs` | Proven |
| `contribute_to(SlotId::Named(...))` | `surface_probe` hosted surface E2E in `kasane-wasm/src/tests.rs` | Proven |
| `OverlayAnchor::Absolute` | `fuzzy_finder` overlay test in `kasane-wasm/src/tests.rs` | Proven |

## ADR-013: WASM Plugin Runtime — Component Model Adoption

**Status:** Decided

**Context:**
While evaluating runtime loading approaches for external plugins in Phase 5b, it was necessary to quantitatively assess the performance feasibility of WASM sandboxing. The current compile-time binding approach (`kasane::run()` + `#[kasane::plugin]`) is type-safe but requires rebuilding to add plugins. WASM would enable install-and-activate without rebuilds, expanding the plugin ecosystem.

**Benchmark environment:** `kasane-wasm-bench` crate (wasmtime 42, criterion)

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
2. `render_pipeline_direct()` — subtree memoization via ViewCache
3. `render_pipeline_sectioned()` — selective redraw per section
4. `render_pipeline_patched()` — direct cell writes via compiled patches
5. Surface variants (`render_pipeline_surfaces_cached/sectioned/patched`)

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
DirtyFlags → ViewCache invalidation → per-section rebuild decision
          → SceneCache invalidation → DrawCommand regeneration decision
          → LayoutCache invalidation → layout recalculation decision
```

If each cache's invalidation mask is correct, all variants are equivalent to the reference implementation.

## ADR-017: SurfaceId-Based Invalidation (Design)

**Status:** Proposed (implementation to be evaluated when Phase 5 begins)

### Background

The current `DirtyFlags` are global: Draw messages from Kakoune invalidate all ViewCache/SceneCache/LayoutCache. In Phase 5 (multi-pane), pane A's Draw would unnecessarily invalidate pane B's cache.

### Proposed Design

1. **`SurfaceDirtyMap`**: Replace global `DirtyFlags` with `HashMap<SurfaceId, DirtyFlags>`
2. **Per-surface ViewCache**: `HashMap<SurfaceId, ViewCache>` for per-surface caching
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

- `PaintHook::surface_filter()` (existing) — per-surface hook filter. Consistent with the design
- `EffectiveSectionDeps` — extendable to per-surface deps
- `PluginSlotCache` — independent cache entries per surface

### Migration Path

1. Introduce `SurfaceDirtyMap` internally while maintaining global `DirtyFlags` as a fallback
2. In `apply()`, set flags only for the target surface for Draw; broadcast to all surfaces for others
3. Gradually migrate ViewCache to per-surface
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

Kasane's rendering pipeline uses a multi-layer caching system (ViewCache, LayoutCache, SceneCache, PaintPatch) driven by manual `DirtyFlags` bitmask tracking. While effective — achieving ~49μs CPU per frame at 80×24 — the system has accumulated complexity:

1. **Manual invalidation bookkeeping**: Each view function must declare its `DirtyFlags` dependencies (BUILD_BASE_DEPS, BUILD_MENU_SECTION_DEPS, etc.), verified at compile time by the `#[kasane::component(deps(...))]` macro. Adding new state fields requires updating both `DirtyFlags` and all dependency declarations.

2. **Cache coherence by convention**: `ViewCache`, `SceneCache`, and `LayoutCache` each duplicate the invalidation logic (which flags invalidate which cache section), with correctness relying on manual alignment rather than structural guarantees.

3. **Plugin interaction complexity**: `PluginSlotCache` uses its own two-level cache (L1: state_hash, L3: slot_deps) independent of the view caching system, requiring separate `prepare_plugin_cache()` calls before rendering.

The Salsa incremental computation framework (v0.26.0) offers automatic dependency tracking and memoization, potentially replacing the manual invalidation bookkeeping while preserving the pipeline's performance characteristics.

### Decision

Adopt a **Stage 1 / Stage 2 split** architecture where:

- **Stage 1 (Salsa tracked)**: Pure Element generation from protocol state. Salsa automatically tracks dependencies and memoizes results. No plugin interaction.
- **Stage 2 (imperative)**: Plugin contributions, transforms, and annotations applied outside Salsa. Uses existing `PluginRegistry` with its `RefCell` interior mutability unchanged.

Salsa is a mandatory dependency. The legacy Surface-based pipeline (`pipeline_surface.rs`, `SurfaceViewSource`) has been removed; all rendering uses the Salsa path exclusively.

### Architecture

Stage 1 uses 6 Salsa input structs (grouped by protocol message boundary) + `PluginEpochInput` (monotonic counter bridging plugin state changes into Salsa's dependency graph). Four tracked view functions produce Element trees from these inputs. Stage 2 composes plugin contributions outside Salsa. Four pipeline variants mirror the legacy paths (cached/sectioned/patched/scene-cached). The legacy Surface-based pipeline has been removed; `SalsaViewSource` is the sole implementation.

For implementation details (input structs, tracked functions, pipeline variants, file mapping), see the source code in `kasane-core/src/state/salsa_*.rs` and `kasane-core/src/render/pipeline_salsa.rs`.

### Trade-offs

1. **Additive, not replacive**: The Salsa layer adds ~11-13μs of cache-hit overhead (5-6 tracked functions × ~2.2μs each), which is negligible relative to the 4167μs frame budget at 240fps. However, it does not delete the existing caching infrastructure — `ViewCache`, `LayoutCache`, and `SceneCache` remain.

2. **Plugin boundary remains imperative**: Plugins with `RefCell` interior mutability cannot participate in Salsa's dependency graph. The epoch-based bridge is a pragmatic compromise: it detects when plugin outputs *might* have changed but cannot provide fine-grained invalidation per-slot or per-plugin.

3. **Dual maintenance during feature flag period**: Both `render_pipeline_surfaces_*` and `render_pipeline_salsa_*` paths must be maintained. The `salsa_pipeline_comparison.rs` test suite (15 tests) verifies byte-identical output between the two paths.

4. **`no_eq` on all view functions**: Since `Element` lacks `PartialEq`, Salsa cannot perform output equality checks to suppress downstream re-evaluation. This means a cache miss on any input *will* propagate to all dependents, even if the output happens to be identical. This is acceptable because the tracked functions are leaf-level (no further tracked functions depend on their Element output).

### Testing

`kasane-core/tests/salsa_pipeline_comparison.rs` — 15 tests verifying cell-by-cell grid equivalence between legacy and Salsa pipelines across scenarios including:

- Base states (empty, buffer content, status bar, menu variants, info popups)
- Plugin contributions (slot, transform, annotation, gutter)
- Combined plugin scenarios

### Future Considerations

- If `Element` gains `PartialEq`, remove `no_eq` annotations for better downstream invalidation suppression
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
| Pure functions enable future Salsa memoization of Stage 2 | `PurePlugin` cannot use `Surface`, `PaintHook`, or pane lifecycle |
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

## Related Documents

- [semantics.md](./semantics.md) — Authoritative specification
- [architecture.md](./architecture.md) — System boundaries and responsibilities
- [index.md](./index.md) — Entry point for all docs
