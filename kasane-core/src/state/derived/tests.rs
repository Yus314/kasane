use super::*;
use crate::protocol::{Atom, Attributes, Color, Coord, CursorMode, Face, NamedColor};
use crate::render::CursorStyle;

fn make_atom(text: &str) -> Atom {
    Atom::plain(text)
}

fn make_cursor_atom(text: &str) -> Atom {
    Atom::from_face(
        Face {
            attributes: Attributes::FINAL_FG | Attributes::REVERSE,
            ..Face::default()
        },
        text,
    )
}

// --- detect_cursors tests ---

#[test]
fn detect_cursors_empty_buffer() {
    let (count, secondary) = detect_cursors(&[], Coord::default());
    assert_eq!(count, 0);
    assert!(secondary.is_empty());
}

#[test]
fn detect_cursors_single_primary() {
    let lines = vec![vec![
        make_atom("hel"),
        make_cursor_atom("l"),
        make_atom("o"),
    ]];
    let primary = Coord { line: 0, column: 3 };
    let (count, secondary) = detect_cursors(&lines, primary);
    assert_eq!(count, 1);
    assert!(secondary.is_empty());
}

#[test]
fn detect_cursors_with_secondary() {
    let lines = vec![
        vec![make_cursor_atom("h"), make_atom("ello")],
        vec![make_atom("wor"), make_cursor_atom("l"), make_atom("d")],
    ];
    let primary = Coord { line: 0, column: 0 };
    let (count, secondary) = detect_cursors(&lines, primary);
    assert_eq!(count, 2);
    assert_eq!(secondary.len(), 1);
    assert_eq!(secondary[0], Coord { line: 1, column: 3 });
}

#[test]
fn detect_cursors_cjk_width() {
    // CJK character "漢" is 2 cells wide
    let lines = vec![vec![make_atom("漢"), make_cursor_atom("x")]];
    let primary = Coord { line: 0, column: 2 };
    let (count, secondary) = detect_cursors(&lines, primary);
    assert_eq!(count, 1);
    assert!(secondary.is_empty());
}

// --- detect_cursors face-matching fallback tests ---

/// Helper: create an atom with an explicit fg+bg face (no REVERSE/FINAL_FG),
/// mimicking third-party themes like anhsirk0/kakoune-themes.
fn make_themed_cursor_atom(text: &str, fg: Color, bg: Color) -> Atom {
    Atom::from_face(
        Face {
            fg,
            bg,
            ..Face::default()
        },
        text,
    )
}

#[test]
fn detect_cursors_fallback_single_primary() {
    // Theme: PrimaryCursor = dark,purple (no +rfg)
    let dark = Color::Rgb {
        r: 0x1e,
        g: 0x21,
        b: 0x27,
    };
    let purple = Color::Rgb {
        r: 0xc6,
        g: 0x78,
        b: 0xdd,
    };
    let lines = vec![vec![
        make_atom("hel"),
        make_themed_cursor_atom("l", dark, purple),
        make_atom("o"),
    ]];
    let primary = Coord { line: 0, column: 3 };
    let (count, secondary) = detect_cursors(&lines, primary);
    assert_eq!(count, 1);
    assert!(secondary.is_empty());
}

#[test]
fn detect_cursors_fallback_with_secondary() {
    // PrimaryCursor = dark,purple; SecondaryCursor = dark,blue
    let dark = Color::Rgb {
        r: 0x1e,
        g: 0x21,
        b: 0x27,
    };
    let purple = Color::Rgb {
        r: 0xc6,
        g: 0x78,
        b: 0xdd,
    };
    let blue = Color::Rgb {
        r: 0x61,
        g: 0xaf,
        b: 0xef,
    };
    let lines = vec![
        vec![
            make_themed_cursor_atom("h", dark, purple),
            make_atom("ello"),
        ],
        vec![
            make_atom("wor"),
            make_themed_cursor_atom("l", dark, blue),
            make_atom("d"),
        ],
    ];
    let primary = Coord { line: 0, column: 0 };
    let (count, secondary) = detect_cursors(&lines, primary);
    assert_eq!(count, 2);
    assert_eq!(secondary.len(), 1);
    assert_eq!(secondary[0], Coord { line: 1, column: 3 });
}

