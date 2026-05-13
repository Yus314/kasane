// γ-3.2.2d-2: View `unified` + `suppresses=[…]` modifiers.
//
// Confirms:
// - `unified` modifier emits a `pub(crate) fn has_<name>(&self) -> bool`
//   on HandlerTable that returns true iff the unified handler is
//   registered. The bridge consults this to decide between the unified
//   monolithic dispatch path and the decomposed handlers.
// - `suppresses=[a, b, c]` emits `pub(crate) const SUPPRESSED_BY_<NAME>:
//   &[&str]` listing the decomposed handler names the unified entry
//   supersedes when registered.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{
            AnnotateContext, AppView, BackgroundLayer, LineAnnotation, PluginState,
        };
        use kasane_core::element::Element;
        use kasane_core::render::InlineDecoration;

        // Decomposed handlers — these are what `annotate_line` suppresses.
        handler gutter(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<Option<Element>>;
        handler background(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<Option<BackgroundLayer>>;
        handler inline(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<Option<InlineDecoration>>;

        // Unified monolithic handler that supersedes the three above.
        handler annotate_line(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<Option<LineAnnotation>>(unified, suppresses = [gutter, background, inline]);
    }
}

fn main() {
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    let table = spec::HandlerTable::empty();

    // No unified handler registered yet — predicate is false.
    assert!(!table.has_annotate_line());

    // Register the unified handler.
    registry.on_annotate_line(|_state, _line, _app, _ctx| None);
    let table = registry.into_table();

    // Predicate is true once the unified handler is registered.
    assert!(table.has_annotate_line());

    // The suppresses const lists the decomposed handler names.
    assert_eq!(
        spec::SUPPRESSED_BY_ANNOTATE_LINE,
        &["gutter", "background", "inline"]
    );
}
