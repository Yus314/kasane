use unicode_width::UnicodeWidthStr;

use crate::element::{Element, FlexChild, GridColumn, Overlay, OverlayAnchor, Style};
use crate::layout::{MenuPlacement, layout_menu_inline, line_display_width};
use crate::plugin::PluginRegistry;
use crate::protocol::resolve_face;
use crate::protocol::{Atom, Face, MenuStyle};
use crate::state::{AppState, MenuColumns, MenuState};

use super::build_styled_line_with_base;

/// Width of the scrollbar column (1 cell).
const SCROLLBAR_WIDTH: u16 = 1;

/// Width of the "< " prefix indicator in prompt-style menus.
const PREFIX_WIDTH: usize = 2;

/// Width reserved for the " >" suffix indicator in prompt-style menus.
const SUFFIX_RESERVE: usize = 2;

/// Maximum height for the search dropdown menu.
const MAX_DROPDOWN_HEIGHT: u16 = 10;

#[crate::kasane_component(deps(MENU_STRUCTURE, MENU_SELECTION, OPTIONS))]
pub(crate) fn build_menu_overlay(
    menu: &MenuState,
    state: &AppState,
    registry: &PluginRegistry,
) -> Option<Overlay> {
    if menu.items.is_empty() || menu.win_height == 0 {
        return None;
    }

    match menu.style {
        MenuStyle::Inline => build_menu_inline(menu, state, registry),
        MenuStyle::Prompt => build_menu_prompt(menu, state, registry),
        MenuStyle::Search => {
            if state.search_dropdown {
                build_menu_search_dropdown(menu, state, registry)
            } else {
                build_menu_search(menu, state, registry)
            }
        }
    }
}

/// Convert AppState menu_position config to layout MenuPlacement.
fn menu_placement(state: &AppState) -> MenuPlacement {
    MenuPlacement::from(state.menu_position)
}

/// Build a single menu item element: face selection + styled line + container wrap.
fn build_menu_item_element(
    menu: &MenuState,
    item_idx: usize,
    width: u16,
    registry: &PluginRegistry,
    state: &AppState,
) -> Element {
    let selected = item_idx < menu.items.len() && Some(item_idx) == menu.selected;
    let face = if selected {
        menu.selected_item_face
    } else {
        menu.menu_face
    };
    let item = if item_idx < menu.items.len() {
        let atoms = &menu.items[item_idx];
        let transformed = registry.transform_menu_item(atoms, item_idx, selected, state);
        let line = transformed.as_ref().unwrap_or(atoms);
        build_styled_line_with_base(line, &face, width)
    } else {
        Element::text("", face)
    };
    Element::container(item, Style::from(face))
}

/// Truncate a slice of atoms to fit within `max_width` display columns.
///
/// If the content fits, resolves faces against `base_face` and returns as-is.
/// If it exceeds, truncates at `max_width - 1` and appends "…" (U+2026, width 1).
fn truncate_atoms(atoms: &[Atom], max_width: u16, base_face: &Face) -> Vec<Atom> {
    let max_w = max_width as usize;
    let total: usize = atoms
        .iter()
        .map(|a| {
            a.contents
                .split(|c: char| c.is_control())
                .map(UnicodeWidthStr::width)
                .sum::<usize>()
        })
        .sum();

    if total <= max_w {
        return atoms
            .iter()
            .map(|a| Atom {
                face: resolve_face(&a.face, base_face),
                contents: a.contents.clone(),
            })
            .collect();
    }

    // Truncate at max_width - 1 to leave room for "…"
    let limit = max_w.saturating_sub(1);
    let mut result = Vec::new();
    let mut used = 0usize;
    for atom in atoms {
        let face = resolve_face(&atom.face, base_face);
        let mut buf = String::new();
        for ch in atom.contents.chars() {
            let cw = if ch.is_control() {
                0
            } else {
                UnicodeWidthStr::width(ch.encode_utf8(&mut [0; 4]) as &str)
            };
            if used + cw > limit {
                break;
            }
            buf.push(ch);
            used += cw;
        }
        if !buf.is_empty() {
            result.push(Atom {
                face,
                contents: buf.into(),
            });
        }
        if used >= limit {
            break;
        }
    }
    // Append ellipsis with the base face
    result.push(Atom {
        face: *base_face,
        contents: "\u{2026}".into(),
    });
    result
}

