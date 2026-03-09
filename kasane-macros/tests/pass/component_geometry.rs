use kasane_core::kasane_component;
use kasane_core::state::AppState;

/// Reads state.cols and state.rows — these are free reads (no DirtyFlag needed).
#[kasane_component(deps(BUFFER))]
fn my_buffer_view(state: &AppState) -> usize {
    let w = state.cols as usize;
    let h = state.rows as usize;
    state.lines.len().min(w * h)
}

fn main() {
    let state = AppState::default();
    let _ = my_buffer_view(&state);
}
