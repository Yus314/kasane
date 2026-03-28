fn alloc_pane_key(next_pane: &mut u32) -> String {
    let key = format!("pane-{}", next_pane);
    *next_pane += 1;
    key
}

kasane_plugin_sdk::define_plugin! {
    id: "pane_manager",

    state {
        pending: bool = false,
        panes: Vec<String> = Vec::new(),
        next_pane: u32 = 0,
    },

    authorities: [PluginAuthority::WorkspaceManagement],

    on_workspace_changed(snapshot) {
        state.panes.retain(|pane_key| {
            snapshot
                .rects
                .iter()
                .any(|r| r.key.as_deref() == Some(pane_key))
        });
    },

    handle_key(event) {
        if state.pending {
            state.pending = false;
            return match &event.key {
                KeyCode::Character(c) if c == "v" && event.modifiers == 0 => {
                    let pane_key = alloc_pane_key(&mut state.next_pane);
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
                    let pane_key = alloc_pane_key(&mut state.next_pane);
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
                KeyCode::Character(c)
                    if c == "+"
                        && event.modifiers & (modifiers::CTRL | modifiers::ALT) == 0 =>
                {
                    Some(vec![Command::WorkspaceCommand(WorkspaceCmd::Resize(
                        0.05,
                    ))])
                }
                KeyCode::Character(c) if c == "-" && event.modifiers == 0 => {
                    Some(vec![Command::WorkspaceCommand(WorkspaceCmd::Resize(
                        -0.05,
                    ))])
                }
                KeyCode::Character(c)
                    if c == ">"
                        && event.modifiers & (modifiers::CTRL | modifiers::ALT) == 0 =>
                {
                    Some(vec![Command::WorkspaceCommand(
                        WorkspaceCmd::ResizeDirection(ResizeDirectionConfig {
                            direction: SplitDirection::Vertical,
                            delta: 0.05,
                        }),
                    )])
                }
                KeyCode::Character(c)
                    if c == "<"
                        && event.modifiers & (modifiers::CTRL | modifiers::ALT) == 0 =>
                {
                    Some(vec![Command::WorkspaceCommand(
                        WorkspaceCmd::ResizeDirection(ResizeDirectionConfig {
                            direction: SplitDirection::Vertical,
                            delta: -0.05,
                        }),
                    )])
                }
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
        if is_ctrl(&event, "w") {
            state.pending = true;
            return Some(vec![]); // Consume Ctrl+W, wait for next key
        }

        None
    },
}
