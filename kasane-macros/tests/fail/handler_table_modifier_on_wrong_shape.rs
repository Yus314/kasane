use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{AppView, OverlayContext, OverlayContribution};

        // `transparent` is only valid on Lifecycle / Dispatcher.
        handler overlay(_app: &AppView<'_>, _ctx: &OverlayContext):
            View<Option<OverlayContribution>>(transparent);
    }
}

fn main() {}