/// Build a two-column menu item element: candidate | gap | docstring.
///
/// Produces a single `Element::StyledLine` (flat, no Grid/Flex nesting per item).
fn build_split_item_element(
    menu: &MenuState,
    columns: &MenuColumns,
    item_idx: usize,
    candidate_col_w: u16,
    _content_w: u16,
    registry: &PluginRegistry,
    state: &AppState,
) -> Element {
    let selected = item_idx < menu.items.len() && Some(item_idx) == menu.selected;
    let face = if selected {
        menu.selected_item_face
    } else {
        menu.menu_face
    };

    if item_idx >= menu.items.len() {
        return Element::container(Element::text("", face), Style::from(face));
    }

    let item = &menu.items[item_idx];
    let transformed = registry.transform_menu_item(item, item_idx, selected, state);
    let (effective_item, effective_split);
    let split = if let Some(ref t) = transformed {
        // Re-split after transform (icon atoms shift indices)
        effective_item = t;
        effective_split = crate::state::split_single_item(t);
        &effective_split
    } else {
        effective_item = item;
        &columns.splits[item_idx]
    };
    let mut atoms: Vec<Atom> = Vec::new();

    // 1. Candidate portion: truncate if wider than candidate_col_w
    let cand_atoms = &effective_item[..split.candidate_end];
    let mut cand_resolved = truncate_atoms(cand_atoms, candidate_col_w, &face);
    // Pad candidate to candidate_col_w
    let cand_w: usize = cand_resolved
        .iter()
        .map(|a| {
            a.contents
                .split(|c: char| c.is_control())
                .map(UnicodeWidthStr::width)
                .sum::<usize>()
        })
        .sum();
    if (cand_w as u16) < candidate_col_w {
        let pad = candidate_col_w as usize - cand_w;
        cand_resolved.push(Atom {
            face,
            contents: " ".repeat(pad).into(),
        });
    }
    atoms.extend(cand_resolved);

    // 2. Gap: 1-space separator
    atoms.push(Atom {
        face,
        contents: " ".into(),
    });

    // 3. Docstring portion: resolve faces (paint-level truncation handles overflow)
    for atom in &effective_item[split.docstring_start..] {
        atoms.push(Atom {
            face: resolve_face(&atom.face, &face),
            contents: atom.contents.clone(),
        });
    }

    Element::container(Element::StyledLine(atoms), Style::from(face))
}

fn build_menu_inline(
    menu: &MenuState,
    state: &AppState,
    registry: &PluginRegistry,
) -> Option<Overlay> {
    let win_w = (menu.effective_content_width(state.cols) + SCROLLBAR_WIDTH).min(state.cols);
    let content_w = win_w.saturating_sub(SCROLLBAR_WIDTH);
    let screen_h = state.available_height();
    let placement = menu_placement(state);

    let win = layout_menu_inline(
        &menu.anchor,
        win_w,
        menu.win_height,
        state.cols,
        screen_h,
        placement,
    );
    if win.width == 0 || win.height == 0 {
        return None;
    }

    // Cap candidate column at 40% of screen width to leave room for docstrings.
    let candidate_col_w = menu
        .columns_split
        .as_ref()
        .map(|mc| mc.max_candidate_width.min(state.cols * 2 / 5));

    // Build item rows
    let item_rows: Vec<FlexChild> = (0..win.height)
        .map(|line| {
            let item_idx = menu.first_item + line as usize;
            let element = match (&menu.columns_split, candidate_col_w) {
                (Some(columns), Some(cw)) => build_split_item_element(
                    menu, columns, item_idx, cw, content_w, registry, state,
                ),
                _ => build_menu_item_element(menu, item_idx, content_w, registry, state),
            };
            FlexChild::fixed(element)
        })
        .collect();

    // Build scrollbar column
    let scrollbar = build_scrollbar(
        win.height,
        menu,
        &menu.menu_face,
        &state.scrollbar_thumb,
        &state.scrollbar_track,
    );

    let content_col = Element::column(item_rows);
    let row = Element::row(vec![
        FlexChild::flexible(content_col, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: row,
        anchor: win.into(),
    })
}

fn build_menu_prompt(
    menu: &MenuState,
    state: &AppState,
    registry: &PluginRegistry,
) -> Option<Overlay> {
    if menu.columns == 0 {
        return None;
    }

    let status_row = state.available_height();
    let wh = menu.win_height;
    let columns = menu.columns as usize;
    let stride = wh as usize;
    let col_w = (state.cols.saturating_sub(1) as usize / columns).max(1);
    let first_col = menu.first_item / stride;
    let start_y = status_row.saturating_sub(wh);

    // Build grid of items (row-major: iterate lines then columns)
    let grid_columns: Vec<GridColumn> = vec![GridColumn::flex(1.0); columns];
    let mut grid_children: Vec<Element> = Vec::with_capacity(wh as usize * columns);
    for line in 0..wh as usize {
        for col in 0..columns {
            let item_idx = (first_col + col) * stride + line;
            grid_children.push(build_menu_item_element(
                menu,
                item_idx,
                col_w as u16,
                registry,
                state,
            ));
        }
    }

    // Add scrollbar
    let scrollbar = build_scrollbar(
        wh,
        menu,
        &menu.menu_face,
        &state.scrollbar_thumb,
        &state.scrollbar_track,
    );
    let content = Element::grid(grid_columns, grid_children);
    let row = Element::row(vec![
        FlexChild::flexible(content, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: Element::container(row, Style::from(menu.menu_face)),
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y: start_y,
            w: state.cols,
            h: wh,
        },
    })
}

fn build_menu_search(
    menu: &MenuState,
    state: &AppState,
    _registry: &PluginRegistry,
) -> Option<Overlay> {
    let status_row = state.available_height();
    let y = status_row.saturating_sub(1);
    let screen_w = state.cols as usize;
    let first = menu.first_item;
    let has_prefix = first > 0;

    let mut atoms: Vec<Atom> = Vec::new();

    // "< " prefix
    if has_prefix {
        atoms.push(Atom {
            face: menu.menu_face,
            contents: "< ".into(),
        });
    }

    // Items with gaps
    let mut x = if has_prefix { PREFIX_WIDTH } else { 0 };
    for idx in first..menu.items.len() {
        let item_w = line_display_width(&menu.items[idx]);
        let has_more = idx + 1 < menu.items.len();
        let suffix_reserve = if has_more { SUFFIX_RESERVE } else { 0 };

        if x + item_w + suffix_reserve > screen_w && x > 0 {
            if has_more {
                // Pad and add ">"
                let pad_len = screen_w.saturating_sub(x + 1);
                if pad_len > 0 {
                    atoms.push(Atom {
                        face: menu.menu_face,
                        contents: " ".repeat(pad_len).into(),
                    });
                }
                atoms.push(Atom {
                    face: menu.menu_face,
                    contents: ">".into(),
                });
            }
            break;
        }

        let face = if Some(idx) == menu.selected {
            menu.selected_item_face
        } else {
            menu.menu_face
        };

        // Add item atoms with resolved face
        for atom in &menu.items[idx] {
            atoms.push(Atom {
                face,
                contents: atom.contents.clone(),
            });
        }
        x += item_w;

        // Gap
        if x < screen_w {
            atoms.push(Atom {
                face: menu.menu_face,
                contents: " ".into(),
            });
            x += 1;
        }
    }

    let element = Element::container(Element::StyledLine(atoms), Style::from(menu.menu_face));

    Some(Overlay {
        element,
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y,
            w: state.cols,
            h: 1,
        },
    })
}

