use std::hash::{Hash, Hasher};

use crate::plugin::{Command, LineDecoration, Plugin, PluginId, Slot};
use crate::protocol::{Color, Face};
use crate::state::{AppState, DirtyFlags};

#[derive(Default, Hash)]
struct State {
    active_line: i32,
}

#[derive(Default)]
pub struct CursorLinePlugin {
    state: State,
}

impl CursorLinePlugin {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Plugin for CursorLinePlugin {
    fn id(&self) -> PluginId {
        PluginId("cursor_line".into())
    }

    fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        if dirty.intersects(DirtyFlags::BUFFER) {
            self.state.active_line = state.cursor_pos.line;
        }
        vec![]
    }

    fn state_hash(&self) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();
        self.state.hash(&mut hasher);
        hasher.finish()
    }

    fn slot_deps(&self, _slot: Slot) -> DirtyFlags {
        DirtyFlags::empty()
    }

    fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
        if line == self.state.active_line as usize {
            Some(LineDecoration {
                left_gutter: None,
                right_gutter: None,
                background: Some(Face {
                    bg: Color::Rgb {
                        r: 40,
                        g: 40,
                        b: 50,
                    },
                    ..Face::default()
                }),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_active_line() {
        let mut plugin = CursorLinePlugin::new();
        let mut state = AppState::default();
        state.cursor_pos.line = 3;
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        let dec = plugin.contribute_line(3, &state);
        assert!(dec.is_some());
        let dec = dec.unwrap();
        assert!(dec.background.is_some());
        assert_eq!(
            dec.background.unwrap().bg,
            Color::Rgb {
                r: 40,
                g: 40,
                b: 50
            }
        );
    }

    #[test]
    fn no_highlight_on_other_lines() {
        let mut plugin = CursorLinePlugin::new();
        let mut state = AppState::default();
        state.cursor_pos.line = 3;
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        assert!(plugin.contribute_line(0, &state).is_none());
        assert!(plugin.contribute_line(2, &state).is_none());
        assert!(plugin.contribute_line(4, &state).is_none());
    }

    #[test]
    fn tracks_cursor_movement() {
        let mut plugin = CursorLinePlugin::new();
        let mut state = AppState::default();

        state.cursor_pos.line = 0;
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);
        assert!(plugin.contribute_line(0, &state).is_some());
        assert!(plugin.contribute_line(5, &state).is_none());

        state.cursor_pos.line = 5;
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);
        assert!(plugin.contribute_line(0, &state).is_none());
        assert!(plugin.contribute_line(5, &state).is_some());
    }

    #[test]
    fn state_hash_changes_on_line_change() {
        let mut plugin = CursorLinePlugin::new();
        let h1 = plugin.state_hash();

        let mut state = AppState::default();
        state.cursor_pos.line = 10;
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);
        let h2 = plugin.state_hash();

        assert_ne!(h1, h2);
    }
}
