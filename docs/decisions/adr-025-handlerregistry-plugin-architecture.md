# ADR-025: HandlerRegistry Plugin Architecture

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
