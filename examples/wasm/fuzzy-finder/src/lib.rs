kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");

use std::cell::RefCell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::types::*;
use kasane::plugin::element_builder;
use kasane_plugin_sdk::{dirty, modifiers};

// ---------------------------------------------------------------------------
// Job ID scheme
// ---------------------------------------------------------------------------

const JOB_FD: u64 = 1;
const JOB_FIND_FALLBACK: u64 = 2;
const JOB_FZF_BASE: u64 = 100;

// ---------------------------------------------------------------------------
// State
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

#[derive(Default)]
struct PluginState {
    state: FzfState,
    query: String,
    file_list: Vec<String>,
    results: Vec<String>,
    selected: usize,
    generation: u64,
    fd_buf: Vec<u8>,
    fzf_buf: Vec<u8>,
    fzf_job_gen: u64,
}

impl PluginState {
    fn current_fzf_job_id(&self) -> u64 {
        JOB_FZF_BASE + self.fzf_job_gen
    }

    fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn reset(&mut self) {
        self.state = FzfState::Inactive;
        self.query.clear();
        self.file_list.clear();
        self.results.clear();
        self.selected = 0;
        self.fd_buf.clear();
        self.fzf_buf.clear();
        self.bump_generation();
    }

    fn visible_results(&self) -> &[String] {
        if self.query.is_empty() {
            &self.file_list
        } else {
            &self.results
        }
    }

    fn clamp_selected(&mut self) {
        let len = self.visible_results().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }
}

thread_local! {
    static STATE: RefCell<PluginState> = RefCell::new(PluginState::default());
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn spawn_find_fallback_command() -> Vec<Command> {
    vec![Command::SpawnProcess(SpawnProcessConfig {
        job_id: JOB_FIND_FALLBACK,
        program: "find".to_string(),
        args: vec![
            ".".to_string(),
            "-type".to_string(),
            "f".to_string(),
            "-not".to_string(),
            "-path".to_string(),
            "*/.git/*".to_string(),
        ],
        stdin_mode: StdinMode::NullStdin,
    })]
}

fn spawn_fzf_filter(state: &PluginState) -> Vec<Command> {
    let job_id = state.current_fzf_job_id();
    let file_data = state.file_list.join("\n");
    let mut cmds = vec![
        Command::SpawnProcess(SpawnProcessConfig {
            job_id,
            program: "fzf".to_string(),
            args: vec!["--filter".to_string(), state.query.clone()],
            stdin_mode: StdinMode::Piped,
        }),
        Command::WriteToProcess(WriteProcessConfig {
            job_id,
            data: file_data.into_bytes(),
        }),
        Command::CloseProcessStdin(job_id),
    ];
    cmds.push(Command::RequestRedraw(dirty::ALL));
    cmds
}

fn kill_active_processes(state: &PluginState) -> Vec<Command> {
    let mut cmds = Vec::new();
    match state.state {
        FzfState::Scanning => {
            cmds.push(Command::KillProcess(JOB_FD));
            cmds.push(Command::KillProcess(JOB_FIND_FALLBACK));
        }
        FzfState::Filtering => {
            cmds.push(Command::KillProcess(state.current_fzf_job_id()));
        }
        _ => {}
    }
    cmds
}

fn push_literal_keys(keys: &mut Vec<String>, text: &str) {
    for ch in text.chars() {
        match ch {
            ' ' => keys.push("<space>".into()),
            '<' => keys.push("<lt>".into()),
            '>' => keys.push("<gt>".into()),
            '-' => keys.push("<minus>".into()),
            '%' => keys.push("<percent>".into()),
            c => keys.push(c.to_string()),
        }
    }
}

fn edit_file_keys(path: &str) -> Vec<String> {
    let mut keys = vec!["<esc>".to_string(), ":".to_string()];
    push_literal_keys(&mut keys, &format!("edit {path}"));
    keys.push("<ret>".to_string());
    keys
}

// ---------------------------------------------------------------------------
// Overlay UI
// ---------------------------------------------------------------------------

fn default_face() -> Face {
    Face {
        fg: Color::DefaultColor,
        bg: Color::DefaultColor,
        underline: Color::DefaultColor,
        attributes: 0,
    }
}

fn highlight_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Rgb(RgbColor { r: 4, g: 57, b: 94 }),
        underline: Color::DefaultColor,
        attributes: 0,
    }
}