// --- compute_lines_dirty tests ---

#[test]
fn lines_dirty_same_content() {
    let lines = vec![vec![make_atom("hello")]];
    let face = Face::default();
    let dirty = compute_lines_dirty(&lines, &lines, &face, &face, &face, &face);
    assert_eq!(dirty, vec![false]);
}

#[test]
fn lines_dirty_changed_content() {
    let old = vec![vec![make_atom("hello")]];
    let new = vec![vec![make_atom("world")]];
    let face = Face::default();
    let dirty = compute_lines_dirty(&old, &new, &face, &face, &face, &face);
    assert_eq!(dirty, vec![true]);
}

#[test]
fn lines_dirty_length_change_marks_all() {
    let old = vec![vec![make_atom("a")]];
    let new = vec![vec![make_atom("a")], vec![make_atom("b")]];
    let face = Face::default();
    let dirty = compute_lines_dirty(&old, &new, &face, &face, &face, &face);
    assert_eq!(dirty, vec![true, true]);
}

#[test]
fn lines_dirty_face_change_marks_all() {
    let lines = vec![vec![make_atom("hello")]];
    let old_face = Face::default();
    let new_face = Face {
        fg: Color::Named(NamedColor::Red),
        ..Face::default()
    };
    let dirty = compute_lines_dirty(&lines, &lines, &old_face, &new_face, &old_face, &old_face);
    assert_eq!(dirty, vec![true]);
}

// --- derive_cursor_mode tests ---

#[test]
fn cursor_mode_prompt() {
    assert_eq!(derive_cursor_mode(0), CursorMode::Prompt);
    assert_eq!(derive_cursor_mode(5), CursorMode::Prompt);
}

#[test]
fn cursor_mode_buffer() {
    assert_eq!(derive_cursor_mode(-1), CursorMode::Buffer);
    assert_eq!(derive_cursor_mode(-100), CursorMode::Buffer);
}

// --- build_status_line tests ---

#[test]
fn build_status_line_combines() {
    let prompt = vec![make_atom(":")];
    let content = vec![make_atom("edit foo")];
    let result = build_status_line(&prompt, &content);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].contents.as_str(), ":");
    assert_eq!(result[1].contents.as_str(), "edit foo");
}

#[test]
fn build_status_line_empty_prompt() {
    let content = vec![make_atom("normal")];
    let result = build_status_line(&[], &content);
    assert_eq!(result.len(), 1);
}

// --- derive_editor_mode tests ---

#[test]
fn editor_mode_normal() {
    let mode_line = vec![make_atom("normal")];
    assert_eq!(
        derive_editor_mode(CursorMode::Buffer, &mode_line),
        EditorMode::Normal
    );
}

#[test]
fn editor_mode_insert() {
    let mode_line = vec![make_atom("insert")];
    assert_eq!(
        derive_editor_mode(CursorMode::Buffer, &mode_line),
        EditorMode::Insert
    );
}

#[test]
fn editor_mode_replace() {
    let mode_line = vec![make_atom("replace")];
    assert_eq!(
        derive_editor_mode(CursorMode::Buffer, &mode_line),
        EditorMode::Replace
    );
}

#[test]
fn editor_mode_prompt() {
    let mode_line = vec![make_atom("insert")];
    // Prompt takes priority over mode_line content
    assert_eq!(
        derive_editor_mode(CursorMode::Prompt, &mode_line),
        EditorMode::Prompt
    );
}

