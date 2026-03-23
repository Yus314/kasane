# Changelog

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
