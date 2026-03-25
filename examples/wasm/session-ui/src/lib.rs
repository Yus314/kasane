// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn highlight_face() -> Face {
    theme_face_or(
        "session_ui.highlight",
        face(named(NamedColor::White), rgb(4, 57, 94)),
    )
}

fn active_face() -> Face {
    theme_face_or(
        "session_ui.active",
        face_fg(named(NamedColor::Green)),
    )
}

// ---------------------------------------------------------------------------
// Overlay UI
// ---------------------------------------------------------------------------

fn build_switcher_overlay(
    switcher_open: bool,
    selected: usize,
    ctx: &OverlayContext,
) -> Option<OverlayContribution> {
    if !switcher_open {
        return None;
    }

    let count = host_state::get_session_count();
    if count == 0 {
        return None;
    }

    // Size: ~50% width, height based on session count + border + title
    let content_rows = count as u16;
    let w = (ctx.screen_cols as u32 * 50 / 100)
        .max(30)
        .min(ctx.screen_cols as u32) as u16;
    let h = (content_rows + 4).min(ctx.screen_rows); // border(2) + title-sep(1) + padding(1)
    let x = (ctx.screen_cols.saturating_sub(w)) / 2;
    let y = (ctx.screen_rows.saturating_sub(h)) / 2;

    let active_key = host_state::get_active_session_key();
    let mut children: Vec<ElementHandle> = Vec::new();

    for i in 0..count {
        if let Some(desc) = host_state::get_session(i) {
            let is_active = active_key.as_deref() == Some(&desc.key);
            let is_selected = i as usize == selected;
            let marker = if is_active { "*" } else { " " };
            let buf = desc.buffer_name.as_deref().unwrap_or("");
            let mode = desc.mode_line.as_deref().unwrap_or("");

            let label = if buf.is_empty() && mode.is_empty() {
                format!(" [{marker}] {}", desc.key)
            } else if mode.is_empty() {
                format!(" [{marker}] {}  {buf}", desc.key)
            } else {
                format!(" [{marker}] {}  {buf}  {mode}", desc.key)
            };

            let f = if is_selected {
                highlight_face()
            } else if is_active {
                active_face()
            } else {
                default_face()
            };
            children.push(text(&label, f));
        }
    }

    let inner = column(&children);
    let title_text = format!(" Sessions ({count}) ");
    let el = container(inner)
        .border(BorderLineStyle::Rounded)
        .shadow()
        .padding(edges(0, 1, 0, 0))
        .title_text(&title_text)
        .build();

    Some(OverlayContribution {
        element: el,
        anchor: OverlayAnchor::Absolute(AbsoluteAnchor { x, y, w, h }),
        z_index: 100,
    })
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

kasane_plugin_sdk::define_plugin! {
    id: "session_ui",

    state {
        #[bind(host_state::get_session_count(), on: dirty::SESSION)]
        session_count: u32 = 0,
        #[bind(host_state::get_active_session_key(), on: dirty::SESSION)]
        active_key: Option<String> = None,
        switcher_open: bool = false,
        selected: usize = 0,
    },

    on_state_changed_effects(dirty) {
        if dirty & dirty::SESSION != 0 {
            state.switcher_open = false;
            if state.session_count > 0 && state.selected >= state.session_count as usize {
                state.selected = state.session_count as usize - 1;
            }
        }
        RuntimeEffects::default()
    },

    slots {
        STATUS_RIGHT(dirty::SESSION) => |_ctx| {
            if state.session_count <= 1 {
                return None;
            }
            let key = state.active_key.as_deref().unwrap_or("?");
            let label = format!(" [{}:{}] ", state.session_count, key);
            let el = text(&label, highlight_face());
            Some(Contribution {
                element: el,
                priority: 10,
                size_hint: ContribSizeHint::Auto,
            })
        },
    },

    handle_key(event) {
        if !state.switcher_open {
            // Ctrl+T opens the switcher
            if is_ctrl(&event, "t") {
                state.switcher_open = true;
                state.selected = 0;
                return Some(vec![Command::RequestRedraw(dirty::ALL)]);
            }
            return None;
        }

        // Switcher is open — consume all keys
        match &event.key {
            KeyCode::Escape => {
                state.switcher_open = false;
                Some(vec![Command::RequestRedraw(dirty::ALL)])
            }
            KeyCode::Character(c) if c == "t" && event.modifiers & modifiers::CTRL != 0 => {
                // Ctrl+T toggles off
                state.switcher_open = false;
                Some(vec![Command::RequestRedraw(dirty::ALL)])
            }
            KeyCode::Up => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
                Some(vec![Command::RequestRedraw(dirty::ALL)])
            }
            KeyCode::Down => {
                let count = state.session_count as usize;
                if count > 0 && state.selected < count - 1 {
                    state.selected += 1;
                }
                Some(vec![Command::RequestRedraw(dirty::ALL)])
            }
            KeyCode::Enter => {
                // Switch to selected session
                let selected = state.selected;
                state.switcher_open = false;
                let mut cmds = vec![Command::RequestRedraw(dirty::ALL)];
                if let Some(desc) = host_state::get_session(selected as u32) {
                    cmds.push(Command::SwitchSession(desc.key));
                }
                Some(cmds)
            }
            KeyCode::Character(c) if c == "n" => {
                // Create a new session and activate it
                state.switcher_open = false;
                Some(vec![
                    Command::RequestRedraw(dirty::ALL),
                    Command::SpawnSession(SessionConfig {
                        key: None,
                        session: None,
                        args: vec![],
                        activate: true,
                    }),
                ])
            }
            KeyCode::Character(c) if c == "d" => {
                // Close selected session (guard: don't close last)
                if state.session_count <= 1 {
                    return Some(vec![]);
                }
                let selected = state.selected;
                if let Some(desc) = host_state::get_session(selected as u32) {
                    let mut cmds = vec![Command::RequestRedraw(dirty::ALL)];
                    cmds.push(Command::CloseSession(Some(desc.key)));
                    return Some(cmds);
                }
                Some(vec![])
            }
            _ => Some(vec![]), // consume all keys when open
        }
    },

    overlay(ctx) {
        build_switcher_overlay(state.switcher_open, state.selected, &ctx)
    },
}