#[test]
fn editor_mode_empty_mode_line() {
    assert_eq!(
        derive_editor_mode(CursorMode::Buffer, &vec![]),
        EditorMode::Normal
    );
}

// --- derive_cursor_style tests ---

#[test]
fn cursor_style_ui_option_override() {
    let mut opts = std::collections::HashMap::new();
    opts.insert("kasane_cursor_style".to_string(), "bar".to_string());
    assert_eq!(
        derive_cursor_style(&opts, true, CursorMode::Buffer, &vec![]),
        CursorStyle::Bar
    );
}

#[test]
fn cursor_style_unfocused() {
    let opts = std::collections::HashMap::new();
    assert_eq!(
        derive_cursor_style(&opts, false, CursorMode::Buffer, &vec![]),
        CursorStyle::Outline
    );
}

#[test]
fn cursor_style_prompt_mode() {
    let opts = std::collections::HashMap::new();
    assert_eq!(
        derive_cursor_style(&opts, true, CursorMode::Prompt, &vec![]),
        CursorStyle::Bar
    );
}

#[test]
fn cursor_style_insert_mode() {
    let opts = std::collections::HashMap::new();
    let mode_line = vec![make_atom("insert")];
    assert_eq!(
        derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
        CursorStyle::Bar
    );
}

#[test]
fn cursor_style_replace_mode() {
    let opts = std::collections::HashMap::new();
    let mode_line = vec![make_atom("replace")];
    assert_eq!(
        derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
        CursorStyle::Underline
    );
}

#[test]
fn cursor_style_normal_mode() {
    let opts = std::collections::HashMap::new();
    let mode_line = vec![make_atom("normal")];
    assert_eq!(
        derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
        CursorStyle::Block
    );
}

// --- R-1: check_cursor_width_consistency tests ---

#[test]
fn width_consistency_ascii() {
    let lines = vec![vec![
        make_atom("hel"),
        make_cursor_atom("l"),
        make_atom("o"),
    ]];
    let cursor_pos = Coord { line: 0, column: 3 };
    assert_eq!(check_cursor_width_consistency(&lines, cursor_pos), None);
}

#[test]
fn width_consistency_cjk() {
    // "漢" is 2 columns wide, cursor at column 2
    let lines = vec![vec![make_atom("漢"), make_cursor_atom("x")]];
    let cursor_pos = Coord { line: 0, column: 2 };
    assert_eq!(check_cursor_width_consistency(&lines, cursor_pos), None);
}

#[test]
fn width_consistency_divergence_detected() {
    // Cursor claims to be at column 5 but line is only 4 columns wide ("hell")
    let lines = vec![vec![make_atom("hell")]];
    let cursor_pos = Coord { line: 0, column: 5 };
    let result = check_cursor_width_consistency(&lines, cursor_pos);
    assert!(result.is_some());
    let div = result.unwrap();
    assert_eq!(div.protocol_column, 5);
    assert_eq!(div.computed_column, 4);
}

// --- I-1: check_primary_cursor_in_set tests ---

#[test]
fn primary_in_set_single_cursor() {
    assert!(check_primary_cursor_in_set(
        1,
        &[],
        Coord { line: 0, column: 0 },
    ));
}

#[test]
fn primary_in_set_multi_cursor() {
    let secondaries = vec![Coord { line: 1, column: 3 }];
    assert!(check_primary_cursor_in_set(
        2,
        &secondaries,
        Coord { line: 0, column: 0 },
    ));
}

#[test]
fn primary_in_set_primary_not_detected() {
    // cursor_count=2 and 2 secondaries → primary wasn't in detected set
    // (valid: primary face may differ from detection heuristic)
    let secondaries = vec![Coord { line: 0, column: 0 }, Coord { line: 1, column: 3 }];
    assert!(check_primary_cursor_in_set(
        2,
        &secondaries,
        Coord { line: 2, column: 0 },
    ));
}

