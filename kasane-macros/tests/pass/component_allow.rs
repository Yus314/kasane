use kasane_core::kasane_component;
use kasane_core::state::AppState;

/// Reads state.infos (INFO) and state.cursor_pos (BUFFER) but intentionally
/// does not rebuild on BUFFER changes — cursor_pos is allowed.
#[kasane_component(deps(INFO), allow(cursor_pos))]
fn my_info_section(state: &AppState) -> usize {
    let _pos = &state.cursor_pos;
    state.infos.len()
}

fn main() {
    let state = AppState::default();
    let _ = my_info_section(&state);
}