fn dim_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightBlack),
        bg: Color::DefaultColor,
        underline: Color::DefaultColor,
        attributes: 0,
    }
}

fn error_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::Red),
        bg: Color::DefaultColor,
        underline: Color::DefaultColor,
        attributes: 0,
    }
}

fn build_overlay(state: &PluginState, ctx: &OverlayContext) -> Option<OverlayContribution> {
    if state.state == FzfState::Inactive {
        return None;
    }

    let cols = ctx.screen_cols;
    let rows = ctx.screen_rows;

    // Overlay dimensions: ~60% width, ~50% height, with minimums
    let w = (cols as u32 * 60 / 100).max(40).min(cols as u32) as u16;
    let h = (rows as u32 * 50 / 100).max(10).min(rows as u32) as u16;
    let x = (cols.saturating_sub(w)) / 2;
    let y = (rows.saturating_sub(h)) / 2;

    // Available rows for results: total height minus border(2) + query(1) + separator(1)
    let max_results = (h as usize).saturating_sub(4).max(1);

    let mut children: Vec<ElementHandle> = Vec::new();

    // Query input line: "> query_"
    let query_display = format!("> {}_", state.query);
    children.push(element_builder::create_text(&query_display, default_face()));

    // Separator
    let sep = "\u{2500}".repeat(w.saturating_sub(2) as usize);
    children.push(element_builder::create_text(&sep, dim_face()));

    // Content area
    match &state.state {
        FzfState::Scanning => {
            children.push(element_builder::create_text("Scanning files...", dim_face()));
        }
        FzfState::Error(msg) => {
            children.push(element_builder::create_text(msg, error_face()));
        }
        FzfState::Ready | FzfState::Filtering => {
            let items = state.visible_results();
            let visible_count = items.len().min(max_results);

            if items.is_empty() {
                let msg = if state.query.is_empty() {
                    "No files found"
                } else {
                    "No matches"
                };
                children.push(element_builder::create_text(msg, dim_face()));
            } else {
                // Scroll window around selected
                let start = if state.selected >= visible_count {
                    state.selected - visible_count + 1
                } else {
                    0
                };
                let end = (start + visible_count).min(items.len());

                for i in start..end {
                    let face = if i == state.selected {
                        highlight_face()
                    } else {
                        default_face()
                    };
                    let prefix = if i == state.selected { "> " } else { "  " };
                    let text = format!("{prefix}{}", &items[i]);
                    children.push(element_builder::create_text(&text, face));
                }
            }

        }
        FzfState::Inactive => unreachable!(),
    }

    let inner = element_builder::create_column(&children);

    let padding = Edges { top: 0, right: 1, bottom: 0, left: 1 };

    // Title: show file count on the border line
    let title_text = match &state.state {
        FzfState::Scanning => " Find File ── scanning... ".to_string(),
        FzfState::Filtering => {
            let total = state.file_list.len();
            let shown = state.results.len();
            format!(" Find File ── {shown}/{total} ")
        }
        FzfState::Ready => {
            let items = state.visible_results();
            let total = state.file_list.len();
            format!(" Find File ── {}/{total} ", items.len())
        }
        FzfState::Error(_) => " Find File ── error ".to_string(),
        FzfState::Inactive => unreachable!(),
    };
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

struct FuzzyFinderPlugin;

impl Guest for FuzzyFinderPlugin {
    fn get_id() -> String {
        "fuzzy_finder".to_string()
    }

    fn requested_capabilities() -> Vec<Capability> {
        vec![Capability::Process]
    }

    fn on_init() -> Vec<Command> {
        vec![]
    }

    fn on_shutdown() -> Vec<Command> {
        vec![]
    }

    fn on_state_changed(_dirty_flags: u16) -> Vec<Command> {
        vec![]
    }

    kasane_plugin_sdk::default_surfaces!();
    kasane_plugin_sdk::default_render_surface!();
    kasane_plugin_sdk::default_handle_surface_event!();
    kasane_plugin_sdk::default_handle_surface_state_changed!();

    fn state_hash() -> u64 {
        STATE.with(|s| {
            let s = s.borrow();
            s.generation
        })
    }

    fn slot_deps(_slot: u8) -> u16 {
        0
    }

    fn handle_key(event: KeyEvent) -> Option<Vec<Command>> {
        STATE.with(|s| {
            let mut state = s.borrow_mut();

            match state.state {
                FzfState::Inactive => {
                    // Ctrl+P activates
                    if matches!(event.key, KeyCode::Character(ref c) if c == "p")
                        && event.modifiers & modifiers::CTRL != 0
                    {
                        state.state = FzfState::Scanning;
                        state.query.clear();
                        state.file_list.clear();
                        state.results.clear();
                        state.selected = 0;
                        state.fd_buf.clear();
                        state.fzf_buf.clear();
                        state.bump_generation();
                        let mut cmds = spawn_fd_command();
                        cmds.push(Command::RequestRedraw(dirty::ALL));
                        return Some(cmds);
                    }
                    None
                }
                FzfState::Scanning | FzfState::Ready | FzfState::Filtering | FzfState::Error(_) => {
                    match &event.key {
                        KeyCode::Escape => {
                            let kill_cmds = kill_active_processes(&state);
                            state.reset();
                            let mut cmds = kill_cmds;
                            cmds.push(Command::RequestRedraw(dirty::ALL));
                            Some(cmds)
                        }
                        KeyCode::Enter => {
                            let items = state.visible_results();
                            let selected = if state.selected < items.len() {
                                Some(items[state.selected].clone())
                            } else {
                                None
                            };
                            let kill_cmds = kill_active_processes(&state);
                            state.reset();
                            let mut cmds = kill_cmds;
                            if let Some(path) = selected {
                                cmds.push(Command::SendKeys(edit_file_keys(&path)));
                            }
                            cmds.push(Command::RequestRedraw(dirty::ALL));
                            Some(cmds)
                        }
                        KeyCode::Up => {
                            if state.selected > 0 {
                                state.selected -= 1;
                                state.bump_generation();
                            }
                            Some(vec![Command::RequestRedraw(dirty::ALL)])
                        }
                        KeyCode::Down => {
                            let len = state.visible_results().len();
                            if len > 0 && state.selected < len - 1 {
                                state.selected += 1;
                                state.bump_generation();
                            }
                            Some(vec![Command::RequestRedraw(dirty::ALL)])
                        }
                        KeyCode::Backspace => {
                            if state.state == FzfState::Scanning {
                                return Some(vec![]);
                            }
                            state.query.pop();
                            state.selected = 0;
                            state.bump_generation();

                            if state.query.is_empty() {
                                // Show all files, kill any running fzf
                                state.results.clear();
                                state.state = FzfState::Ready;
                                let mut cmds = vec![Command::KillProcess(state.current_fzf_job_id())];
                                cmds.push(Command::RequestRedraw(dirty::ALL));
                                Some(cmds)
                            } else {
                                // Re-filter: kill previous fzf before spawning new one
                                state.fzf_job_gen += 1;
                                state.fzf_buf.clear();
                                state.state = FzfState::Filtering;
                                let prev_job = JOB_FZF_BASE + state.fzf_job_gen - 1;
                                let mut cmds = vec![Command::KillProcess(prev_job)];
                                cmds.extend(spawn_fzf_filter(&state));
                                Some(cmds)
                            }
                        }
                        KeyCode::Character(c) => {
                            if matches!(state.state, FzfState::Scanning | FzfState::Error(_)) {
                                return Some(vec![]);
                            }
                            // Ignore if modifier keys are pressed (except shift)
                            if event.modifiers & (modifiers::CTRL | modifiers::ALT) != 0 {
                                return Some(vec![]);
                            }
                            state.query.push_str(c);
                            state.selected = 0;
                            state.fzf_job_gen += 1;
                            state.fzf_buf.clear();
                            state.state = FzfState::Filtering;
                            state.bump_generation();
                            // Kill previous fzf if running
                            let prev_job = JOB_FZF_BASE + state.fzf_job_gen - 1;
                            let mut cmds = vec![Command::KillProcess(prev_job)];
                            cmds.extend(spawn_fzf_filter(&state));
                            Some(cmds)
                        }
                        KeyCode::Tab => Some(vec![]), // consume but ignore
                        _ => Some(vec![]), // consume all keys when active
                    }
                }
            }
        })
    }

    fn handle_mouse(_event: MouseEvent, _id: InteractiveId) -> Option<Vec<Command>> {
        None
    }

    fn observe_key(_event: KeyEvent) {}
    fn observe_mouse(_event: MouseEvent) {}

    fn on_io_event(event: IoEvent) -> Vec<Command> {
        STATE.with(|s| {
            let mut state = s.borrow_mut();

            match event {
                IoEvent::Process(pe) => {
                    let job_id = pe.job_id;
                    match pe.kind {
                        ProcessEventKind::Stdout(data) => {
                            if job_id == JOB_FD || job_id == JOB_FIND_FALLBACK {
                                state.fd_buf.extend_from_slice(&data);
                            } else if job_id == state.current_fzf_job_id() {
                                state.fzf_buf.extend_from_slice(&data);
                            }
                            // Stale fzf results are silently ignored
                            vec![]
                        }
                        ProcessEventKind::Stderr(_) => {
                            // Ignored for proof artifact
                            vec![]
                        }
                        ProcessEventKind::Exited(exit_code) => {
                            if job_id == JOB_FD || job_id == JOB_FIND_FALLBACK {
                                if exit_code == 0 || !state.fd_buf.is_empty() {
                                    state.file_list = split_lines(&state.fd_buf);
                                    state.fd_buf.clear();
                                    state.state = FzfState::Ready;
                                    state.bump_generation();

                                    // If query was typed while scanning, start filtering
                                    if !state.query.is_empty() {
                                        state.fzf_job_gen += 1;
                                        state.fzf_buf.clear();
                                        state.state = FzfState::Filtering;
                                        let mut cmds = spawn_fzf_filter(&state);
                                        cmds.push(Command::RequestRedraw(dirty::ALL));
                                        return cmds;
                                    }

                                    return vec![Command::RequestRedraw(dirty::ALL)];
                                }
                                // Non-zero exit with no data — treat as error
                                state.state = FzfState::Error(
                                    "file listing failed".to_string(),
                                );
                                state.bump_generation();
                                return vec![Command::RequestRedraw(dirty::ALL)];
                            }

                            if job_id == state.current_fzf_job_id() {
                                state.results = split_lines(&state.fzf_buf);
                                state.fzf_buf.clear();
                                state.clamp_selected();
                                state.state = FzfState::Ready;
                                state.bump_generation();
                                return vec![Command::RequestRedraw(dirty::ALL)];
                            }

                            // Stale fzf exit
                            vec![]
                        }
                        ProcessEventKind::SpawnFailed(error) => {
                            if job_id == JOB_FD {
                                // fd not found, try find
                                return spawn_find_fallback_command();
                            }
                            if job_id == JOB_FIND_FALLBACK {
                                state.state = FzfState::Error(
                                    "file listing command not found (tried fd, find)".to_string(),
                                );
                                state.bump_generation();
                                return vec![Command::RequestRedraw(dirty::ALL)];
                            }
                            if job_id == state.current_fzf_job_id() {
                                state.state = FzfState::Error(
                                    format!("fzf not installed: {error}"),
                                );
                                state.bump_generation();
                                return vec![Command::RequestRedraw(dirty::ALL)];
                            }
                            vec![]
                        }
                    }
                }
            }
        })
    }

    fn contribute_overlay_v2(ctx: OverlayContext) -> Option<OverlayContribution> {
        STATE.with(|s| {
            let state = s.borrow();
            build_overlay(&state, &ctx)
        })
    }

    fn contribute_overlay() -> Option<Overlay> {
        None
    }

    kasane_plugin_sdk::default_line!();
    kasane_plugin_sdk::default_contribute!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_update!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_named_slot!();
    kasane_plugin_sdk::default_contribute_to!();
    kasane_plugin_sdk::default_transform!();
    kasane_plugin_sdk::default_transform_priority!();
    kasane_plugin_sdk::default_contribute_deps!();
    kasane_plugin_sdk::default_transform_deps!();
    kasane_plugin_sdk::default_annotate!();
    kasane_plugin_sdk::default_annotate_deps!();
}

export!(FuzzyFinderPlugin);
