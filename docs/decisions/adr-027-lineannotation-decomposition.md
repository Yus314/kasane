# ADR-027: LineAnnotation Decomposition

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
