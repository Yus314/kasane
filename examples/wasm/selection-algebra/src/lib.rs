use std::collections::HashMap;

const HISTORY_MAX: usize = 20;

// ---------------------------------------------------------------------------
// Type definitions (outside define_plugin! for testability)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum PromptMode {
    Inactive = 0,
    Select = 1,
    Split = 2,
    SearchForward = 3,
    SearchBackward = 4,
    Keep = 5,
    Remove = 6,
}

impl PromptMode {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Select,
            2 => Self::Split,
            3 => Self::SearchForward,
            4 => Self::SearchBackward,
            5 => Self::Keep,
            6 => Self::Remove,
            _ => Self::Inactive,
        }
    }

    fn is_active(self) -> bool {
        self != Self::Inactive
    }

    fn is_set_operation(self) -> bool {
        matches!(self, Self::Keep | Self::Remove)
    }
}

fn detect_prompt_mode(prompt: &str) -> PromptMode {
    match prompt.trim_end() {
        "select:" => PromptMode::Select,
        "split:" => PromptMode::Split,
        "/" => PromptMode::SearchForward,
        "?" => PromptMode::SearchBackward,
        "keep:" => PromptMode::Keep,
        "remove:" => PromptMode::Remove,
        _ => PromptMode::Inactive,
    }
}

/// Pre-computed verdict for a single-line selection.
#[derive(Debug, Clone)]
struct SelectionVerdict {
    line: i32,
    start_col: u32,
    end_col: u32,
    matches: bool,
}

/// A captured selection operation for the history panel.
#[derive(Debug, Clone)]
struct HistoryEntry {
    mode: u8,
    regex: String,
    count_before: u32,
    count_after: u32,
}

// ---------------------------------------------------------------------------
// Pure helper functions (testable without SDK)
// ---------------------------------------------------------------------------

fn mode_short_label(mode: u8) -> &'static str {
    match PromptMode::from_u8(mode) {
        PromptMode::Select => "s:",
        PromptMode::Split => "S:",
        PromptMode::SearchForward => "/",
        PromptMode::SearchBackward => "?",
        PromptMode::Keep => "k:",
        PromptMode::Remove => "K:",
        PromptMode::Inactive => "",
    }
}

/// Truncate a regex string to at most `max_len` bytes, appending "…" if truncated.
fn truncate_regex(regex: &str, max_len: usize) -> String {
    if regex.len() <= max_len {
        return regex.to_string();
    }
    // Reserve 3 bytes for '…' (U+2026)
    let mut end = max_len.saturating_sub(3);
    while end > 0 && !regex.is_char_boundary(end) {
        end -= 1;
    }
    let mut s = regex[..end].to_string();
    s.push('\u{2026}');
    s
}

fn format_delta(before: u32, after: u32) -> String {
    let delta = after as i64 - before as i64;
    if delta > 0 {
        format!("+{delta}")
    } else if delta < 0 {
        format!("{delta}")
    } else {
        "=".to_string()
    }
}

// ---------------------------------------------------------------------------
// Face helpers (use SDK-generated functions via glob import)
// ---------------------------------------------------------------------------

fn match_face_for_mode(mode: PromptMode) -> Face {
    let dark = is_dark_background();
    match mode {
        PromptMode::SearchForward | PromptMode::SearchBackward => {
            let fb = if dark {
                face_bg(rgb(80, 80, 20))
            } else {
                face_bg(rgb(255, 255, 180))
            };
            theme_face_or("vsa.search", fb)
        }
        PromptMode::Select => {
            let fb = if dark {
                face_bg(rgb(20, 60, 20))
            } else {
                face_bg(rgb(200, 255, 200))
            };
            theme_face_or("vsa.select", fb)
        }
        PromptMode::Split => {
            let fb = if dark {
                face_bg(rgb(20, 30, 60))
            } else {
                face_bg(rgb(200, 220, 255))
            };
            theme_face_or("vsa.split", fb)
        }
        PromptMode::Keep => {
            let fb = if dark {
                face_bg(rgb(20, 60, 20))
            } else {
                face_bg(rgb(200, 255, 200))
            };
            theme_face_or("vsa.keep", fb)
        }
        PromptMode::Remove => {
            let fb = if dark {
                face_bg(rgb(60, 20, 20))
            } else {
                face_bg(rgb(255, 200, 200))
            };
            theme_face_or("vsa.remove", fb)
        }
        PromptMode::Inactive => default_face(),
    }
}

