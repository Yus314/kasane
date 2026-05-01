//! CellGrid golden snapshot tests (ADR-031 Phase A — Step 2).
//!
//! These tests render a deterministic Kakoune protocol scene through the
//! full kasane-core pipeline (protocol → state → view → layout → paint)
//! and compare the resulting [`CellGrid`] against a committed text
//! snapshot. They are the regression gate for ADR-031 Phase A's B-wide
//! PR (Atom → `Arc<Style>` + hot-path WireFace removal): every code path
//! that B-wide modifies sits between protocol input and CellGrid output,
//! so any unintended pixel-level behavioural change manifests as a
//! CellGrid diff first.
//!
//! ## Why CellGrid, not pixels
//!
//! Pixel-level golden tests via wgpu would require breaking
//! [`SceneRenderer::render_inner`]'s surface coupling — a non-trivial
//! refactor that ADR-032 W2 deferred. CellGrid sits one layer above and
//! captures everything B-wide can change (resolved face, grapheme
//! placement, cell width). The GPU pipeline below CellGrid is unchanged
//! by B-wide, so a CellGrid match implies a pixel match.
//!
//! Pixel tests remain valuable for Phase 10 (font metrics, subpixel,
//! variable font axes); they are layered on top of this gate later.
//!
//! ## Snapshot update workflow
//!
//! - Default: each test asserts the dumped CellGrid matches the
//!   committed file at `tests/golden/snapshots/<name>.snap.txt`.
//! - First run with no snapshot: the test writes one and passes
//!   (bootstrap mode).
//! - Updating: set `KASANE_GOLDEN_UPDATE=1` to overwrite.

use std::path::PathBuf;

use kasane_core::protocol::{
    Atom, Attributes, Color, Coord, KakouneRequest, NamedColor, StatusStyle, Style, WireFace,
};
use kasane_core::render::{CellGrid, RenderResult};
use kasane_core::test_support::{render_to_grid, render_to_grid_with_result, test_state_80x24};

// ---------------------------------------------------------------------------
// Snapshot harness
// ---------------------------------------------------------------------------

fn snapshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/snapshots")
}

