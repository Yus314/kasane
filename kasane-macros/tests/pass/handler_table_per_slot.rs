// γ-3.2.2c: View per_slot=K + prioritized storage codegen.
//
// Confirms:
// - `per_slot=K` swaps `Option<ErasedHandler>` for `Vec<<Name>Entry>`
//   storage and emits a slot-keyed setter that pushes one entry per call.
// - `per_slot + prioritized` adds an `i16 priority` field on the entry
//   struct and threads it through the setter signature.
// - The auto-generated `<Name>Entry` struct exposes the `key` and
//   (optional) `priority` fields plus the erased handler.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::element::Element;
        use kasane_core::plugin::{
            AnnotateContext, AppView, ContributeContext, Contribution, GutterSide, PluginState,
            SlotId,
        };

        // PerSlot only.
        handler contribute(_app: &AppView<'_>, _ctx: &ContributeContext):
            View<Option<Contribution>>(per_slot = SlotId);

        // PerSlot + Prioritized.
        handler gutter(_line: usize, _app: &AppView<'_>, _ctx: &AnnotateContext):
            View<Option<Element>>(per_slot = GutterSide, prioritized);
    }
}

fn main() {
    use kasane_core::plugin::{GutterSide, SlotId};

    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();

    // PerSlot setter takes the key parameter.
    registry.on_contribute(SlotId::STATUS_LEFT, |_state, _app, _ctx| None);
    registry.on_contribute(SlotId::STATUS_RIGHT, |_state, _app, _ctx| None);

    // PerSlot + Prioritized setter takes both key and priority.
    registry.on_gutter(GutterSide::Left, 10, |_state, _line, _app, _ctx| None);
    registry.on_gutter(GutterSide::Right, 5, |_state, _line, _app, _ctx| None);

    let table = registry.into_table();
    assert_eq!(table.contribute_handlers.len(), 2);
    assert_eq!(table.gutter_handlers.len(), 2);

    // Entry struct exposes the key and (for prioritized) priority.
    assert_eq!(table.contribute_handlers[0].key, SlotId::STATUS_LEFT);
    assert_eq!(table.contribute_handlers[1].key, SlotId::STATUS_RIGHT);
    assert_eq!(table.gutter_handlers[0].key, GutterSide::Left);
    assert_eq!(table.gutter_handlers[0].priority, 10);
    assert_eq!(table.gutter_handlers[1].key, GutterSide::Right);
    assert_eq!(table.gutter_handlers[1].priority, 5);
}
