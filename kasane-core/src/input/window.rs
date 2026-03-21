//! Window-mode plugin for `<C-w>` key chords.
//!
//! Handles workspace split/focus commands:
//! - `<C-w>v` — vertical split
//! - `<C-w>s` — horizontal split
//! - `<C-w>w` / `<C-w>W` — focus next / previous pane
//! - `<C-w>h/j/k/l` — focus left / down / up / right pane
//! - `<C-w>q` — close most recent split pane

use crate::input::{Key, KeyEvent, Modifiers};
use crate::layout::SplitDirection;
use crate::plugin::{AppView, Command, PluginBackend, PluginCapabilities, PluginId};
use crate::surface::SurfaceId;
use crate::workspace::{FocusDirection, Placement, WorkspaceCommand};

/// Plugin that intercepts `<C-w>` and dispatches workspace commands.
pub struct WindowModePlugin {
    /// Whether Ctrl+W was pressed and we're waiting for the next key.
    pending: bool,
    /// Active mirror surface IDs, ordered by creation time (LIFO removal).
    mirrors: Vec<SurfaceId>,
    /// Counter for generating unique mirror SurfaceIds.
    next_mirror: u32,
}

impl Default for WindowModePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowModePlugin {
    pub fn new() -> Self {
        WindowModePlugin {
            pending: false,
            mirrors: Vec::new(),
            next_mirror: 0,
        }
    }

    fn alloc_mirror_id(&mut self) -> SurfaceId {
        let id = SurfaceId(SurfaceId::PLUGIN_BASE + self.next_mirror);
        self.next_mirror += 1;
        id
    }
}