fn update_mode() -> bool {
    std::env::var("KASANE_GOLDEN_UPDATE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Deterministic text dump of a [`CellGrid`].
///
/// Format (versioned to allow future changes without silent diffs):
///
/// ```text
/// # golden snapshot v1
/// # dim <W> <H>
///
/// # faces (legend, ids assigned in first-seen scan order)
/// f0 <face debug>
/// f1 <face debug>
/// ...
///
/// # rows
/// row 00: |<full row text, trailing spaces preserved>|
///         faces: 0..N=fK ...    (run-length face ids)
/// row 01: |...|
///         faces: ...
/// ...
///
/// # non-default widths (only emitted when present)
/// (x, y): width=W
/// ```
///
/// Diffs surface as: a single row's text changing, a single face run
/// shifting, or a width regression at a known cell. WireFace additions appear
/// as new `fN` legend entries with the corresponding `=fN` reference in a
/// row's run list.
pub fn dump_cellgrid(grid: &CellGrid) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    let _ = writeln!(output, "# golden snapshot v1");
    let _ = writeln!(output, "# dim {} {}", grid.width(), grid.height());

    // First pass: assign face ids in first-seen order (deterministic
    // because we walk the grid in (y, x) order). Also collect per-row
    // text + face id sequences + widths.
    let mut faces: Vec<kasane_core::render::TerminalStyle> = Vec::new();
    let mut row_text: Vec<String> = Vec::with_capacity(grid.height() as usize);
    let mut row_faces: Vec<Vec<usize>> = Vec::with_capacity(grid.height() as usize);
    let mut row_widths: Vec<Vec<u8>> = Vec::with_capacity(grid.height() as usize);

    for y in 0..grid.height() {
        let mut text = String::with_capacity(grid.width() as usize);
        let mut ids = Vec::with_capacity(grid.width() as usize);
        let mut widths = Vec::with_capacity(grid.width() as usize);
        for x in 0..grid.width() {
            let cell = grid.get(x, y).expect("cell in bounds");
            text.push_str(&cell.grapheme);
            let id = match faces.iter().position(|f| *f == cell.style) {
                Some(i) => i,
                None => {
                    faces.push(cell.style);
                    faces.len() - 1
                }
            };
            ids.push(id);
            widths.push(cell.width);
        }
        row_text.push(text);
        row_faces.push(ids);
        row_widths.push(widths);
    }

    // Faces legend.
    let _ = writeln!(output, "\n# faces");
    for (i, face) in faces.iter().enumerate() {
        let _ = writeln!(output, "f{i} {face:?}");
    }

    // Rows.
    let _ = writeln!(output, "\n# rows");
    for y in 0..grid.height() {
        let text = &row_text[y as usize];
        let faces_row = &row_faces[y as usize];
        let _ = writeln!(output, "row {y:02}: |{text}|");

        let mut face_str = String::new();
        let mut i = 0;
        while i < faces_row.len() {
            let f = faces_row[i];
            let mut j = i + 1;
            while j < faces_row.len() && faces_row[j] == f {
                j += 1;
            }
            let _ = write!(face_str, " {i}..{j}=f{f}");
            i = j;
        }
        let _ = writeln!(output, "        faces:{face_str}");
    }

    // Non-default widths (cell.width != 1).
    let mut nondefault: Vec<(u16, u16, u8)> = Vec::new();
    for (y, widths) in row_widths.iter().enumerate() {
        for (x, &w) in widths.iter().enumerate() {
            if w != 1 {
                nondefault.push((x as u16, y as u16, w));
            }
        }
    }
    if !nondefault.is_empty() {
        let _ = writeln!(output, "\n# non-default widths");
        for (x, y, w) in nondefault {
            let _ = writeln!(output, "({x}, {y}): width={w}");
        }
    }

    output
}

/// Append a deterministic dump of [`RenderResult`] cursor fields to a
/// CellGrid snapshot. The TUI backend doesn't write the cursor cell into
/// the grid (the terminal draws it via SGR positioning), so cursor
/// coordinates / style / colour are only visible through `RenderResult`.
/// Including them in the snapshot pins display-column → screen-coordinate
/// translation for cursor placement under CJK / EOL / line-change scenarios.
fn append_cursor_dump(out: &mut String, result: &RenderResult) {
    use std::fmt::Write;
    let _ = writeln!(out, "\n# cursor");
    let _ = writeln!(out, "position: ({}, {})", result.cursor_x, result.cursor_y);
    let _ = writeln!(out, "style:    {:?}", result.cursor_style);
    let _ = writeln!(out, "color:    {:?}", result.cursor_color);
}

/// Assert that `grid`'s dump matches the committed snapshot at
/// `tests/golden/snapshots/<name>.snap.txt`. On first run (no file) or
/// when `KASANE_GOLDEN_UPDATE=1`, write the snapshot and pass.
pub fn assert_grid_snapshot(grid: &CellGrid, name: &str) {
    let dir = snapshots_dir();
    std::fs::create_dir_all(&dir).expect("create snapshots dir");
    let path = dir.join(format!("{name}.snap.txt"));
    let actual = dump_cellgrid(grid);

    if update_mode() || !path.exists() {
        std::fs::write(&path, &actual).expect("write snapshot");
        eprintln!("golden grid wrote snapshot: {}", path.display());
        return;
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("load snapshot {}: {e}", path.display()));

    if actual != expected {
        // Show a compact diff: locate the first differing line.
        let actual_lines: Vec<&str> = actual.lines().collect();
        let expected_lines: Vec<&str> = expected.lines().collect();
        let max = actual_lines.len().max(expected_lines.len());
        let mut first_diff_line: Option<usize> = None;
        for i in 0..max {
            let a = actual_lines.get(i).copied().unwrap_or("");
            let e = expected_lines.get(i).copied().unwrap_or("");
            if a != e {
                first_diff_line = Some(i);
                break;
            }
        }
        let detail = match first_diff_line {
            Some(line_no) => {
                let context_before = line_no.saturating_sub(2);
                let context_after = (line_no + 3).min(max);
                let mut detail = format!("first diff at line {}:\n", line_no + 1);
                for i in context_before..context_after {
                    let a = actual_lines.get(i).copied().unwrap_or("<eof>");
                    let e = expected_lines.get(i).copied().unwrap_or("<eof>");
                    let mark = if a == e { " " } else { ">" };
                    detail.push_str(&format!(
                        "  {mark} L{}\n      actual:   {a}\n      expected: {e}\n",
                        i + 1
                    ));
                }
                detail
            }
            None => "(no line-level diff found; check trailing whitespace)".to_string(),
        };

        panic!(
            "golden snapshot mismatch for {name}\n{detail}\n\
             update with: KASANE_GOLDEN_UPDATE=1 cargo test -p kasane-core --test golden_grid"
        );
    }
}

/// Like [`assert_grid_snapshot`] but extends the dump with cursor metadata
/// from [`RenderResult`]. Required for cursor-positioning goldens because
/// the TUI cursor cell isn't written into [`CellGrid`] — the terminal
/// draws it through SGR positioning.
pub fn assert_cursor_grid_snapshot(grid: &CellGrid, result: &RenderResult, name: &str) {
    let dir = snapshots_dir();
    std::fs::create_dir_all(&dir).expect("create snapshots dir");
    let path = dir.join(format!("{name}.snap.txt"));
    let mut actual = dump_cellgrid(grid);
    append_cursor_dump(&mut actual, result);

    if update_mode() || !path.exists() {
        std::fs::write(&path, &actual).expect("write snapshot");
        eprintln!("golden grid wrote snapshot: {}", path.display());
        return;
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("load snapshot {}: {e}", path.display()));

    if actual != expected {
        let actual_lines: Vec<&str> = actual.lines().collect();
        let expected_lines: Vec<&str> = expected.lines().collect();
        let max = actual_lines.len().max(expected_lines.len());
        let mut first_diff_line: Option<usize> = None;
        for i in 0..max {
            let a = actual_lines.get(i).copied().unwrap_or("");
            let e = expected_lines.get(i).copied().unwrap_or("");
            if a != e {
                first_diff_line = Some(i);
                break;
            }
        }
        let detail = match first_diff_line {
            Some(line_no) => {
                let context_before = line_no.saturating_sub(2);
                let context_after = (line_no + 3).min(max);
                let mut detail = format!("first diff at line {}:\n", line_no + 1);
                for i in context_before..context_after {
                    let a = actual_lines.get(i).copied().unwrap_or("<eof>");
                    let e = expected_lines.get(i).copied().unwrap_or("<eof>");
                    let mark = if a == e { " " } else { ">" };
                    detail.push_str(&format!(
                        "  {mark} L{}\n      actual:   {a}\n      expected: {e}\n",
                        i + 1
                    ));
                }
                detail
            }
            None => "(no line-level diff found; check trailing whitespace)".to_string(),
        };

        panic!(
            "golden snapshot mismatch for {name}\n{detail}\n\
             update with: KASANE_GOLDEN_UPDATE=1 cargo test -p kasane-core --test golden_grid"
        );
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Build a WireFace with the given fg colour and bold attribute.
fn red_bold() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Red),
        bg: Color::Default,
        underline: Color::Default,
        attributes: Attributes::BOLD,
    }
}

