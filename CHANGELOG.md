# Changelog

## [Unreleased]

### Added — ADR-031 Parley text stack migration (in progress)

This is a multi-phase migration; the changes below ship the foundation. The cosmic-text path remains the production renderer until Phase 11 cuts it.

- **core**: `protocol::Style` Parley-native text style alongside the legacy `Face`. Continuous `FontWeight` (100..=900), `FontSlant`, `FontFeatures` bitset, `FontVariation` axes, `BidiOverride`, and `TextDecoration` with five `DecorationStyle` variants (Solid/Curly/Dotted/Dashed/Double). `Atom::style()` projects the wire-format `Face` into a `Style` for the new path; `Style::from_face` / `to_face` support round-tripping during migration.
- **gui**: `kasane-gui::gpu::parley_text` module — facade (`ParleyText` owning `FontContext` + `LayoutContext` + swash `ScaleContext`), shaper (`shape_line` driving `parley::RangedBuilder`), L1 `LayoutCache` (`Arc<ParleyLayout>` keyed by content + style + font_size + max_width with `O(1)` `invalidate_all`), swash glyph rasteriser (4-level subpixel x quantisation, color emoji via `Source::ColorOutline` → `ColorBitmap` → `Outline` → `Bitmap` priority), L2 `GlyphRasterCache` + L3 `AtlasShelf` with bidirectional eviction link.
- **gui**: `SceneRenderer` carries the new state alongside the cosmic-text fields. `KASANE_TEXT_BACKEND=parley` opts into Parley `CellMetrics` at startup; full rendering swap arrives in Phase 9b.
- **gui**: `parley_text::hit_test` provides `(x, y) → byte_offset` and `byte → x_advance` helpers built on `parley::Cluster::from_point` / `from_byte_index`. Bidi-aware (`HitResult::is_rtl`).
- **bench**: `cargo bench --bench parley_pipeline` measures the new pipeline. Steady-state cursor-only frame ~62 µs at 24 lines (within the ≤ 70 µs Phase 11 target); typing-pattern frame ~81 µs (Phase 11 micro-optimisation candidate).
- **deps**: `parley = 0.9`, `swash = 0.2` added at the workspace level. `kasane-gui` carries both alongside `cosmic-text = 0.18` until Phase 11.

See [ADR-031](docs/decisions.md#adr-031-text-stack-migration--cosmic-text--parley--swash-with-protocol-style-redesign) for the full decision record and phase plan; [docs/roadmap.md §2.1](docs/roadmap.md#21-now) for current phase status.

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
