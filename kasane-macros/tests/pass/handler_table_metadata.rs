// γ-3.2.2e-2: View `prioritized` standalone + `targets=T` storage.
//
// Confirms:
// - When `prioritized` (and optionally `targets=T`) appears on a View
//   entry without `per_slot`, the macro emits a 3rd storage shape:
//   `Option<<Name>Entry>` with `priority: i16` and (optionally)
//   `targets: T` metadata fields.
// - The setter signature gains positional parameters for the metadata,
//   ordered `priority → targets → handler`.
// - The handler is still type-erased and stored on the entry struct.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{
            AppView, ElementPatch, PluginState, TransformContext, TransformTarget,
        };

        // The `transform` pattern: prioritized + targets, no per_slot.
        // Storage becomes `Option<TransformEntry>` with priority + targets +
        // handler fields. The full_fallback companion is a documented
        // carve-out (see the corresponding fail test).
        handler transform(
            _target: &TransformTarget,
            _app: &AppView<'_>,
            _ctx: &TransformContext,
        ): View<ElementPatch>(prioritized, targets = Vec<TransformTarget>);
    }
}

fn main() {
    use kasane_core::plugin::{ElementPatch, TransformTarget};

    // Empty table: handler is None.
    let table = spec::HandlerTable::empty();
    assert!(table.transform_handler.is_none());

    // Register: setter takes positional priority + targets + handler.
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    registry.on_transform(
        50,
        vec![TransformTarget::STATUS_BAR],
        |_state, _target, _app, _ctx| ElementPatch::Identity,
    );
    let table = registry.into_table();
    let entry = table.transform_handler.as_ref().unwrap();
    assert_eq!(entry.priority, 50);
    assert_eq!(entry.targets, vec![TransformTarget::STATUS_BAR]);
}