fn cyan_underline() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Default,
        underline: Color::Named(NamedColor::Cyan),
        attributes: Attributes::UNDERLINE,
    }
}

/// Construct an 80×24 buffer scene with multi-style atoms exercising the
/// resolution path that B-wide will rewire. Lines mimic a small Rust
/// source file fragment with keyword highlighting and an underlined
/// macro identifier.
fn buffer_scene() -> Vec<Vec<Atom>> {
    let kw = red_bold();
    let macro_face = cyan_underline();
    vec![
        vec![
            Atom::with_style("fn", Style::from_face(&kw)),
            Atom::plain(" main() {"),
        ],
        vec![Atom::plain("    let x = 42;")],
        vec![],
        vec![
            Atom::plain("    "),
            Atom::with_style("println!", Style::from_face(&macro_face)),
            Atom::plain("(\"hello\");"),
        ],
        vec![Atom::plain("}")],
    ]
}

/// Construct a CJK-mixed scene. Width=2 East Asian Wide cells must appear in
/// the snapshot's "non-default widths" section; any regression in display
/// column accounting (e.g. `cursor_pos.column` interpreted as bytes) shows
/// up as a per-row text shift or a missing wide-cell entry.
fn buffer_scene_cjk() -> Vec<Vec<Atom>> {
    let kw = red_bold();
    let macro_face = cyan_underline();
    vec![
        vec![
            Atom::with_style("fn", Style::from_face(&kw)),
            Atom::plain(" メイン() {"),
        ],
        vec![Atom::plain("    let 挨拶 = \"こんにちは\";")],
        vec![],
        vec![
            Atom::plain("    "),
            Atom::with_style("println!", Style::from_face(&macro_face)),
            Atom::plain("(\"{}\", 挨拶);"),
        ],
        vec![Atom::plain("}")],
    ]
}

/// Construct a scene with a combining-mark sequence. The voiced sound mark
/// (U+3099) attaches to the preceding base; the snapshot should keep them
/// in a single cell with the canonical decomposed pair preserved verbatim.
fn buffer_scene_combining() -> Vec<Vec<Atom>> {
    vec![
        vec![Atom::plain("base: か + ゙ = が")],
        vec![Atom::plain("decomp: \u{304B}\u{3099}")],
        vec![Atom::plain("nfc:    \u{304C}")],
    ]
}