fn verdict_face(mode: PromptMode, matches: bool) -> Face {
    let will_keep = match mode {
        PromptMode::Keep => matches,
        PromptMode::Remove => !matches,
        _ => return default_face(),
    };
    let dark = is_dark_background();
    if will_keep {
        let fb = if dark {
            face_bg(rgb(20, 80, 20))
        } else {
            face_bg(rgb(180, 255, 180))
        };
        theme_face_or("vsa.verdict.keep", fb)
    } else {
        let fb = if dark {
            face_bg(rgb(80, 20, 20))
        } else {
            face_bg(rgb(255, 180, 180))
        };
        theme_face_or("vsa.verdict.remove", fb)
    }
}

fn diff_added_face() -> Face {
    let dark = is_dark_background();
    let fb = if dark {
        face_bg(rgb(20, 80, 20))
    } else {
        face_bg(rgb(180, 255, 180))
    };
    theme_face_or("vsa.diff.added", fb)
}

fn diff_removed_face() -> Face {
    let dark = is_dark_background();
    let fb = if dark {
        face_bg(rgb(80, 20, 20))
    } else {
        face_bg(rgb(255, 180, 180))
    };
    theme_face_or("vsa.diff.removed", fb)
}

fn panel_highlight_face() -> Face {
    theme_face_or(
        "vsa.panel.highlight",
        face(named(NamedColor::White), rgb(4, 57, 94)),
    )
}

// ---------------------------------------------------------------------------
// UI builders (use SDK-generated types)
// ---------------------------------------------------------------------------