#[test]
fn primary_in_set_impossible_count() {
    // cursor_count=1 but 2 secondaries → impossible
    let secondaries = vec![Coord { line: 0, column: 0 }, Coord { line: 1, column: 3 }];
    assert!(!check_primary_cursor_in_set(
        1,
        &secondaries,
        Coord { line: 2, column: 0 },
    ));
}

#[test]
fn primary_in_set_empty_buffer() {
    assert!(check_primary_cursor_in_set(0, &[], Coord::default(),));
}

// --- detect_cursors_incremental tests ---

#[test]
fn detect_cursors_incremental_matches_full_on_all_dirty() {
    let lines = vec![
        vec![make_cursor_atom("h"), make_atom("ello")],
        vec![make_atom("wor"), make_cursor_atom("l"), make_atom("d")],
    ];
    let primary = Coord { line: 0, column: 0 };
    let all_dirty = vec![true; lines.len()];
    let mut cache = CursorCache::default();

    let (inc_count, inc_sec) = detect_cursors_incremental(&lines, primary, &all_dirty, &mut cache);
    let (full_count, full_sec) = detect_cursors(&lines, primary);

    assert_eq!(inc_count, full_count);
    assert_eq!(inc_sec, full_sec);
}

#[test]
fn detect_cursors_incremental_with_partial_dirty() {
    // Initial: cursors on lines 0 and 1
    let lines_v1 = vec![
        vec![make_cursor_atom("h"), make_atom("ello")],
        vec![make_atom("wor"), make_cursor_atom("l"), make_atom("d")],
        vec![make_atom("line3")],
    ];
    let primary = Coord { line: 0, column: 0 };
    let mut cache = CursorCache::default();

    // Warm the cache with a full scan
    let all_dirty = vec![true; lines_v1.len()];
    detect_cursors_incremental(&lines_v1, primary, &all_dirty, &mut cache);

    // Now change only line 1 (move cursor away)
    let lines_v2 = vec![
        vec![make_cursor_atom("h"), make_atom("ello")],
        vec![make_atom("world")],
        vec![make_atom("line3")],
    ];
    let partial_dirty = vec![false, true, false];
    let (count, sec) = detect_cursors_incremental(&lines_v2, primary, &partial_dirty, &mut cache);

    // Only line 0 should have a cursor now
    assert_eq!(count, 1);
    assert!(sec.is_empty());

    // Verify matches full scan
    let (full_count, full_sec) = detect_cursors(&lines_v2, primary);
    assert_eq!(count, full_count);
    assert_eq!(sec, full_sec);
}

#[test]
fn detect_cursors_incremental_line_count_change_forces_full_scan() {
    let lines_v1 = vec![
        vec![make_cursor_atom("a"), make_atom("bc")],
        vec![make_atom("def")],
    ];
    let primary = Coord { line: 0, column: 0 };
    let mut cache = CursorCache::default();

    // Warm cache
    let all_dirty = vec![true; lines_v1.len()];
    detect_cursors_incremental(&lines_v1, primary, &all_dirty, &mut cache);
    assert_eq!(cache.per_line.len(), 2);

    // Change to 3 lines — should force full scan
    let lines_v2 = vec![
        vec![make_cursor_atom("a"), make_atom("bc")],
        vec![make_atom("def")],
        vec![make_cursor_atom("g")],
    ];
    let dirty_2 = vec![false, false, true]; // wrong length vs cache
    let (count, sec) = detect_cursors_incremental(&lines_v2, primary, &dirty_2, &mut cache);

    assert_eq!(count, 2); // cursors on line 0 and 2
    assert_eq!(sec.len(), 1);
    assert_eq!(cache.per_line.len(), 3);
}