/// Build a search menu as a vertical dropdown instead of the default inline bar.
fn build_menu_search_dropdown(
    menu: &MenuState,
    state: &AppState,
    registry: &PluginRegistry,
) -> Option<Overlay> {
    let screen_h = state.available_height();
    let status_row = state.available_height();
    let max_h = MAX_DROPDOWN_HEIGHT.min(screen_h.saturating_sub(1));
    let win_h = (menu.items.len() as u16).min(max_h).max(1);
    let win_w = (menu.max_item_width + SCROLLBAR_WIDTH).min(state.cols);
    let content_w = win_w.saturating_sub(SCROLLBAR_WIDTH);

    // Place above the status bar
    let y = status_row.saturating_sub(win_h);

    let item_rows: Vec<FlexChild> = (0..win_h)
        .map(|line| {
            let item_idx = menu.first_item + line as usize;
            FlexChild::fixed(build_menu_item_element(
                menu, item_idx, content_w, registry, state,
            ))
        })
        .collect();

    let scrollbar = build_scrollbar(
        win_h,
        menu,
        &menu.menu_face,
        &state.scrollbar_thumb,
        &state.scrollbar_track,
    );
    let content_col = Element::column(item_rows);
    let row = Element::row(vec![
        FlexChild::flexible(content_col, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: row,
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y,
            w: win_w,
            h: win_h,
        },
    })
}