/// Plain (non-ZWJ, non-VS) emoji scene. Pins the East Asian Wide width=2
/// classification for typical SMP emoji codepoints. Sequences that need
/// font shaping to determine width (variation selectors, ZWJ joiners,
/// regional indicators) are intentionally omitted — they are deferred
/// until a consumer demands them, since their width depends on the
/// terminal's Unicode-width interpretation.
fn buffer_scene_emoji_basic() -> Vec<Vec<Atom>> {
    vec![
        vec![Atom::plain("hello 🌍 world")],
        vec![Atom::plain("👋 こんにちは 🌸")],
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Smoke baseline: an 80×24 ASCII scene with a few styled atoms,
/// rendered through the full kasane-core pipeline. This is the gate
/// for ADR-031 Phase A B-wide regressions.
#[test]
fn ascii_80x24_smoke() {
    let mut state = test_state_80x24();
    let _ = state.apply(KakouneRequest::Draw {
        lines: buffer_scene(),
        cursor_pos: Coord { line: 0, column: 0 },
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        widget_columns: 0,
    });
    let _ = state.apply(KakouneRequest::DrawStatus {
        prompt: vec![],
        content: vec![Atom::plain(" main.rs ")],
        content_cursor_pos: -1,
        mode_line: vec![Atom::plain("normal")],
        default_style: kasane_core::protocol::default_unresolved_style(),
        style: StatusStyle::Status,
    });

    let registry = kasane_core::plugin::PluginRuntime::default();
    let grid = render_to_grid(&state, &registry);

    assert_grid_snapshot(&grid, "ascii_80x24_smoke");
}

/// CJK + ASCII mixed scene — full-width Japanese identifiers and string
/// literals share lines with ASCII syntax. Pins display column accounting
/// for the East Asian Wide path; a regression that interprets
/// `cursor_pos.column` as a byte offset would surface as a row text mismatch.
#[test]
fn cjk_80x24_smoke() {
    let mut state = test_state_80x24();
    let _ = state.apply(KakouneRequest::Draw {
        lines: buffer_scene_cjk(),
        cursor_pos: Coord { line: 0, column: 0 },
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        widget_columns: 0,
    });
    let _ = state.apply(KakouneRequest::DrawStatus {
        prompt: vec![],
        content: vec![Atom::plain(" cjk.rs ")],
        content_cursor_pos: -1,
        mode_line: vec![Atom::plain("normal")],
        default_style: kasane_core::protocol::default_unresolved_style(),
        style: StatusStyle::Status,
    });

    let registry = kasane_core::plugin::PluginRuntime::default();
    let grid = render_to_grid(&state, &registry);

    assert_grid_snapshot(&grid, "cjk_80x24_smoke");
}

// ---------------------------------------------------------------------------
// Cursor positioning tests
// ---------------------------------------------------------------------------
//
// These exercise the display-column → screen-coordinate translation that
// `cursor_position` performs for the TUI / GUI backends. The cursor cell
// itself isn't written into [`CellGrid`] — the terminal draws it via SGR
// positioning — so coverage flows through [`RenderResult::cursor_x/_y`]
// captured by [`assert_cursor_grid_snapshot`].

/// Buffer with mixed ASCII + CJK content for cursor placement tests.
/// Row 0 has wide cells starting at display column 3 (`メ`); row 1 has
/// a CJK identifier embedded after the `let` keyword. Same content as
/// `buffer_scene_cjk` but kept local so cursor tests stay self-contained
/// against future fixture refactors.
fn buffer_scene_for_cursor() -> Vec<Vec<Atom>> {
    vec![
        vec![Atom::plain("fn メイン() {")],
        vec![Atom::plain("    let x = 42;")],
        vec![Atom::plain("}")],
    ]
}

/// Helper that runs the standard 80×24 pipeline for the cursor fixtures.
fn render_cursor_scene(cursor_pos: Coord) -> (CellGrid, RenderResult) {
    let mut state = test_state_80x24();
    let _ = state.apply(KakouneRequest::Draw {
        lines: buffer_scene_for_cursor(),
        cursor_pos,
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        widget_columns: 0,
    });
    let _ = state.apply(KakouneRequest::DrawStatus {
        prompt: vec![],
        content: vec![Atom::plain(" cursor.rs ")],
        content_cursor_pos: -1,
        mode_line: vec![Atom::plain("normal")],
        default_style: kasane_core::protocol::default_unresolved_style(),
        style: StatusStyle::Status,
    });

    let registry = kasane_core::plugin::PluginRuntime::default();
    render_to_grid_with_result(&state, &registry)
}

/// Cursor at the buffer origin (line 0, column 0). Pins the screen
/// coordinate baseline: cursor_x must be 0 (or the gutter offset when
/// gutters are enabled), cursor_y must be 0.
#[test]
fn cursor_at_origin() {
    let (grid, result) = render_cursor_scene(Coord { line: 0, column: 0 });
    assert_cursor_grid_snapshot(&grid, &result, "cursor_at_origin");
}

/// Cursor inside the CJK identifier `メイン` at display column 6
/// (the start of `イ`). A regression that interprets `cursor_pos.column`
/// as a byte offset would land at column 4 (after `fn ` byte-wise points
/// into the middle of `メ`'s UTF-8 sequence) instead of column 6 — see
/// project memory `project_cursor_pos_display_column.md`.
#[test]
fn cursor_in_cjk_display_column() {
    let (grid, result) = render_cursor_scene(Coord { line: 0, column: 6 });
    assert_cursor_grid_snapshot(&grid, &result, "cursor_in_cjk_display_column");
}

/// Cursor at end of line 1 — display column 15 lands on the trailing `;`
/// of `    let x = 42;`. Pins that EOL cursor placement maps to the
/// last cell of content, not past the buffer edge.
#[test]
fn cursor_at_eol() {
    let (grid, result) = render_cursor_scene(Coord {
        line: 1,
        column: 15,
    });
    assert_cursor_grid_snapshot(&grid, &result, "cursor_at_eol");
}

/// Cursor positioned past the rendered buffer (line 100). Pins the
/// out-of-viewport behaviour: `cursor_position` clamps via
/// `buffer_line_to_screen_y`, so cursor_y should saturate or land at a
/// well-defined value rather than wrap or panic.
#[test]
fn cursor_past_viewport() {
    let (grid, result) = render_cursor_scene(Coord {
        line: 100,
        column: 0,
    });
    assert_cursor_grid_snapshot(&grid, &result, "cursor_past_viewport");
}

/// Combining-mark sequence — kana base + voiced sound mark. The dakuten
/// path (U+304B U+3099) must group into a single grapheme cell rather than
/// laying out as two separate cells. Decomposed and pre-composed forms
/// (U+304C) appear on adjacent rows so the snapshot pins both paths.
#[test]
fn cjk_combining_marks() {
    let mut state = test_state_80x24();
    let _ = state.apply(KakouneRequest::Draw {
        lines: buffer_scene_combining(),
        cursor_pos: Coord { line: 0, column: 0 },
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        widget_columns: 0,
    });
    let _ = state.apply(KakouneRequest::DrawStatus {
        prompt: vec![],
        content: vec![Atom::plain(" combining.rs ")],
        content_cursor_pos: -1,
        mode_line: vec![Atom::plain("normal")],
        default_style: kasane_core::protocol::default_unresolved_style(),
        style: StatusStyle::Status,
    });

    let registry = kasane_core::plugin::PluginRuntime::default();
    let grid = render_to_grid(&state, &registry);

    assert_grid_snapshot(&grid, "cjk_combining_marks");
}

/// Basic emoji scene (no variation selectors, no ZWJ). Pins the standard
/// SMP wide-cell classification: each emoji codepoint must occupy two
/// display columns with the trailing column reporting width=0. A
/// regression in the East Asian Wide path or a `unicode-width` crate
/// upgrade that re-classifies emoji surfaces here.
#[test]
fn cjk_emoji_basic() {
    let mut state = test_state_80x24();
    let _ = state.apply(KakouneRequest::Draw {
        lines: buffer_scene_emoji_basic(),
        cursor_pos: Coord { line: 0, column: 0 },
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                underline: Color::Default,
                attributes: Attributes::empty(),
            },
        )),
        widget_columns: 0,
    });
    let _ = state.apply(KakouneRequest::DrawStatus {
        prompt: vec![],
        content: vec![Atom::plain(" emoji.rs ")],
        content_cursor_pos: -1,
        mode_line: vec![Atom::plain("normal")],
        default_style: kasane_core::protocol::default_unresolved_style(),
        style: StatusStyle::Status,
    });

    let registry = kasane_core::plugin::PluginRuntime::default();
    let grid = render_to_grid(&state, &registry);

    assert_grid_snapshot(&grid, "cjk_emoji_basic");
}
