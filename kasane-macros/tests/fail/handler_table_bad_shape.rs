use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{AppView, Effects};

        handler bad(_app: &AppView<'_>): Magic<Effects>;
    }
}

fn main() {}
