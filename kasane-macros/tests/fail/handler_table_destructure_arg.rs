use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{AppView, Effects};

        // Destructuring patterns are rejected — the wrapper closure needs
        // a simple identifier to forward.
        handler init((_a, _b): (usize, usize)): Lifecycle<Effects>;
    }
}

fn main() {}