fn build_history_panel(
    open: bool,
    selected: usize,
    history: &[HistoryEntry],
    ctx: &OverlayContext,
) -> Option<OverlayContribution> {
    if !open || history.is_empty() {
        return None;
    }

    let mut children: Vec<ElementHandle> = Vec::new();
    // Display newest-first (reverse iteration)
    for (i, entry) in history.iter().rev().enumerate() {
        let is_selected = i == selected;
        let label = mode_short_label(entry.mode);
        let re_disp = truncate_regex(&entry.regex, 20);
        let delta = format_delta(entry.count_before, entry.count_after);
        let line = format!(
            " {:<3}{:<22} {:>3}\u{2192}{:<3} {:>4} ",
            label, re_disp, entry.count_before, entry.count_after, delta,
        );
        let f = if is_selected {
            panel_highlight_face()
        } else {
            default_face()
        };
        children.push(text(&line, f));
    }

    let inner = column(&children);
    let title = format!(" Selection History ({}) ", history.len());
    let el = container(inner)
        .border(BorderLineStyle::Rounded)
        .shadow()
        .padding(edges(0, 1, 0, 0))
        .title_text(&title)
        .build();

    let anchor = content_fit_overlay(
        ctx.screen_cols,
        ctx.screen_rows,
        50,
        30,
        history.len() as u16,
        4,
    );

    Some(OverlayContribution {
        element: el,
        anchor: OverlayAnchor::Absolute(anchor),
        z_index: 90,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- detect_prompt_mode --

    #[test]
    fn mode_select() {
        assert_eq!(detect_prompt_mode("select:"), PromptMode::Select);
    }

    #[test]
    fn mode_split() {
        assert_eq!(detect_prompt_mode("split:"), PromptMode::Split);
    }

    #[test]
    fn mode_search_forward() {
        assert_eq!(detect_prompt_mode("/"), PromptMode::SearchForward);
    }

    #[test]
    fn mode_search_backward() {
        assert_eq!(detect_prompt_mode("?"), PromptMode::SearchBackward);
    }

    #[test]
    fn mode_keep() {
        assert_eq!(detect_prompt_mode("keep:"), PromptMode::Keep);
    }

    #[test]
    fn mode_remove() {
        assert_eq!(detect_prompt_mode("remove:"), PromptMode::Remove);
    }

    #[test]
    fn mode_inactive_on_empty() {
        assert_eq!(detect_prompt_mode(""), PromptMode::Inactive);
    }

    #[test]
    fn mode_inactive_on_unknown() {
        assert_eq!(detect_prompt_mode("prompt:"), PromptMode::Inactive);
    }

    #[test]
    fn mode_trailing_whitespace_is_trimmed() {
        assert_eq!(detect_prompt_mode("select: "), PromptMode::Select);
        assert_eq!(detect_prompt_mode("/  "), PromptMode::SearchForward);
    }

    #[test]
    fn mode_leading_space_is_not_trimmed() {
        assert_eq!(detect_prompt_mode(" select:"), PromptMode::Inactive);
    }

    #[test]
    fn mode_from_u8_roundtrip() {
        for mode in [
            PromptMode::Inactive,
            PromptMode::Select,
            PromptMode::Split,
            PromptMode::SearchForward,
            PromptMode::SearchBackward,
            PromptMode::Keep,
            PromptMode::Remove,
        ] {
            assert_eq!(PromptMode::from_u8(mode as u8), mode);
        }
    }

    #[test]
    fn mode_from_u8_invalid() {
        assert_eq!(PromptMode::from_u8(99), PromptMode::Inactive);
    }

    #[test]
    fn mode_is_active() {
        assert!(!PromptMode::Inactive.is_active());
        assert!(PromptMode::Select.is_active());
        assert!(PromptMode::SearchForward.is_active());
    }

    #[test]
    fn mode_is_set_operation() {
        assert!(!PromptMode::Select.is_set_operation());
        assert!(PromptMode::Keep.is_set_operation());
        assert!(PromptMode::Remove.is_set_operation());
    }

    // -- regex-lite basics --

    #[test]
    fn regex_basic_match() {
        let re = regex_lite::Regex::new(r"\d+").unwrap();
        let matches: Vec<_> = re.find_iter("abc 123 def 456").collect();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start(), 4);
        assert_eq!(matches[0].end(), 7);
        assert_eq!(matches[1].start(), 12);
        assert_eq!(matches[1].end(), 15);
    }

    #[test]
    fn regex_empty_pattern() {
        let re = regex_lite::Regex::new("").unwrap();
        assert!(re.is_match("anything"));
    }

    #[test]
    fn regex_invalid_pattern() {
        assert!(regex_lite::Regex::new("(unclosed").is_err());
        assert!(regex_lite::Regex::new("[bad").is_err());
    }

    #[test]
    fn regex_byte_offset_utf8() {
        let re = regex_lite::Regex::new("world").unwrap();
        let text = "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f} world";
        let m = re.find(text).unwrap();
        assert_eq!(m.start(), 16);
        assert_eq!(m.end(), 21);
    }

    // -- mode_short_label --

    #[test]
    fn short_labels() {
        assert_eq!(mode_short_label(PromptMode::Select as u8), "s:");
        assert_eq!(mode_short_label(PromptMode::Split as u8), "S:");
        assert_eq!(mode_short_label(PromptMode::SearchForward as u8), "/");
        assert_eq!(mode_short_label(PromptMode::SearchBackward as u8), "?");
        assert_eq!(mode_short_label(PromptMode::Keep as u8), "k:");
        assert_eq!(mode_short_label(PromptMode::Remove as u8), "K:");
        assert_eq!(mode_short_label(PromptMode::Inactive as u8), "");
    }

    // -- truncate_regex --

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate_regex("abc", 10), "abc");
    }

    #[test]
    fn truncate_exact_boundary() {
        assert_eq!(truncate_regex("abcdefghij", 10), "abcdefghij");
    }

    #[test]
    fn truncate_long_gets_ellipsis() {
        let result = truncate_regex("abcdefghijk", 10);
        assert!(result.ends_with('\u{2026}'));
        assert!(result.len() <= 10);
    }

    #[test]
    fn truncate_utf8_boundary() {
        // "\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}" = 15 bytes, 5 chars
        let input = "\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}";
        assert_eq!(input.len(), 15);
        let result = truncate_regex(input, 8);
        assert!(result.ends_with('\u{2026}'));
        // 8 - 3 = 5 byte budget → "\u{3042}" (3 bytes) fits, next char boundary at 6 > 5
        assert!(result.starts_with('\u{3042}'));
    }

    #[test]
    fn truncate_empty() {
        assert_eq!(truncate_regex("", 10), "");
    }

    // -- format_delta --

    #[test]
    fn delta_positive() {
        assert_eq!(format_delta(1, 5), "+4");
    }

    #[test]
    fn delta_negative() {
        assert_eq!(format_delta(5, 3), "-2");
    }

    #[test]
    fn delta_zero() {
        assert_eq!(format_delta(3, 3), "=");
    }

    // -- HistoryEntry --

    #[test]
    fn history_entry_display() {
        let entry = HistoryEntry {
            mode: PromptMode::Select as u8,
            regex: r"\d+".to_string(),
            count_before: 1,
            count_after: 5,
        };
        assert_eq!(mode_short_label(entry.mode), "s:");
        assert_eq!(format_delta(entry.count_before, entry.count_after), "+4");
    }

    // -- timer gen --

    #[test]
    fn timer_gen_encode_decode() {
        let generation: u32 = 42;
        let bytes = generation.to_le_bytes();
        assert_eq!(bytes.len(), 4);
        assert_eq!(u32::from_le_bytes(bytes), 42);
    }

    #[test]
    fn timer_gen_zero() {
        let generation: u32 = 0;
        let bytes = generation.to_le_bytes();
        assert_eq!(u32::from_le_bytes(bytes), 0);
    }

    // -- panel format --

    #[test]
    fn panel_line_format() {
        let entry = HistoryEntry {
            mode: PromptMode::Select as u8,
            regex: r"\d+".to_string(),
            count_before: 1,
            count_after: 5,
        };
        let label = mode_short_label(entry.mode);
        let re_disp = truncate_regex(&entry.regex, 20);
        let delta = format_delta(entry.count_before, entry.count_after);
        let line = format!(
            " {:<3}{:<22} {:>3}\u{2192}{:<3} {:>4} ",
            label, re_disp, entry.count_before, entry.count_after, delta,
        );
        assert!(line.contains("s:"));
        assert!(line.contains(r"\d+"));
        assert!(line.contains("+4"));
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Collect current cursor positions, sorted for stable diff.
fn collect_cursors(primary_line: i32, primary_col: i32) -> Vec<(i32, i32)> {
    let mut cursors = Vec::new();
    cursors.push((primary_line, primary_col));
    let sec_count = host_state::get_secondary_cursor_count();
    for i in 0..sec_count {
        if let Some(coord) = host_state::get_secondary_cursor(i) {
            cursors.push((coord.line, coord.column));
        }
    }
    cursors.sort();
    cursors
}

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    settings {
        enabled: bool = true,
        diff_fade_ms: i64 = 2000,
    },

    state {
        // Phase 1: Regex preview
        mode: u8 = 0,
        regex_text: String = String::new(),
        regex_ok: bool = false,
        match_lines: HashMap<u32, Vec<(u32, u32)>> = HashMap::new(),
        match_count: u32 = 0,
        search_start: u32 = 0,
        search_end: u32 = 0,
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        cursor_line: i32 = 0,

        // Phase 2: Selection diff
        #[bind(host_state::get_cursor_count(), on: dirty::BUFFER)]
        cursor_count: u32 = 0,
        prev_cursor_count: u32 = 0,
        prev_cursors: Vec<(i32, i32)> = Vec::new(),
        diff_added: Vec<(i32, i32)> = Vec::new(),
        diff_removed: Vec<(i32, i32)> = Vec::new(),
        diff_active: bool = false,
        cursor_initialized: bool = false,
        diff_timer_gen: u32 = 0,

        // Phase 3: Selection verdicts
        selection_verdicts: Vec<SelectionVerdict> = Vec::new(),

        // Phase 4b: History capture
        pending_capture: bool = false,
        pending_mode: u8 = 0,
        pending_regex: String = String::new(),
        pending_count_before: u32 = 0,
        history: Vec<HistoryEntry> = Vec::new(),

        // Phase 4c: History panel
        panel_open: bool = false,
        panel_selected: usize = 0,
    },

    on_state_changed_effects(dirty) {
        // ---- Enabled guard ----
        if !__setting_enabled() {
            if state.regex_ok || state.diff_active || !state.match_lines.is_empty() {
                state.regex_text.clear();
                state.regex_ok = false;
                state.match_lines.clear();
                state.match_count = 0;
                state.selection_verdicts.clear();
                state.diff_active = false;
                state.diff_added.clear();
                state.diff_removed.clear();
                state.mode = 0;
            }
            state.pending_capture = false;
            state.pending_regex.clear();
            if state.panel_open {
                state.panel_open = false;
            }
            return Effects::default();
        }

        let mut cmds: Vec<Command> = Vec::new();
        let mut need_scan = false;

        // ---- Phase 1: Prompt mode detection ----
        if dirty & dirty::STATUS != 0 {
            let prompt_atoms = host_state::get_status_prompt();
            let prompt: String = prompt_atoms.iter().map(|a| a.contents.as_str()).collect();
            let new_mode = detect_prompt_mode(&prompt);
            let old_mode = PromptMode::from_u8(state.mode);
            state.mode = new_mode as u8;

            if new_mode.is_active() {
                // Close panel when entering active mode
                if state.panel_open {
                    state.panel_open = false;
                }

                let content_atoms = host_state::get_status_content();
                let new_text: String =
                    content_atoms.iter().map(|a| a.contents.as_str()).collect();

                if new_text.is_empty() {
                    state.regex_text.clear();
                    state.regex_ok = false;
                    state.match_lines.clear();
                    state.match_count = 0;
                    state.selection_verdicts.clear();
                } else if new_text != state.regex_text {
                    state.regex_text = new_text;
                    match regex_lite::Regex::new(&state.regex_text) {
                        Ok(_) => {
                            state.regex_ok = true;
                            need_scan = true;
                        }
                        Err(_) => {
                            state.regex_ok = false;
                            state.match_lines.clear();
                            state.match_count = 0;
                            state.selection_verdicts.clear();
                        }
                    }
                }
            } else {
                // Transitioning to Inactive — capture history if coming from active
                if old_mode.is_active() && !state.regex_text.is_empty() {
                    state.pending_capture = true;
                    state.pending_mode = old_mode as u8;
                    state.pending_regex = state.regex_text.clone();
                    state.pending_count_before = state.prev_cursor_count;
                }

                // Clear all regex state
                if !state.regex_text.is_empty() || state.regex_ok {
                    state.regex_text.clear();
                    state.regex_ok = false;
                    state.match_lines.clear();
                    state.match_count = 0;
                    state.selection_verdicts.clear();
                }
            }
        }

        // Buffer content changed while regex is active — re-scan
        if dirty & dirty::BUFFER != 0 && state.regex_ok {
            need_scan = true;
        }

        // ---- Phase 1 & 3: Scan visible lines ----
        if need_scan && state.regex_ok
            && let Ok(re) = regex_lite::Regex::new(&state.regex_text)
        {
            let rows = host_state::get_rows() as u32;
            let cursor = state.cursor_line.max(0) as u32;
            let line_count = host_state::get_line_count();
            let start = cursor.saturating_sub(rows);
            let end = (cursor + rows).min(line_count);
            state.search_start = start;
            state.search_end = end;

            let lines = host_state::get_lines_text(start, end);
            state.match_lines.clear();
            let mut total: u32 = 0;

            for (i, line_text) in lines.iter().enumerate() {
                let matches: Vec<(u32, u32)> = re
                    .find_iter(line_text)
                    .take(200)
                    .map(|m| (m.start() as u32, m.end() as u32))
                    .collect();
                if !matches.is_empty() {
                    total = total.saturating_add(matches.len() as u32);
                    state.match_lines.insert(start + i as u32, matches);
                }
            }
            state.match_count = total;

            // Phase 3: Selection verdicts for Keep/Remove
            let mode = PromptMode::from_u8(state.mode);
            if mode.is_set_operation() {
                let sel_count = host_state::get_selection_count();
                state.selection_verdicts.clear();
                for i in 0..sel_count {
                    if let Some(sel) = host_state::get_selection(i)
                        && sel.anchor.line == sel.cursor.line
                    {
                        let line = sel.anchor.line;
                        let (s, e) = if sel.anchor.column <= sel.cursor.column {
                            (
                                sel.anchor.column as u32,
                                sel.cursor.column as u32 + 1,
                            )
                        } else {
                            (
                                sel.cursor.column as u32,
                                sel.anchor.column as u32 + 1,
                            )
                        };
                        let matches = host_state::get_line_text(line as u32)
                            .and_then(|t| t.get(s as usize..e as usize).map(String::from))
                            .is_some_and(|t| re.is_match(&t));
                        state.selection_verdicts.push(SelectionVerdict {
                            line,
                            start_col: s,
                            end_col: e,
                            matches,
                        });
                    }
                }
            } else {
                state.selection_verdicts.clear();
            }
        }

        // ---- Phase 2 & 4b: Cursor diff + history capture ----
        if dirty & dirty::BUFFER != 0 {
            let mode = PromptMode::from_u8(state.mode);

            if mode == PromptMode::Inactive {
                // Finalize pending history capture
                if state.pending_capture {
                    state.pending_capture = false;
                    let entry = HistoryEntry {
                        mode: state.pending_mode,
                        regex: std::mem::take(&mut state.pending_regex),
                        count_before: state.pending_count_before,
                        count_after: state.cursor_count,
                    };
                    state.history.push(entry);
                    if state.history.len() > HISTORY_MAX {
                        state.history.remove(0);
                    }
                    if state.panel_selected >= state.history.len()
                        && !state.history.is_empty()
                    {
                        state.panel_selected = state.history.len() - 1;
                    }
                }

                // Cursor diff
                let new_count = state.cursor_count;
                let current =
                    collect_cursors(state.cursor_line, host_state::get_cursor_col());

                if !state.cursor_initialized {
                    state.prev_cursors = current;
                    state.prev_cursor_count = new_count;
                    state.cursor_initialized = true;
                } else if new_count != state.prev_cursor_count {
                    let mut added = Vec::new();
                    let mut removed = Vec::new();
                    for c in &current {
                        if !state.prev_cursors.contains(c) {
                            added.push(*c);
                        }
                    }
                    for c in &state.prev_cursors {
                        if !current.contains(c) {
                            removed.push(*c);
                        }
                    }
                    state.diff_added = added;
                    state.diff_removed = removed;
                    state.diff_active =
                        !state.diff_added.is_empty() || !state.diff_removed.is_empty();
                    state.prev_cursors = current;
                    state.prev_cursor_count = new_count;

                    // Schedule fade-out timer
                    if state.diff_active {
                        let fade_ms = __setting_diff_fade_ms();
                        if fade_ms > 0 {
                            state.diff_timer_gen = state.diff_timer_gen.wrapping_add(1);
                            cmds.push(Command::ScheduleTimer(TimerConfig {
                                timer_id: state.diff_timer_gen as u64,
                                delay_ms: fade_ms as u64,
                                target_plugin: "selection_algebra".to_string(),
                                payload: state.diff_timer_gen.to_le_bytes().to_vec(),
                            }));
                        }
                    }
                } else {
                    if state.diff_active {
                        state.diff_active = false;
                        state.diff_added.clear();
                        state.diff_removed.clear();
                    }
                    state.prev_cursors = current;
                }
            } else if state.diff_active {
                state.diff_active = false;
                state.diff_added.clear();
                state.diff_removed.clear();
            }
        }

        effects(cmds)
    },

    update_effects(payload) {
        if !__setting_enabled() {
            return Effects::default();
        }
        if payload.len() >= 4 {
            let generation = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
            if generation == state.diff_timer_gen && state.diff_active {
                state.diff_active = false;
                state.diff_added.clear();
                state.diff_removed.clear();
                return just_redraw();
            }
        }
        Effects::default()
    },

    display() {
        if !__setting_enabled() {
            return vec![];
        }

        let mode = PromptMode::from_u8(state.mode);
        let mut directives: Vec<DisplayDirective> = Vec::new();

        // ---- Phase 1 & 3: Active mode — regex match + verdict highlights ----
        if mode.is_active() {
            let f = match_face_for_mode(mode);
            for (&line, matches) in &state.match_lines {
                for &(start, end) in matches {
                    directives.push(style_inline(line, start, end, f));
                }
            }

            if mode.is_set_operation() {
                for v in &state.selection_verdicts {
                    directives.push(style_inline(
                        v.line as u32,
                        v.start_col,
                        v.end_col,
                        verdict_face(mode, v.matches),
                    ));
                }
            }
        }

        // ---- Phase 2: Inactive mode — diff highlights ----
        if !mode.is_active() && state.diff_active {
            for &(dl, dc) in &state.diff_added {
                let col = dc.max(0) as u32;
                directives.push(style_inline(dl as u32, col, col + 1, diff_added_face()));
            }

            for &(dl, dc) in &state.diff_removed {
                let col = dc.max(0) as u32;
                directives.push(style_inline(dl as u32, col, col + 1, diff_removed_face()));
            }
        }

        directives
    },

    slots {
        STATUS_RIGHT(dirty::STATUS | dirty::BUFFER | dirty::SETTINGS) => |_ctx| {
            if !__setting_enabled() {
                return None;
            }

            let mode = PromptMode::from_u8(state.mode);
            if mode.is_active() {
                // Active mode: show match count or error
                if !state.regex_ok {
                    if !state.regex_text.is_empty() {
                        let f = theme_face_or(
                            "vsa.error",
                            face_fg(named(NamedColor::Red)),
                        );
                        return Some(auto_contribution(text(" \u{26a0} ", f)));
                    }
                    return None;
                }
                let count = state.match_count;
                if count > 0 {
                    let label = format!(" {} matches ", count);
                    Some(auto_contribution(text(&label, default_face())))
                } else if !state.regex_text.is_empty() {
                    let f = face_fg(named(NamedColor::BrightBlack));
                    Some(auto_contribution(text(" 0 matches ", f)))
                } else {
                    None
                }
            } else if let Some(last) = state.history.last() {
                // Inactive: show last operation summary (dimmed)
                let label = mode_short_label(last.mode);
                let re_disp = truncate_regex(&last.regex, 15);
                let delta = format_delta(last.count_before, last.count_after);
                let summary = format!(
                    " {}{} {}\u{2192}{} {} ",
                    label, re_disp, last.count_before, last.count_after, delta,
                );
                let f = face_fg(named(NamedColor::BrightBlack));
                Some(auto_contribution(text(&summary, f)))
            } else {
                None
            }
        },
    },

    overlay(ctx) {
        if !__setting_enabled() {
            return None;
        }
        build_history_panel(state.panel_open, state.panel_selected, &state.history, &ctx)
    },

    key_map {
        when(state.panel_open) {
            key(Escape)  => "close_panel",
            char('q')    => "close_panel",
            key(Up)      => "panel_up",
            key(Down)    => "panel_down",
            any()        => "consume_panel",
        },
    },

    actions {
        "toggle_panel" => |_event| {
            if state.panel_open {
                state.panel_open = false;
            } else if !state.history.is_empty() {
                state.panel_open = true;
                state.panel_selected = 0;
            }
            KeyResponse::ConsumeRedraw
        },
        "close_panel" => |_event| {
            state.panel_open = false;
            KeyResponse::ConsumeRedraw
        },
        "panel_up" => |_event| {
            if state.panel_selected > 0 {
                state.panel_selected -= 1;
            }
            KeyResponse::ConsumeRedraw
        },
        "panel_down" => |_event| {
            let len = state.history.len();
            if len > 0 && state.panel_selected < len - 1 {
                state.panel_selected += 1;
            }
            KeyResponse::ConsumeRedraw
        },
        "consume_panel" => |_event| {
            KeyResponse::Consume
        },
    },
}
