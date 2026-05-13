// γ-3.2.2e-1: View `default = expr` + `stateless` modifiers.
//
// Confirms:
// - `default = expr` emits a `pub(crate) fn default_<name>() -> <Out>`
//   helper that returns the spec-declared fallback value (used by the
//   bridge when no handler is registered).
// - `stateless` drops the implicit `&S` arg from both the erased alias
//   and the setter signature; the wrapper does not downcast. This
//   matches the lenses-factory pattern (`Fn() -> Vec<Arc<dyn Lens>>`).

use kasane_macros::handler_table;
use std::sync::Arc;

handler_table! {
    pub mod spec {
        use kasane_core::lens::Lens;
        use kasane_core::plugin::{AnnotateContext, AppView, PluginState, VirtualTextItem};
        use std::sync::Arc;

        // default = expr: virtual_text returns Vec<VirtualTextItem>; the
        // bridge falls back to vec![] when no handler is registered.
        handler virtual_text(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<Vec<VirtualTextItem>>(default = vec![]);

        // stateless: lenses factory has no &S arg.
        handler lenses(): View<Vec<Arc<dyn Lens>>>(stateless, default = vec![]);
    }
}

fn main() {
    use kasane_core::plugin::VirtualTextItem;

    // default helper returns the spec-declared fallback.
    let fallback: Vec<VirtualTextItem> = spec::default_virtual_text();
    assert!(fallback.is_empty());
    let fallback_lenses: Vec<Arc<dyn kasane_core::lens::Lens>> = spec::default_lenses();
    assert!(fallback_lenses.is_empty());

    // The stateless setter accepts a closure with no &S arg.
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    registry.on_lenses(|| Vec::new());
    let table = registry.into_table();
    assert!(table.lenses_handler.is_some());

    // The non-stateless setter still requires &S.
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    registry.on_virtual_text(|_state, _line, _app, _ctx| Vec::new());
    let table = registry.into_table();
    assert!(table.virtual_text_handler.is_some());
}
