use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{AppView, Effects};

        // `state` is reserved as the implicit first arg slot.
        handler init(state: &AppView<'_>): Lifecycle<Effects>;
    }
}

fn main() {}
