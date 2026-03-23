kasane_plugin_sdk::generate!();

use std::cell::RefCell;

use kasane_plugin_sdk::{modifiers, plugin};

#[derive(Default)]
struct PluginState {
    /// Whether Ctrl+W was pressed and we're waiting for the next key.
    pending: bool,
    /// Active pane keys, ordered by creation time (LIFO removal).
    panes: Vec<String>,
    /// Counter for generating unique pane keys.
    next_pane: u32,
    /// Generation counter for state hashing.
    generation: u64,
}

impl PluginState {
    fn alloc_pane_key(&mut self) -> String {
        let key = format!("pane-{}", self.next_pane);
        self.next_pane += 1;
        key
    }

    fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

thread_local! {
    static STATE: RefCell<PluginState> = RefCell::new(PluginState::default());
}

struct PaneManagerPlugin;

#[plugin]
impl Guest for PaneManagerPlugin {
    fn get_id() -> String {
        "pane_manager".to_string()
    }

    fn requested_authorities() -> Vec<PluginAuthority> {
        vec![PluginAuthority::WorkspaceManagement]
    }

    fn state_hash() -> u64 {
        STATE.with(|s| s.borrow().generation)
    }

    fn on_workspace_changed(snapshot: WorkspaceSnapshot) {
        STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.panes.retain(|pane_key| {
                snapshot
                    .rects
                    .iter()
                    .any(|r| r.key.as_deref() == Some(pane_key))
            });
        });
    }

    fn handle_key(event: KeyEvent) -> Option<Vec<Command>> {
        STATE.with(|s| {
            let mut state = s.borrow_mut();

            if state.pending {
                state.pending = false;
                state.bump_generation();
                return match &event.key {
                    KeyCode::Character(c) if c == "v" && event.modifiers == 0 => {
                        let pane_key = state.alloc_pane_key();
                        state.panes.push(pane_key.clone());
                        Some(vec![Command::SpawnPaneClient(SpawnPaneClientConfig {
                            pane_key,
                            placement: SurfacePlacement::SplitFocused(SplitFocusedPlacement {
                                direction: SplitDirection::Vertical,
                                ratio: 0.5,
                            }),
                        })])
                    }
                    KeyCode::Character(c) if c == "s" && event.modifiers == 0 => {
                        let pane_key = state.alloc_pane_key();
                        state.panes.push(pane_key.clone());
                        Some(vec![Command::SpawnPaneClient(SpawnPaneClientConfig {
                            pane_key,
                            placement: SurfacePlacement::SplitFocused(SplitFocusedPlacement {
                                direction: SplitDirection::Horizontal,
                                ratio: 0.5,
                            }),
                        })])
                    }
                    KeyCode::Character(c) if c == "w" && event.modifiers == 0 => Some(vec![
                        Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::NextDir)),
                    ]),
                    // Shift+W: TUI sends 'W' + SHIFT, GUI sends 'W' + empty
                    KeyCode::Character(c)
                        if c == "W"
                            && event.modifiers & (modifiers::CTRL | modifiers::ALT) == 0 =>
                    {
                        Some(vec![Command::WorkspaceCommand(
                            WorkspaceCmd::FocusDirection(FocusDir::PrevDir),
                        )])
                    }
                    KeyCode::Character(c) if c == "h" && event.modifiers == 0 => Some(vec![
                        Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::LeftDir)),
                    ]),
                    KeyCode::Character(c) if c == "j" && event.modifiers == 0 => Some(vec![
                        Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::DownDir)),
                    ]),
                    KeyCode::Character(c) if c == "k" && event.modifiers == 0 => Some(vec![
                        Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::UpDir)),
                    ]),
                    KeyCode::Character(c) if c == "l" && event.modifiers == 0 => Some(vec![
                        Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(
                            FocusDir::RightDir,
                        )),
                    ]),
                    KeyCode::Character(c) if c == "q" && event.modifiers == 0 => {
                        if let Some(pane_key) = state.panes.pop() {
                            Some(vec![Command::ClosePaneClient(pane_key)])
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
            if matches!(&event.key, KeyCode::Character(c) if c == "w")
                && event.modifiers == modifiers::CTRL
            {
                state.pending = true;
                state.bump_generation();
                return Some(vec![]); // Consume Ctrl+W, wait for next key
            }

            None
        })
    }
}

export!(PaneManagerPlugin);
