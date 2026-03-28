kasane_plugin_sdk::define_plugin! {
    id: "impl_block_test",

    state {
        query: String = String::new(),
        count: u32 = 0,
    },

    impl {
        fn reset(&mut self) {
            self.query.clear();
            self.count = 0;
        }

        fn is_active(&self) -> bool {
            !self.query.is_empty()
        }

        fn increment(&mut self) {
            self.count += 1;
        }
    },

    handle_key(event) {
        let _ = event;
        state.reset();
        state.increment();
        if state.is_active() {
            Some(vec![])
        } else {
            None
        }
    },
}

fn main() {}
