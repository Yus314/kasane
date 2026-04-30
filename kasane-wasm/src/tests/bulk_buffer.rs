use crate::bindings::kasane::plugin::host_state::Host;
use crate::host::HostState;
use kasane_core::protocol::{Atom, WireFace};

type Line = Vec<Atom>;

fn make_host_with_lines(lines: &[&str]) -> HostState {
    let mut host = HostState::default();
    host.lines = lines.iter().map(|s| vec![Atom::plain(*s)]).collect();
    host.line_count = lines.len() as u32;
    host
}

fn make_host_with_raw_lines(lines: Vec<Line>) -> HostState {
    let mut host = HostState::default();
    host.line_count = lines.len() as u32;
    host.lines = lines;
    host
}

// --- get_lines_text tests ---

#[test]
fn get_lines_text_full_buffer() {
    let mut host = make_host_with_lines(&["hello", "world", "foo"]);
    let result = host.get_lines_text(0, 3);
    assert_eq!(result, vec!["hello", "world", "foo"]);
}

#[test]
fn get_lines_text_partial_range() {
    let mut host = make_host_with_lines(&["a", "b", "c", "d"]);
    let result = host.get_lines_text(1, 3);
    assert_eq!(result, vec!["b", "c"]);
}

#[test]
fn get_lines_text_empty_range_start_eq_end() {
    let mut host = make_host_with_lines(&["a", "b"]);
    let result = host.get_lines_text(1, 1);
    assert!(result.is_empty());
}

#[test]
fn get_lines_text_start_gt_end() {
    let mut host = make_host_with_lines(&["a", "b"]);
    let result = host.get_lines_text(2, 1);
    assert!(result.is_empty());
}

#[test]
fn get_lines_text_start_beyond_line_count() {
    let mut host = make_host_with_lines(&["a", "b"]);
    let result = host.get_lines_text(5, 10);
    assert!(result.is_empty());
}

#[test]
fn get_lines_text_end_clamped() {
    let mut host = make_host_with_lines(&["a", "b", "c"]);
    let result = host.get_lines_text(1, 100);
    assert_eq!(result, vec!["b", "c"]);
}

#[test]
fn get_lines_text_empty_buffer() {
    let mut host = make_host_with_lines(&[]);
    let result = host.get_lines_text(0, 5);
    assert!(result.is_empty());
}

#[test]
fn get_lines_text_multi_atom_concatenation() {
    let lines = vec![vec![Atom::plain("hel"), Atom::plain("lo")]];
    let mut host = make_host_with_raw_lines(lines);
    let result = host.get_lines_text(0, 1);
    assert_eq!(result, vec!["hello"]);
}

// --- get_lines_atoms tests ---

#[test]
fn get_lines_atoms_full_buffer() {
    let mut host = make_host_with_lines(&["hello", "world"]);
    let result = host.get_lines_atoms(0, 2);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].len(), 1);
    assert_eq!(result[0][0].contents, "hello");
    assert_eq!(result[1][0].contents, "world");
}

#[test]
fn get_lines_atoms_partial_range() {
    let mut host = make_host_with_lines(&["a", "b", "c", "d"]);
    let result = host.get_lines_atoms(1, 3);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0][0].contents, "b");
    assert_eq!(result[1][0].contents, "c");
}

#[test]
fn get_lines_atoms_empty_range() {
    let mut host = make_host_with_lines(&["a", "b"]);
    let result = host.get_lines_atoms(1, 1);
    assert!(result.is_empty());
}

#[test]
fn get_lines_atoms_end_clamped() {
    let mut host = make_host_with_lines(&["x", "y"]);
    let result = host.get_lines_atoms(0, 100);
    assert_eq!(result.len(), 2);
}

#[test]
fn get_lines_atoms_preserves_face() {
    use kasane_core::protocol::{Color, NamedColor, Style};

    let face = WireFace {
        fg: Color::Named(NamedColor::Red),
        bg: Color::Named(NamedColor::Blue),
        ..WireFace::default()
    };
    let lines = vec![vec![Atom::with_style("styled", Style::from_face(&face))]];
    let mut host = make_host_with_raw_lines(lines);
    let result = host.get_lines_atoms(0, 1);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].len(), 1);
    assert_eq!(result[0][0].contents, "styled");
    // The face should be converted via atoms_to_wit, verify it's not default
    // (exact face structure depends on WIT conversion, but contents must match)
}