fn build_scrollbar(
    win_height: u16,
    menu: &MenuState,
    face: &Face,
    thumb: &str,
    track: &str,
) -> Element {
    let wh = win_height as usize;
    let item_count = menu.items.len();
    let columns = menu.columns as usize;
    if wh == 0 || item_count == 0 {
        return Element::Empty;
    }

    let menu_lines = item_count.div_ceil(columns);
    let mark_h = (wh * wh).div_ceil(menu_lines).min(wh);
    let menu_cols = item_count.div_ceil(wh);
    let first_col = menu.first_item / wh;
    let denom = menu_cols.saturating_sub(columns).max(1);
    let mark_y = ((wh - mark_h) * first_col / denom).min(wh - mark_h);

    let mut rows: Vec<FlexChild> = Vec::new();
    for row in 0..wh {
        let ch = if row >= mark_y && row < mark_y + mark_h {
            thumb
        } else {
            track
        };
        rows.push(FlexChild::fixed(Element::text(ch, *face)));
    }

    Element::column(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Color, Coord, NamedColor};
    use crate::state::MenuParams;

    fn make_completion_item(candidate: &str, padding: &str, docstring: &str) -> Vec<Atom> {
        vec![
            Atom {
                face: Face::default(),
                contents: candidate.into(),
            },
            Atom {
                face: Face::default(),
                contents: padding.into(),
            },
            Atom {
                face: Face {
                    fg: Color::Named(NamedColor::Cyan),
                    ..Face::default()
                },
                contents: docstring.into(),
            },
        ]
    }

    #[test]
    fn test_truncate_atoms_no_op() {
        let atoms = vec![Atom {
            face: Face::default(),
            contents: "hello".into(),
        }];
        let result = truncate_atoms(&atoms, 10, &Face::default());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contents.as_str(), "hello");
    }

    #[test]
    fn test_truncate_atoms_with_ellipsis() {
        let atoms = vec![Atom {
            face: Face::default(),
            contents: "hello_world_long".into(),
        }];
        let result = truncate_atoms(&atoms, 8, &Face::default());
        // Should be truncated to 7 chars + "…"
        let last = result.last().unwrap();
        assert_eq!(last.contents.as_str(), "\u{2026}");
        let total_w: usize = result
            .iter()
            .map(|a| UnicodeWidthStr::width(a.contents.as_str()))
            .sum();
        assert_eq!(total_w, 8);
    }

    #[test]
    fn test_truncate_atoms_cjk() {
        // "あいう" = 3 CJK chars, each width 2 → total 6
        let atoms = vec![Atom {
            face: Face::default(),
            contents: "あいう".into(),
        }];
        let result = truncate_atoms(&atoms, 5, &Face::default());
        // Can fit "あい" (4) + "…" (1) = 5
        let total_w: usize = result
            .iter()
            .map(|a| UnicodeWidthStr::width(a.contents.as_str()))
            .sum();
        assert_eq!(total_w, 5);
        assert_eq!(result.last().unwrap().contents.as_str(), "\u{2026}");
    }

    #[test]
    fn test_build_split_item_element() {
        let items = vec![
            make_completion_item("foo", "   ", "{string}"),
            make_completion_item("barbaz", " ", "{int}"),
        ];
        let menu = MenuState::new(
            items,
            MenuParams {
                anchor: Coord { line: 5, column: 0 },
                selected_item_face: Face::default(),
                menu_face: Face::default(),
                style: MenuStyle::Inline,
                screen_w: 80,
                screen_h: 24,
                max_height: 10,
            },
        );
        let columns = menu.columns_split.as_ref().unwrap();
        let cand_w = columns.max_candidate_width.min(80 * 2 / 5);

        let registry = PluginRegistry::new();
        let state = AppState::default();
        let element = build_split_item_element(&menu, columns, 0, cand_w, 20, &registry, &state);
        // Should be a Container wrapping a StyledLine
        if let Element::Container { child, .. } = &element {
            if let Element::StyledLine(atoms) = child.as_ref() {
                // Should have: candidate atoms + pad + gap + docstring atoms
                assert!(
                    atoms.len() >= 3,
                    "expected at least 3 atoms, got {}",
                    atoms.len()
                );
                // Last atom should contain the docstring
                let last = &atoms[atoms.len() - 1];
                assert_eq!(last.contents.as_str(), "{string}");
            } else {
                panic!("expected StyledLine inside Container");
            }
        } else {
            panic!("expected Container element");
        }
    }

    #[test]
    fn test_build_menu_inline_two_column() {
        // Simulate real-world: a long candidate causes excessive padding on short ones.
        // "x"*40 (40) + " " (1) + "{int}" (5) → raw width 46
        // "foo"  (3)  + " "*38  + "{string}" (8) → raw width 49 (padded to align)
        // max_item_width = 49, effective = min(40,32) + 1 + 8 = 41
        let items = vec![
            make_completion_item("foo", &" ".repeat(38), "{string}"),
            make_completion_item(&"x".repeat(40), " ", "{int}"),
        ];
        let mut state = crate::render::test_helpers::test_state_80x24();
        state.apply(crate::protocol::KakouneRequest::MenuShow {
            items,
            anchor: Coord { line: 5, column: 0 },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
        });

        let menu = state.menu.as_ref().unwrap();
        assert!(menu.columns_split.is_some());
        let registry = PluginRegistry::new();
        let overlay = build_menu_inline(menu, &state, &registry);
        assert!(overlay.is_some());

        let o = overlay.unwrap();
        if let OverlayAnchor::Absolute { w, .. } = o.anchor {
            // Two-column width (42) should be less than raw single-column (50)
            assert!(
                w < menu.max_item_width + 1,
                "two-column menu should be narrower: w={w}, max_item_width+1={}",
                menu.max_item_width + 1
            );
        }
    }
}