impl PluginBackend for WindowModePlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.window".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::INPUT_HANDLER
    }

    fn handle_key(&mut self, key: &KeyEvent, _state: &AppView<'_>) -> Option<Vec<Command>> {
        if self.pending {
            self.pending = false;
            return match key.key {
                Key::Char('v') if key.modifiers.is_empty() => {
                    let id = self.alloc_mirror_id();
                    self.mirrors.push(id);
                    Some(vec![Command::SpawnPaneClient {
                        surface_id: id,
                        placement: Placement::SplitFocused {
                            direction: SplitDirection::Vertical,
                            ratio: 0.5,
                        },
                    }])
                }
                Key::Char('s') if key.modifiers.is_empty() => {
                    let id = self.alloc_mirror_id();
                    self.mirrors.push(id);
                    Some(vec![Command::SpawnPaneClient {
                        surface_id: id,
                        placement: Placement::SplitFocused {
                            direction: SplitDirection::Horizontal,
                            ratio: 0.5,
                        },
                    }])
                }
                Key::Char('w') if key.modifiers.is_empty() => Some(vec![Command::Workspace(
                    WorkspaceCommand::FocusDirection(FocusDirection::Next),
                )]),
                // Shift+W: TUI sends Key::Char('W') + SHIFT, GUI sends Key::Char('W') + empty
                Key::Char('W') if !key.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) => {
                    Some(vec![Command::Workspace(WorkspaceCommand::FocusDirection(
                        FocusDirection::Prev,
                    ))])
                }
                Key::Char('h') if key.modifiers.is_empty() => Some(vec![Command::Workspace(
                    WorkspaceCommand::FocusDirection(FocusDirection::Left),
                )]),
                Key::Char('j') if key.modifiers.is_empty() => Some(vec![Command::Workspace(
                    WorkspaceCommand::FocusDirection(FocusDirection::Down),
                )]),
                Key::Char('k') if key.modifiers.is_empty() => Some(vec![Command::Workspace(
                    WorkspaceCommand::FocusDirection(FocusDirection::Up),
                )]),
                Key::Char('l') if key.modifiers.is_empty() => Some(vec![Command::Workspace(
                    WorkspaceCommand::FocusDirection(FocusDirection::Right),
                )]),
                Key::Char('q') if key.modifiers.is_empty() => {
                    if let Some(id) = self.mirrors.pop() {
                        Some(vec![Command::ClosePaneClient { surface_id: id }])
                    } else {
                        Some(vec![])
                    }
                }
                _ => {
                    // Unknown chord — don't consume, let it through
                    None
                }
            };
        }

        // Detect Ctrl+W
        if key.key == Key::Char('w') && key.modifiers == Modifiers::CTRL {
            self.pending = true;
            return Some(vec![]); // Consume Ctrl+W, wait for next key
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn ctrl_w() -> KeyEvent {
        KeyEvent {
            key: Key::Char('w'),
            modifiers: Modifiers::CTRL,
        }
    }

    fn plain(c: char) -> KeyEvent {
        KeyEvent {
            key: Key::Char(c),
            modifiers: Modifiers::empty(),
        }
    }

    #[test]
    fn ctrl_w_v_splits_vertically() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        let r1 = plugin.handle_key(&ctrl_w(), &view);
        assert!(r1.is_some_and(|v| v.is_empty())); // consumed, no commands

        let r2 = plugin.handle_key(&plain('v'), &view);
        let cmds = r2.unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Command::SpawnPaneClient { .. }));
    }

    #[test]
    fn ctrl_w_s_splits_horizontally() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('s'), &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Command::SpawnPaneClient { .. }));
    }

    #[test]
    fn ctrl_w_w_focuses_next() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('w'), &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::Workspace(WorkspaceCommand::FocusDirection(FocusDirection::Next))
        ));
    }

    #[test]
    fn ctrl_w_q_removes_split() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        // First split
        plugin.handle_key(&ctrl_w(), &view);
        plugin.handle_key(&plain('v'), &view);

        // Then close
        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('q'), &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Command::ClosePaneClient { .. }));
    }

    #[test]
    fn unknown_chord_not_consumed() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        let result = plugin.handle_key(&plain('x'), &view);
        assert!(result.is_none()); // not consumed
    }

    #[test]
    fn normal_keys_pass_through() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        assert!(plugin.handle_key(&plain('a'), &view).is_none());
        assert!(plugin.handle_key(&plain('v'), &view).is_none());
    }

    #[test]
    fn ctrl_w_v_twice_creates_two_splits() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        // First split
        plugin.handle_key(&ctrl_w(), &view);
        let cmds1 = plugin.handle_key(&plain('v'), &view).unwrap();
        assert_eq!(cmds1.len(), 1);
        assert!(matches!(cmds1[0], Command::SpawnPaneClient { .. }));

        // Second split — should also produce AddSurface, not FocusDirection
        plugin.handle_key(&ctrl_w(), &view);
        let cmds2 = plugin.handle_key(&plain('v'), &view).unwrap();
        assert_eq!(cmds2.len(), 1);
        assert!(matches!(cmds2[0], Command::SpawnPaneClient { .. }));

        assert_eq!(plugin.mirrors.len(), 2);
        assert_ne!(plugin.mirrors[0], plugin.mirrors[1]);
    }

    #[test]
    fn ctrl_w_q_removes_most_recent() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        // Create two splits
        plugin.handle_key(&ctrl_w(), &view);
        plugin.handle_key(&plain('v'), &view);
        plugin.handle_key(&ctrl_w(), &view);
        plugin.handle_key(&plain('v'), &view);
        let second_id = plugin.mirrors[1];

        // Close most recent
        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('q'), &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::ClosePaneClient { surface_id } if surface_id == second_id
        ));
        assert_eq!(plugin.mirrors.len(), 1);
    }

    #[test]
    fn ctrl_w_h_focuses_left() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('h'), &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::Workspace(WorkspaceCommand::FocusDirection(FocusDirection::Left))
        ));
    }

    #[test]
    fn ctrl_w_j_focuses_down() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('j'), &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::Workspace(WorkspaceCommand::FocusDirection(FocusDirection::Down))
        ));
    }

    #[test]
    fn ctrl_w_k_focuses_up() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('k'), &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::Workspace(WorkspaceCommand::FocusDirection(FocusDirection::Up))
        ));
    }

    #[test]
    fn ctrl_w_l_focuses_right() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('l'), &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::Workspace(WorkspaceCommand::FocusDirection(FocusDirection::Right))
        ));
    }

    #[test]
    fn ctrl_w_shift_w_focuses_prev() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        // Shift+W comes as Key::Char('W') with SHIFT modifier (TUI)
        let shift_w = KeyEvent {
            key: Key::Char('W'),
            modifiers: Modifiers::SHIFT,
        };
        let cmds = plugin.handle_key(&shift_w, &view).unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            Command::Workspace(WorkspaceCommand::FocusDirection(FocusDirection::Prev))
        ));
    }

    #[test]
    fn ctrl_w_q_on_empty_does_nothing() {
        let mut plugin = WindowModePlugin::new();
        let state = AppState::default();
        let view = AppView::new(&state);

        plugin.handle_key(&ctrl_w(), &view);
        let cmds = plugin.handle_key(&plain('q'), &view).unwrap();
        assert!(cmds.is_empty());
    }
}