#[test]
fn detect_cursors_incremental_face_fallback_forces_full_rescan() {
    // Lines with no FINAL_FG+REVERSE — will trigger face fallback
    let dark = Color::Rgb {
        r: 0x1e,
        g: 0x21,
        b: 0x27,
    };
    let purple = Color::Rgb {
        r: 0xc6,
        g: 0x78,
        b: 0xdd,
    };
    let lines = vec![vec![
        make_atom("hel"),
        make_themed_cursor_atom("l", dark, purple),
        make_atom("o"),
    ]];
    let primary = Coord { line: 0, column: 3 };
    let mut cache = CursorCache::default();

    let all_dirty = vec![true];
    let (count, _sec) = detect_cursors_incremental(&lines, primary, &all_dirty, &mut cache);

    // Face fallback should be used
    assert!(cache.used_fallback);
    assert_eq!(count, 1);

    // Next call should force full scan since used_fallback is set
    let (count2, _sec2) = detect_cursors_incremental(&lines, primary, &[false], &mut cache);
    assert_eq!(count2, 1);
}

#[test]
fn scan_line_cursors_by_attributes_per_line() {
    // "hel" (3) + cursor "l" (1) + "o" (1) + cursor "!" (1) = columns 0..6
    let line = vec![
        make_atom("hel"),
        make_cursor_atom("l"),
        make_atom("o"),
        make_cursor_atom("!"),
    ];
    let mut out = Vec::new();
    cursor::scan_line_cursors_by_attributes(&line, 5, &mut out);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0], Coord { line: 5, column: 3 });
    assert_eq!(out[1], Coord { line: 5, column: 5 }); // 3+1+1 = 5
}

// --- detect_selections tests ---

fn make_selection_atom(text: &str) -> Atom {
    Atom::from_face(
        Face {
            bg: Color::Named(NamedColor::Blue),
            ..Face::default()
        },
        text,
    )
}

#[test]
fn detect_selections_single_char_cursor() {
    // No selection highlight around cursor → returns selection with anchor == cursor
    let lines = vec![vec![
        make_atom("hel"),
        make_cursor_atom("l"),
        make_atom("o"),
    ]];
    let cursor = Coord { line: 0, column: 3 };
    let sels = detect_selections(&lines, cursor, &[], &Face::default());
    // Cursor has REVERSE+FINAL_FG bg which is Default (no bg set) → detection
    // depends on whether cursor face bg is non-default. Since default cursor
    // atom has bg=Default, no selection bg is found.
    assert!(sels.is_empty() || sels[0].anchor == sels[0].cursor);
}

#[test]
fn detect_selections_with_selection_face() {
    // "he" + selection "ll" + cursor "o" + selection " w" + "orld"
    // Selection face: blue bg. Cursor face: REVERSE+FINAL_FG.
    let lines = vec![vec![
        make_atom("he"),
        make_selection_atom("ll"),
        make_cursor_atom("o"),
        make_selection_atom(" w"),
        make_atom("orld"),
    ]];
    let cursor = Coord { line: 0, column: 4 }; // "he"=2, "ll"=2, cursor at 4
    let sels = detect_selections(&lines, cursor, &[], &Face::default());
    assert_eq!(sels.len(), 1);
    assert!(sels[0].is_primary);
    // Selection should span from "ll" start (col 2) to " w" end (col 6)
    assert_eq!(sels[0].anchor.column, 2);
    assert_eq!(sels[0].cursor.column, 6); // "o"=1 + " w"=2 → col 4+1+2-1=6
}

#[test]
fn detect_selections_empty_lines() {
    let sels = detect_selections(&[], Coord::default(), &[], &Face::default());
    assert!(sels.is_empty());
}

#[test]
fn detect_selections_too_many_cursors() {
    let lines = vec![vec![make_atom("text")]];
    let cursor = Coord { line: 0, column: 0 };
    // 65 secondary cursors → exceeds safety valve
    let secondaries: Vec<Coord> = (0..65).map(|i| Coord { line: 0, column: i }).collect();
    let sels = detect_selections(&lines, cursor, &secondaries, &Face::default());
    assert!(sels.is_empty());
}
