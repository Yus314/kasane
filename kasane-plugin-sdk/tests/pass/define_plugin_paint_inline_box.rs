// Verifies the `paint_inline_box(box_id) { body }` section parses and
// produces a paint method that returns Option<ElementHandle>. ADR-031
// Phase 10 Step 2 SDK ergonomics gap closure.

kasane_plugin_sdk::define_plugin! {
    id: "paint_inline_box_test",

    state {
        last_box_id: u64 = 0,
    },

    paint_inline_box(box_id) {
        state.last_box_id = box_id;
        // Returning None is valid; real plugins build an ElementHandle
        // from a Container/Text element. The fixture only verifies that
        // the macro section accepts the syntax and the generated method
        // signature matches the host expectation.
        None
    },
}

fn main() {}
