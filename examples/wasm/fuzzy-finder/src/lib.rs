// ---------------------------------------------------------------------------
// Job ID scheme
// ---------------------------------------------------------------------------

use kasane_plugin_sdk::process::{ProcessHandle, ProcessResult, ProcessStep};

const JOB_FD: u64 = 1;
const JOB_FIND_FALLBACK: u64 = 2;
const JOB_FZF_BASE: u64 = 100;

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum FzfState {
    Inactive,
    Scanning,
    Ready,
    Filtering,
    Error(String),
}

impl Default for FzfState {
    fn default() -> Self {
        FzfState::Inactive
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn current_fzf_job_id(fzf_job_gen: u64) -> u64 {
    JOB_FZF_BASE + fzf_job_gen
}

fn visible_results<'a>(query: &str, file_list: &'a [String], results: &'a [String]) -> &'a [String] {
    if query.is_empty() {
        file_list
    } else {
        results
    }
}

fn clamp_selected(selected: &mut usize, visible_len: usize) {
    if visible_len == 0 {
        *selected = 0;
    } else if *selected >= visible_len {
        *selected = visible_len - 1;
    }
}

fn split_lines(buf: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(buf);
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

fn spawn_fd_command() -> Vec<Command> {
    vec![Command::SpawnProcess(SpawnProcessConfig {
        job_id: JOB_FD,
        program: "fd".to_string(),
        args: vec!["--type".to_string(), "f".to_string()],
        stdin_mode: StdinMode::NullStdin,
    })]
}

fn spawn_fzf_filter(query: &str, file_list: &[String], fzf_job_gen: u64) -> Vec<Command> {
    let job_id = current_fzf_job_id(fzf_job_gen);
    let file_data = file_list.join("\n");
    vec![
        Command::SpawnProcess(SpawnProcessConfig {
            job_id,
            program: "fzf".to_string(),
            args: vec!["--filter".to_string(), query.to_string()],
            stdin_mode: StdinMode::Piped,
        }),
        Command::WriteToProcess(WriteProcessConfig {
            job_id,
            data: file_data.into_bytes(),
        }),
        Command::CloseProcessStdin(job_id),
        Command::RequestRedraw(dirty::ALL),
    ]
}

fn kill_active_processes(fzf_state: &FzfState, fzf_job_gen: u64) -> Vec<Command> {
    let mut cmds = Vec::new();
    match fzf_state {
        FzfState::Scanning => {
            cmds.push(Command::KillProcess(JOB_FD));
            cmds.push(Command::KillProcess(JOB_FIND_FALLBACK));
        }
        FzfState::Filtering => {
            cmds.push(Command::KillProcess(current_fzf_job_id(fzf_job_gen)));
        }
        _ => {}
    }
    cmds
}

// ---------------------------------------------------------------------------
// Overlay UI
// ---------------------------------------------------------------------------

fn highlight_face() -> Face {
    theme_face_or(
        "fuzzy_finder.highlight",
        face(named(NamedColor::White), rgb(4, 57, 94)),
    )
}

fn dim_face() -> Face {
    theme_face_or(
        "fuzzy_finder.dim",
        face_fg(named(NamedColor::BrightBlack)),
    )
}

fn error_face() -> Face {
    face_fg(named(NamedColor::Red))
}

fn build_fzf_overlay(
    fzf_state: &FzfState,
    query: &str,
    file_list: &[String],
    results: &[String],
    selected: usize,
    ctx: &OverlayContext,
) -> Option<OverlayContribution> {
    if *fzf_state == FzfState::Inactive {
        return None;
    }

    let anchor = centered_overlay(ctx.screen_cols, ctx.screen_rows, 60, 50, 40, 10);

    // Available rows for results: total height minus border(2) + query(1) + separator(1)
    let max_results = (anchor.h as usize).saturating_sub(4).max(1);

    let mut children: Vec<ElementHandle> = Vec::new();

    // Query input line: "> query_"
    let query_display = format!("> {query}_");
    children.push(text(&query_display, default_face()));

    // Separator
    let sep = "\u{2500}".repeat(anchor.w.saturating_sub(2) as usize);
    children.push(text(&sep, dim_face()));

    // Content area
    match fzf_state {
        FzfState::Scanning => {
            children.push(text("Scanning files...", dim_face()));
        }
        FzfState::Error(msg) => {
            children.push(text(msg, error_face()));
        }
        FzfState::Ready | FzfState::Filtering => {
            let items = visible_results(query, file_list, results);
            let visible_count = items.len().min(max_results);

            if items.is_empty() {
                let msg = if query.is_empty() {
                    "No files found"
                } else {
                    "No matches"
                };
                children.push(text(msg, dim_face()));
            } else {
                // Scroll window around selected
                let start = if selected >= visible_count {
                    selected - visible_count + 1
                } else {
                    0
                };
                let end = (start + visible_count).min(items.len());

                for i in start..end {
                    let f = if i == selected {
                        highlight_face()
                    } else {
                        default_face()
                    };
                    let prefix = if i == selected { "> " } else { "  " };
                    let label = format!("{prefix}{}", &items[i]);
                    children.push(text(&label, f));
                }
            }
        }
        FzfState::Inactive => unreachable!(),
    }

    let inner = column(&children);

    // Title: show file count on the border line
    let title_text = match fzf_state {
        FzfState::Scanning => " Find File ── scanning... ".to_string(),
        FzfState::Filtering => {
            let total = file_list.len();
            let shown = results.len();
            format!(" Find File ── {shown}/{total} ")
        }
        FzfState::Ready => {
            let items = visible_results(query, file_list, results);
            let total = file_list.len();
            format!(" Find File ── {}/{total} ", items.len())
        }
        FzfState::Error(_) => " Find File ── error ".to_string(),
        FzfState::Inactive => unreachable!(),
    };
    let el = container(inner)
        .border(BorderLineStyle::Rounded)
        .shadow()
        .padding(padding_h(1))
        .title_text(&title_text)
        .build();

    Some(OverlayContribution {
        element: el,
        anchor: OverlayAnchor::Absolute(anchor),
        z_index: 100,
    })
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        fzf_state: FzfState = FzfState::Inactive,
        query: String = String::new(),
        file_list: Vec<String> = Vec::new(),
        results: Vec<String> = Vec::new(),
        selected: usize = 0,
        fd_handle: ProcessHandle = ProcessHandle::new(JOB_FD).with_fallback(
            JOB_FIND_FALLBACK,
            ProcessStep {
                program: "find".to_string(),
                args: vec![
                    ".".to_string(),
                    "-type".to_string(),
                    "f".to_string(),
                    "-not".to_string(),
                    "-path".to_string(),
                    "*/.git/*".to_string(),
                ],
            },
        ),
        fzf_buf: Vec<u8> = Vec::new(),
        fzf_job_gen: u64 = 0,
    },

    handle_key(event) {
        match state.fzf_state {
            FzfState::Inactive => {
                // Ctrl+P activates
                if is_ctrl(&event, 'p') {
                    state.fzf_state = FzfState::Scanning;
                    state.query.clear();
                    state.file_list.clear();
                    state.results.clear();
                    state.selected = 0;
                    state.fd_handle.reset();
                    state.fzf_buf.clear();
                    let mut cmds = spawn_fd_command();
                    cmds.push(Command::RequestRedraw(dirty::ALL));
                    return Some(cmds);
                }
                None
            }
            FzfState::Scanning | FzfState::Ready | FzfState::Filtering | FzfState::Error(_) => {
                match &event.key {
                    KeyCode::Escape => {
                        let kill_cmds = kill_active_processes(&state.fzf_state, state.fzf_job_gen);
                        state.fzf_state = FzfState::Inactive;
                        state.query.clear();
                        state.file_list.clear();
                        state.results.clear();
                        state.selected = 0;
                        state.fd_handle.reset();
                        state.fzf_buf.clear();
                        let mut cmds = kill_cmds;
                        cmds.push(Command::RequestRedraw(dirty::ALL));
                        Some(cmds)
                    }
                    KeyCode::Enter => {
                        let items = visible_results(&state.query, &state.file_list, &state.results);
                        let selected = if state.selected < items.len() {
                            Some(items[state.selected].clone())
                        } else {
                            None
                        };
                        let kill_cmds = kill_active_processes(&state.fzf_state, state.fzf_job_gen);
                        state.fzf_state = FzfState::Inactive;
                        state.query.clear();
                        state.file_list.clear();
                        state.results.clear();
                        state.selected = 0;
                        state.fd_handle.reset();
                        state.fzf_buf.clear();
                        let mut cmds = kill_cmds;
                        if let Some(path) = selected {
                            cmds.push(Command::SendKeys(keys::command(&format!(
                                "edit {path}"
                            ))));
                        }
                        cmds.push(Command::RequestRedraw(dirty::ALL));
                        Some(cmds)
                    }
                    KeyCode::Up => nav_up(&mut state.selected),
                    KeyCode::Down => {
                        let len = visible_results(&state.query, &state.file_list, &state.results).len();
                        nav_down(&mut state.selected, len)
                    }
                    KeyCode::Backspace => {
                        if state.fzf_state == FzfState::Scanning {
                            return consumed();
                        }
                        state.query.pop();
                        state.selected = 0;

                        if state.query.is_empty() {
                            // Show all files, kill any running fzf
                            state.results.clear();
                            state.fzf_state = FzfState::Ready;
                            let mut cmds =
                                vec![Command::KillProcess(current_fzf_job_id(state.fzf_job_gen))];
                            cmds.push(Command::RequestRedraw(dirty::ALL));
                            Some(cmds)
                        } else {
                            // Re-filter: kill previous fzf before spawning new one
                            state.fzf_job_gen += 1;
                            state.fzf_buf.clear();
                            state.fzf_state = FzfState::Filtering;
                            let prev_job = JOB_FZF_BASE + state.fzf_job_gen - 1;
                            let mut cmds = vec![Command::KillProcess(prev_job)];
                            cmds.extend(spawn_fzf_filter(&state.query, &state.file_list, state.fzf_job_gen));
                            Some(cmds)
                        }
                    }
                    KeyCode::Char(c) => {
                        if matches!(state.fzf_state, FzfState::Scanning | FzfState::Error(_)) {
                            return consumed();
                        }
                        // Ignore if modifier keys are pressed (except shift)
                        if event.modifiers & (modifiers::CTRL | modifiers::ALT) != 0 {
                            return consumed();
                        }
                        if let Some(ch) = char::from_u32(*c) {
                            state.query.push(ch);
                        }
                        state.selected = 0;
                        state.fzf_job_gen += 1;
                        state.fzf_buf.clear();
                        state.fzf_state = FzfState::Filtering;
                        // Kill previous fzf if running
                        let prev_job = JOB_FZF_BASE + state.fzf_job_gen - 1;
                        let mut cmds = vec![Command::KillProcess(prev_job)];
                        cmds.extend(spawn_fzf_filter(&state.query, &state.file_list, state.fzf_job_gen));
                        Some(cmds)
                    }
                    KeyCode::Tab => consumed(), // consume but ignore
                    _ => consumed(),            // consume all keys when active
                }
            }
        }
    },

    on_io_event_effects(event) {
        let IoEvent::Process(pe) = event else {
            return effects(vec![]);
        };
        let job_id = pe.job_id;
        let io_kind = to_io_event_kind(&pe.kind);

        // --- fd / find (file listing via ProcessHandle) ---
        match state.fd_handle.feed(job_id, io_kind) {
            ProcessResult::Pending => return effects(vec![]),
            ProcessResult::Completed(data) => {
                state.file_list = split_lines(&data);
                state.fzf_state = FzfState::Ready;

                if !state.query.is_empty() {
                    state.fzf_job_gen += 1;
                    state.fzf_buf.clear();
                    state.fzf_state = FzfState::Filtering;
                    let mut cmds = spawn_fzf_filter(&state.query, &state.file_list, state.fzf_job_gen);
                    cmds.push(Command::RequestRedraw(dirty::ALL));
                    return effects(cmds);
                }
                return just_redraw();
            }
            ProcessResult::TryFallback => {
                let (step, fb_id) = state.fd_handle.fallback_info().unwrap();
                return effects(vec![Command::SpawnProcess(SpawnProcessConfig {
                    job_id: fb_id,
                    program: step.program.clone(),
                    args: step.args.clone(),
                    stdin_mode: StdinMode::NullStdin,
                })]);
            }
            ProcessResult::Failed(msg) => {
                state.fzf_state = FzfState::Error(
                    format!("file listing command not found: {msg}"),
                );
                return just_redraw();
            }
            ProcessResult::Ignored => { /* fall through to fzf handling */ }
        }

        // --- fzf (filtering) ---
        let fzf_job = current_fzf_job_id(state.fzf_job_gen);
        if job_id != fzf_job {
            return effects(vec![]);
        }
        match pe.kind {
            ProcessEventKind::Stdout(data) => {
                state.fzf_buf.extend_from_slice(&data);
                effects(vec![])
            }
            ProcessEventKind::Stderr(_) => effects(vec![]),
            ProcessEventKind::Exited(_) => {
                state.results = split_lines(&state.fzf_buf);
                state.fzf_buf.clear();
                let vis_len = visible_results(&state.query, &state.file_list, &state.results).len();
                clamp_selected(&mut state.selected, vis_len);
                state.fzf_state = FzfState::Ready;
                just_redraw()
            }
            ProcessEventKind::SpawnFailed(error) => {
                state.fzf_state = FzfState::Error(format!("fzf not installed: {error}"));
                just_redraw()
            }
        }
    },

    overlay(ctx) {
        build_fzf_overlay(
            &state.fzf_state,
            &state.query,
            &state.file_list,
            &state.results,
            state.selected,
            &ctx,
        )
    },
}
