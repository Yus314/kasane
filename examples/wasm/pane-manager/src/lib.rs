fn alloc_pane_key(next_pane: &mut u32) -> String {
    let key = format!("pane-{}", next_pane);
    *next_pane += 1;
    key
}

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        panes: Vec<String> = Vec::new(),
        next_pane: u32 = 0,
    },

    on_workspace_changed(snapshot) {
        state.panes.retain(|pane_key| {
            snapshot
                .rects
                .iter()
                .any(|r| r.key.as_deref() == Some(pane_key))
        });
    },

    key_map {
        chord(ctrl('w')) {
            char('v') => "split_v",
            char('s') => "split_h",
            char('w') => "focus_next",
            char('W') => "focus_prev",
            char('h') => "focus_left",
            char('j') => "focus_down",
            char('k') => "focus_up",
            char('l') => "focus_right",
            char('+') => "grow",
            char('-') => "shrink",
            char('>') => "grow_v",
            char('<') => "shrink_v",
            char('q') => "close_pane",
        },
    },

    actions {
        "split_v" => |_event| {
            let pane_key = alloc_pane_key(&mut state.next_pane);
            state.panes.push(pane_key.clone());
            KeyResponse::ConsumeWith(vec![Command::SpawnPaneClient(SpawnPaneClientConfig {
                pane_key,
                placement: SurfacePlacement::SplitFocused(SplitFocusedPlacement {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                }),
            })])
        },
        "split_h" => |_event| {
            let pane_key = alloc_pane_key(&mut state.next_pane);
            state.panes.push(pane_key.clone());
            KeyResponse::ConsumeWith(vec![Command::SpawnPaneClient(SpawnPaneClientConfig {
                pane_key,
                placement: SurfacePlacement::SplitFocused(SplitFocusedPlacement {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                }),
            })])
        },
        "focus_next" => |_event| {
            KeyResponse::ConsumeWith(vec![
                Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::NextDir)),
            ])
        },
        "focus_prev" => |_event| {
            KeyResponse::ConsumeWith(vec![
                Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::PrevDir)),
            ])
        },
        "focus_left" => |_event| {
            KeyResponse::ConsumeWith(vec![
                Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::LeftDir)),
            ])
        },
        "focus_down" => |_event| {
            KeyResponse::ConsumeWith(vec![
                Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::DownDir)),
            ])
        },
        "focus_up" => |_event| {
            KeyResponse::ConsumeWith(vec![
                Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::UpDir)),
            ])
        },
        "focus_right" => |_event| {
            KeyResponse::ConsumeWith(vec![
                Command::WorkspaceCommand(WorkspaceCmd::FocusDirection(FocusDir::RightDir)),
            ])
        },
        "grow" => |_event| {
            KeyResponse::ConsumeWith(vec![Command::WorkspaceCommand(WorkspaceCmd::Resize(0.05))])
        },
        "shrink" => |_event| {
            KeyResponse::ConsumeWith(vec![Command::WorkspaceCommand(WorkspaceCmd::Resize(-0.05))])
        },
        "grow_v" => |_event| {
            KeyResponse::ConsumeWith(vec![Command::WorkspaceCommand(
                WorkspaceCmd::ResizeDirection(ResizeDirectionConfig {
                    direction: SplitDirection::Vertical,
                    delta: 0.05,
                }),
            )])
        },
        "shrink_v" => |_event| {
            KeyResponse::ConsumeWith(vec![Command::WorkspaceCommand(
                WorkspaceCmd::ResizeDirection(ResizeDirectionConfig {
                    direction: SplitDirection::Vertical,
                    delta: -0.05,
                }),
            )])
        },
        "close_pane" => |_event| {
            if let Some(pane_key) = state.panes.pop() {
                KeyResponse::ConsumeWith(vec![Command::ClosePaneClient(pane_key)])
            } else {
                KeyResponse::Consume
            }
        },
    },
}
