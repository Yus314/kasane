//! Property-based tests for DirtyFlags correctness.
//!
//! Verifies that `AppState::apply()` returns DirtyFlags consistent with
//! the fields actually mutated, using the `FIELD_DIRTY_MAP` generated
//! by `#[derive(DirtyTracked)]`.

use proptest::prelude::*;

use kasane_core::protocol::{
    Atom, Color, Coord, Face, KakouneRequest, MenuStyle, NamedColor, StatusStyle,
};
use kasane_core::state::{AppState, DirtyFlags};

/// Generate a random Face.
fn arb_face() -> impl Strategy<Value = Face> {
    (0u8..5, 0u8..5).prop_map(|(fg_idx, bg_idx)| {
        let fg = match fg_idx {
            0 => Color::Default,
            1 => Color::Named(NamedColor::Red),
            2 => Color::Named(NamedColor::Green),
            3 => Color::Named(NamedColor::Blue),
            _ => Color::Named(NamedColor::Yellow),
        };
        let bg = match bg_idx {
            0 => Color::Default,
            1 => Color::Named(NamedColor::Red),
            2 => Color::Named(NamedColor::Green),
            _ => Color::Default,
        };
        Face {
            fg,
            bg,
            ..Face::default()
        }
    })
}

/// Generate a random Coord within reasonable bounds.
fn arb_coord() -> impl Strategy<Value = Coord> {
    (0i32..100, 0i32..200).prop_map(|(line, column)| Coord { line, column })
}

/// Generate a random Line (vec of Atoms).
fn arb_line() -> impl Strategy<Value = Vec<Atom>> {
    prop::collection::vec(
        ("[a-z]{1,10}", arb_face())
            .prop_map(|(contents, face): (String, _)| Atom::from_face(face, contents)),
        1..5,
    )
}

/// Generate a random list of lines.
fn arb_lines() -> impl Strategy<Value = Vec<Vec<Atom>>> {
    prop::collection::vec(arb_line(), 1..30)
}

/// Generate lines together with a cursor position that is consistent with the
/// line content (satisfies the R-1 width invariant in `apply()`).
///
/// Since `arb_line()` generates only ASCII atoms (`[a-z]{1,10}`), the display
/// width of each atom equals `contents.len()`.
fn arb_lines_with_cursor() -> impl Strategy<Value = (Vec<Vec<Atom>>, Coord)> {
    arb_lines()
        .prop_flat_map(|lines| {
            let widths: Vec<u32> = lines
                .iter()
                .map(|line| line.iter().map(|a| a.contents.len() as u32).sum::<u32>())
                .collect();
            let n = lines.len();
            (Just(lines), Just(widths), 0..n)
        })
        .prop_flat_map(|(lines, widths, line_idx)| {
            let max_col = widths[line_idx];
            (Just(lines), Just(line_idx as i32), 0..=max_col)
        })
        .prop_map(|(lines, line, col)| {
            (
                lines,
                Coord {
                    line,
                    column: col as i32,
                },
            )
        })
}

// --- Tests for each apply() match arm ---

