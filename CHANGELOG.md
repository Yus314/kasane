# Changelog

## [Unreleased]

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
