# ADR-031: Text Stack Migration — cosmic-text → Parley + swash, with Protocol Style Redesign

**Status:** Accepted, Closed (2026-04-30). Parley + swash is the
production stack as of 2026-04-26. The protocol-side `Style` redesign
and plugin ABI break landed across April 28–29 (Phase A.4 split
`7fca4784`, B-wide `98592a47`, Phase 4 Tier A `a5ef9f56`, Phase 5
Tier B `8f281f52` + binaries `f4df0762`). The closure cascade
(PR-5a..PR-7) on `feat/parley-color-emoji-test` retired the public
Face↔Style bridges, bumped the WIT contract to 2.0.0 with Style-native
function names, and rebuilt all bundled / fixture WASM. All 50
workspace test suites and the full 188 `kasane-wasm` cases pass
against `kasane:plugin@2.0.0`.

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

**Closure cascade (2026-04-30, branch `feat/parley-color-emoji-test`):**
A six-PR sequence delivered the bridge retirement, observability
cleanup, WIT bump, and rename:

- `04aa9fa3` (PR-5a) — `Truth` Style-native. `default_face` /
  `padding_face` / `status_default_face` accessors → `*_style`,
  returning `&'a Style`. `AppView`'s parallel Face-bridge accessors
  deleted (Style-native versions already existed). Mapping tables in
  `state/mod.rs` and `state/tests/dirty_flags.rs` realigned to the
  underlying `ObservedState` field names.
- `093f5516` (PR-5b) — production round-trips eliminated. Added
  `Brush::linear_blend(a, b, ratio, fallback_a, fallback_b)`.
  `make_secondary_cursor_face` rewritten as Brush-native
  `make_secondary_cursor_style`; `apply_secondary_cursor_faces` now
  mutates `cell.style: TerminalStyle` directly without touching the
  `Cell::face()` bridge. `BufferRefParams` /
  `BufferLineAction::BufferLine` / `BufferLineAction::Padding` carry
  `Style` end-to-end through the TUI walker (`paint.rs`) and the GPU
  walker (`walk_scene.rs`), so per-line `Style::from_face` round-trips
  are gone. `BufferRefState` and the `salsa_inputs` `BufferInput` /
  `StatusInput` field names follow.
  `cargo bench parley/frame_warm_24_lines`: 63.3 µs (−4 % vs Phase 11
  case A baseline 64.9 µs; within criterion noise but directionally
  consistent with one fewer round-trip per line).
- `16266fd1` (PR-5c) — public Face↔Style bridges retired.
  `Cell::face()`, `Atom::face()`, `kasane-tui::sgr::emit_sgr_diff(Face)`
  shim, and the `convert_attribute(Attributes)` test helper deleted
  outright. `Style::from_face` / `Style::to_face`, the `From<Face> for
  Style` / `From<&Face> for Style` / `From<Face> for ElementStyle`
  impls, and `TerminalStyle::from_face` marked `#[doc(hidden)]` —
  invisible from the rendered API surface but still callable for the
  Kakoune wire-format conversion path that the JSON-RPC parser, the
  `Atom::from_wire` constructor, and the wire `test_support` helpers
  depend on. `Style::to_face_with_attrs` downgraded from `pub fn` to
  `pub(super)`. ~30 production callsites + ~150 test sites cascade
  via mechanical sed; the golden `ascii_80x24_smoke` snapshot
  regenerated for the `TerminalStyle`-keyed face legend.
- `571bff58` (PR-7) — WIT 2.0.0. `kasane:plugin@1.1.0 → @2.0.0` with
  six function renames (`get-default-face` → `get-default-style` and
  five siblings) plus a forced collision-resolving rename
  (`get-menu-style` returning `option<string>` → `get-menu-mode`,
  freeing the name for the actual menu-item style). `HOST_ABI_VERSION`
  bumped, all 23 `abi_version = "1.1.0"` literal sites in fixtures /
  manifests / resolver tests bumped, all 12 bundled / fixture WASM
  artefacts rebuilt, the `surface-probe` guest and the
  `define_plugin!` `theme_style_or` macro updated to the new function
  names, and the `color-preview` test expectation for the Phase 10
  exemplar (gutter + inline-box per color) corrected.
- `c87699d0` (PR-6) — `Atom::from_face` → `Atom::from_wire`. The
  wire-format intent is now in the constructor name; 17 callsites
  cascade. `Face` / `Color` / `Attributes` are already
  `#[doc(hidden)]` from PR-5c, so the visibility downgrade and the
  full `Face` → `WireFace` rename across the host crates
  (kasane-wasm convert layer, kasane-tui / kasane-gui benches and
  diagnostics) are scoped out — the `#[doc(hidden)]` markings keep
  `Face` invisible from the rendered API surface, and a future PR
  may complete the rename + downgrade once those host sites migrate
  to Style end-to-end.

Performance after closure (`cargo bench --bench parley_pipeline`,
`feat/parley-color-emoji-test`): warm 63.3 µs, one_line_changed
~83 µs. The +18 % gap vs the original 70 µs `frame_one_line_changed`
target persists and is structural to Parley's `shape_warm = 13.58 µs`
per L1 miss — closing it requires upstream Parley shape-cache work
or sub-line word/cluster caching, neither of which is on the
critical path. Per ADR-024 (perception-oriented performance policy)
the 83 µs absolute number is comfortably below the 200 µs SLO and
the 4.17 ms 240-Hz scanout, and the `Atom::face()`-on-hot-path
mutex hypothesis from §動機 (iii) is refuted — the gap is now
formally accepted.

Phase 11 perf-tune (`StyledLine` allocation reuse, `atom_styles:
Vec<Arc<Style>>`, sub-line shape cache) and the deferred `Face` →
`WireFace` rename + `pub(in crate::protocol)` visibility downgrade
are tracked as post-closure independent workstreams; see the
"Next-ADR seeds" subsection below.

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

**Supersedes (text stack only):** [ADR-014](./adr-014-gui-technology-stack-winit-wgpu-glyphon.md) §14-1's selection of glyphon (cosmic-text + swash + etagere). Window management (winit) and GPU API (wgpu) are unchanged. The atlas allocator (etagere) and the swash rasterizer are retained — only cosmic-text's layout/buffer abstraction and the glyphon-derived text pipeline are replaced.

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

This sub-section applies [ADR-024](./adr-024-perception-oriented-performance-policy.md) to the Phase 11 typing-pattern gap so the perf-tune workstream has a defined stopping condition rather than open-ended pursuit of the original 70 µs target.

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
