// γ-3.2.2e-2: `full_fallback` is a documented carve-out — the
// on_<name>_full setter has a TransformSubject signature that does not
// generalize. The macro generates the entry-struct storage so a
// hand-written setter can populate the `full_handler` companion field,
// but the modifier itself is rejected.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{
            AppView, ElementPatch, PluginState, TransformContext, TransformTarget,
        };

        handler transform(
            _target: &TransformTarget,
            _app: &AppView<'_>,
            _ctx: &TransformContext,
        ): View<ElementPatch>(prioritized, targets = Vec<TransformTarget>, full_fallback);
    }
}

fn main() {}
