kasane_plugin_sdk::generate!();

use kasane_plugin_sdk::{dirty, modifiers, plugin};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

kasane_plugin_sdk::state! {
    struct PluginState {
        session_count: u32 = 0,
        active_key: Option<String> = None,
        switcher_open: bool = false,
        selected: usize = 0,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn highlight_face() -> Face {
    face(named(NamedColor::White), rgb(4, 57, 94))
}

fn active_face() -> Face {
    face_fg(named(NamedColor::Green))
}

// ---------------------------------------------------------------------------
// Overlay UI
// ---------------------------------------------------------------------------

fn build_switcher_overlay(
    state: &PluginState,
    ctx: &OverlayContext,
) -> Option<OverlayContribution> {
    if !state.switcher_open {
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
            let is_selected = i as usize == state.selected;
            let marker = if is_active { "*" } else { " " };
            let buf = desc.buffer_name.as_deref().unwrap_or("");
            let mode = desc.mode_line.as_deref().unwrap_or("");

            let text = if buf.is_empty() && mode.is_empty() {
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
            children.push(element_builder::create_text(&text, f));
        }
    }

    let inner = element_builder::create_column(&children);
    let padding = Edges {
        top: 0,
        right: 1,
        bottom: 0,
        left: 0,
    };

    let title_text = format!(" Sessions ({count}) ");
    let title_atoms = [Atom {
        face: default_face(),
        contents: title_text,
    }];
    let container = element_builder::create_container_styled(
        inner,
        Some(BorderLineStyle::Rounded),
        true,
        padding,
        default_face(),
        Some(&title_atoms),
    );

    Some(OverlayContribution {
        element: container,
        anchor: OverlayAnchor::Absolute(AbsoluteAnchor { x, y, w, h }),
        z_index: 100,
    })
}

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

struct SessionUiPlugin;

#[plugin]
impl Guest for SessionUiPlugin {
    fn get_id() -> String {
        "session_ui".to_string()
    }

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
        if dirty_flags & dirty::SESSION != 0 {
            STATE.with(|s| {
                let mut state = s.borrow_mut();
                state.session_count = host_state::get_session_count();
                state.active_key = host_state::get_active_session_key();
                // Close switcher on session change to avoid stale overlay state
                state.switcher_open = false;
                // Clamp selected if sessions were removed
                if state.session_count > 0 && state.selected >= state.session_count as usize {
                    state.selected = state.session_count as usize - 1;
                }
                state.bump_generation();
            });
        }
        vec![]
    }

    fn state_hash() -> u64 {
        STATE.with(|s| s.borrow().generation)
    }

    fn contribute_to(region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
        kasane_plugin_sdk::route_slot_ids!(region, {
            STATUS_RIGHT => {
                STATE.with(|s| {
                    let state = s.borrow();
                    if state.session_count <= 1 {
                        return None;
                    }
                    let key = state.active_key.as_deref().unwrap_or("?");
                    let text = format!(" [{}:{}] ", state.session_count, key);
                    let el = element_builder::create_text(&text, highlight_face());
                    Some(Contribution {
                        element: el,
                        priority: 10,
                        size_hint: ContribSizeHint::Auto,
                    })
                })
            },
        })
    }

    fn contribute_deps(region: SlotId) -> u16 {
        kasane_plugin_sdk::route_slot_id_deps!(region, {
            STATUS_RIGHT => dirty::SESSION,
        })
    }

    fn handle_key(event: KeyEvent) -> Option<Vec<Command>> {
        STATE.with(|s| {
            let mut state = s.borrow_mut();

            if !state.switcher_open {
                // Ctrl+T opens the switcher
                if matches!(event.key, KeyCode::Character(ref c) if c == "t")
                    && event.modifiers & modifiers::CTRL != 0
                {
                    state.switcher_open = true;
                    state.selected = 0;
                    state.bump_generation();
                    return Some(vec![Command::RequestRedraw(dirty::ALL)]);
                }
                return None;
            }

            // Switcher is open — consume all keys
            match &event.key {
                KeyCode::Escape => {
                    state.switcher_open = false;
                    state.bump_generation();
                    Some(vec![Command::RequestRedraw(dirty::ALL)])
                }
                KeyCode::Character(c) if c == "t" && event.modifiers & modifiers::CTRL != 0 => {
                    // Ctrl+T toggles off
                    state.switcher_open = false;
                    state.bump_generation();
                    Some(vec![Command::RequestRedraw(dirty::ALL)])
                }
                KeyCode::Up => {
                    if state.selected > 0 {
                        state.selected -= 1;
                        state.bump_generation();
                    }
                    Some(vec![Command::RequestRedraw(dirty::ALL)])
                }
                KeyCode::Down => {
                    let count = state.session_count as usize;
                    if count > 0 && state.selected < count - 1 {
                        state.selected += 1;
                        state.bump_generation();
                    }
                    Some(vec![Command::RequestRedraw(dirty::ALL)])
                }
                KeyCode::Enter => {
                    // Switch to selected session
                    let selected = state.selected;
                    state.switcher_open = false;
                    state.bump_generation();
                    let mut cmds = vec![Command::RequestRedraw(dirty::ALL)];
                    if let Some(desc) = host_state::get_session(selected as u32) {
                        cmds.push(Command::SwitchSession(desc.key));
                    }
                    Some(cmds)
                }
                KeyCode::Character(c) if c == "n" => {
                    // Create a new session and activate it
                    state.switcher_open = false;
                    state.bump_generation();
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
        })
    }

    fn contribute_overlay_v2(ctx: OverlayContext) -> Option<OverlayContribution> {
        STATE.with(|s| {
            let state = s.borrow();
            build_switcher_overlay(&state, &ctx)
        })
    }
}

export!(SessionUiPlugin);
