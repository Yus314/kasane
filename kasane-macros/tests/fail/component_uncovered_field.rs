use kasane_core::kasane_component;
use kasane_core::state::AppState;

/// Declares deps(STATUS) but reads state.cursor_count which maps to BUFFER.
#[kasane_component(deps(STATUS))]
fn bad_component(state: &AppState) -> String {
    format!("cursors: {}", state.cursor_count)
}

fn main() {}