proptest! {
    /// Draw always returns BUFFER (BUFFER_CONTENT | BUFFER_CURSOR).
    #[test]
    fn draw_returns_buffer(
        (lines, cursor_pos) in arb_lines_with_cursor(),
        default_face in arb_face(),
        padding_face in arb_face(),
        widget_columns in 0u16..10,
    ) {
        let mut state = AppState::default();
        let flags = state.apply(KakouneRequest::Draw {
            lines,
            cursor_pos,
            default_face,
            padding_face,
            widget_columns,
        });
        // Draw always touches lines (BUFFER_CONTENT) and cursor_pos (BUFFER_CURSOR)
        prop_assert!(flags.contains(DirtyFlags::BUFFER_CONTENT));
        prop_assert!(flags.contains(DirtyFlags::BUFFER_CURSOR));
    }

    /// DrawStatus always returns at least STATUS.
    #[test]
    fn draw_status_returns_status(
        prompt in arb_line(),
        content in arb_line(),
        content_cursor_pos in -1i32..10,
        mode_line in arb_line(),
        default_face in arb_face(),
        style in prop_oneof![
            Just(StatusStyle::Status),
            Just(StatusStyle::Command),
            Just(StatusStyle::Search),
            Just(StatusStyle::Prompt),
        ],
    ) {
        let mut state = AppState::default();
        let flags = state.apply(KakouneRequest::DrawStatus {
            prompt,
            content,
            content_cursor_pos,
            mode_line,
            default_face,
            style,
        });
        prop_assert!(flags.contains(DirtyFlags::STATUS));
        // May also contain BUFFER_CURSOR if cursor mode changed
    }

    /// MenuShow always returns MENU_STRUCTURE.
    #[test]
    fn menu_show_returns_menu_structure(
        item_count in 1usize..20,
        anchor in arb_coord(),
        selected_face in arb_face(),
        menu_face in arb_face(),
    ) {
        let items: Vec<_> = (0..item_count)
            .map(|i| vec![Atom::from_face(Face::default(), format!("item{i}"))])
            .collect();
        let mut state = AppState::default();
        state.runtime.rows = 24;
        state.runtime.cols = 80;
        let flags = state.apply(KakouneRequest::MenuShow {
            items,
            anchor,
            selected_item_face: selected_face,
            menu_face,
            style: MenuStyle::Inline,
        });
        prop_assert!(flags.contains(DirtyFlags::MENU_STRUCTURE));
    }

    /// MenuSelect always returns MENU_SELECTION.
    #[test]
    fn menu_select_returns_selection(selected in -1i32..30) {
        let mut state = AppState::default();
        state.runtime.rows = 24;
        state.runtime.cols = 80;
        // First show a menu
        let items: Vec<_> = (0..10)
            .map(|i| vec![Atom::from_face(Face::default(), format!("item{i}"))])
            .collect();
        state.apply(KakouneRequest::MenuShow {
            items,
            anchor: Coord::default(),
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
        });
        let flags = state.apply(KakouneRequest::MenuSelect { selected });
        prop_assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    }

    /// InfoShow always returns INFO.
    #[test]
    fn info_show_returns_info(
        title in arb_line(),
        content in prop::collection::vec(arb_line(), 1..3),
        anchor in arb_coord(),
        face in arb_face(),
    ) {
        let mut state = AppState::default();
        let flags = state.apply(KakouneRequest::InfoShow {
            title,
            content,
            anchor,
            face,
            style: kasane_core::protocol::InfoStyle::Prompt,
        });
        prop_assert!(flags.contains(DirtyFlags::INFO));
    }
}

fn make_atom(s: &str) -> Atom {
    Atom::from_face(Face::default(), s)
}

/// MenuHide returns MENU | BUFFER_CONTENT.
#[test]
fn menu_hide_returns_menu_and_buffer() {
    let mut state = AppState::default();
    state.runtime.rows = 24;
    state.runtime.cols = 80;
    state.apply(KakouneRequest::MenuShow {
        items: vec![vec![make_atom("x")]],
        anchor: Coord::default(),
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    let flags = state.apply(KakouneRequest::MenuHide);
    assert!(flags.contains(DirtyFlags::MENU));
    assert!(flags.contains(DirtyFlags::BUFFER_CONTENT));
}

/// InfoHide returns INFO | BUFFER_CONTENT.
#[test]
fn info_hide_returns_info_and_buffer() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::InfoShow {
        title: vec![make_atom("t")],
        content: vec![vec![make_atom("c")]],
        anchor: Coord::default(),
        face: Face::default(),
        style: kasane_core::protocol::InfoStyle::Prompt,
    });
    let flags = state.apply(KakouneRequest::InfoHide);
    assert!(flags.contains(DirtyFlags::INFO));
    assert!(flags.contains(DirtyFlags::BUFFER_CONTENT));
}

/// SetUiOptions returns OPTIONS.
#[test]
fn set_ui_options_returns_options() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::SetUiOptions {
        options: [("key".into(), "val".into())].into_iter().collect(),
    });
    assert!(flags.contains(DirtyFlags::OPTIONS));
}

/// Refresh(force=true) returns ALL.
#[test]
fn refresh_force_returns_all() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::Refresh { force: true });
    assert_eq!(flags, DirtyFlags::ALL);
}

/// Refresh(force=false) returns BUFFER | STATUS.
#[test]
fn refresh_no_force_returns_buffer_status() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::Refresh { force: false });
    assert!(flags.contains(DirtyFlags::BUFFER));
    assert!(flags.contains(DirtyFlags::STATUS));
}
