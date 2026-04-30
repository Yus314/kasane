use unicode_width::UnicodeWidthStr;

use crate::element::{
    Element, ElementStyle, FlexChild, GridColumn, Overlay, OverlayAnchor, StyleToken,
};
use crate::layout::{MenuPlacement, layout_menu_inline, line_display_width};
use crate::plugin::{AppView, PluginView};
use crate::protocol::resolve_style;
use crate::protocol::{Atom, MenuStyle, Style};
use crate::render::builders::{
    self, MAX_DROPDOWN_HEIGHT, PREFIX_WIDTH, SCROLLBAR_WIDTH, SUFFIX_RESERVE,
};
use crate::state::{AppState, MenuColumns, MenuState};

use super::build_styled_line_with_base;

/// Resolve menu item style: theme override takes precedence, protocol style as fallback.
fn resolve_menu_style(menu: &MenuState, selected: bool, state: &AppState) -> Style {
    if selected {
        state.config.theme.resolve_with_protocol_fallback(
            &StyleToken::MENU_ITEM_SELECTED,
            menu.selected_item_face.clone(),
        )
    } else {
        state
            .config
            .theme
            .resolve_with_protocol_fallback(&StyleToken::MENU_ITEM_NORMAL, menu.menu_face.clone())
    }
}

#[crate::kasane_component]
pub fn build_menu_overlay(
    menu: &MenuState,
    state: &AppState,
    registry: &PluginView<'_>,
) -> Option<Overlay> {
    if menu.items.is_empty() || menu.win_height == 0 {
        return None;
    }

    match menu.style {
        MenuStyle::Inline => build_menu_inline(menu, state, registry),
        MenuStyle::Prompt => build_menu_prompt(menu, state, registry),
        MenuStyle::Search => {
            if state.config.search_dropdown {
                build_menu_search_dropdown(menu, state, registry)
            } else {
                build_menu_search(menu, state, registry)
            }
        }
    }
}

/// Convert AppState menu_position config to layout MenuPlacement.
fn menu_placement(state: &AppState) -> MenuPlacement {
    MenuPlacement::from(state.config.menu_position)
}

/// Build a single menu item element: face selection + styled line + container wrap.
fn build_menu_item_element(
    menu: &MenuState,
    item_idx: usize,
    width: u16,
    registry: &PluginView<'_>,
    state: &AppState,
) -> Element {
    let selected = item_idx < menu.items.len() && Some(item_idx) == menu.selected;
    let style = resolve_menu_style(menu, selected, state);
    let item = if item_idx < menu.items.len() {
        let atoms = &menu.items[item_idx];
        let transformed =
            registry.transform_menu_item(atoms, item_idx, selected, &AppView::new(state));
        let line = transformed.as_ref().unwrap_or(atoms);
        build_styled_line_with_base(line, &style, width)
    } else {
        Element::text_with_style("", style.clone())
    };
    Element::container(item, ElementStyle::from(style))
}

use builders::truncate_atoms;

/// Build a two-column menu item element: candidate | gap | docstring.
///
/// Produces a single `Element::StyledLine` (flat, no Grid/Flex nesting per item).
fn build_split_item_element(
    menu: &MenuState,
    columns: &MenuColumns,
    item_idx: usize,
    candidate_col_w: u16,
    _content_w: u16,
    registry: &PluginView<'_>,
    state: &AppState,
) -> Element {
    let selected = item_idx < menu.items.len() && Some(item_idx) == menu.selected;
    let style = resolve_menu_style(menu, selected, state);

    if item_idx >= menu.items.len() {
        return Element::container(
            Element::text_with_style("", style.clone()),
            ElementStyle::from(style),
        );
    }

    let item = &menu.items[item_idx];
    let transformed = registry.transform_menu_item(item, item_idx, selected, &AppView::new(state));
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
    let mut cand_resolved = truncate_atoms(
        cand_atoms,
        candidate_col_w,
        &style,
        &state.config.truncation_char,
    );
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
        cand_resolved.push(Atom::with_style(" ".repeat(pad), style.clone()));
    }
    atoms.extend(cand_resolved);

    // 2. Gap: 1-space separator
    atoms.push(Atom::with_style(" ", style.clone()));

    // 3. Docstring portion: resolve styles (paint-level truncation handles overflow)
    for atom in &effective_item[split.docstring_start..] {
        atoms.push(Atom::with_style(
            atom.contents.clone(),
            resolve_style(&atom.style, &style),
        ));
    }

    Element::container(Element::StyledLine(atoms), ElementStyle::from(style))
}

fn build_menu_inline(
    menu: &MenuState,
    state: &AppState,
    registry: &PluginView<'_>,
) -> Option<Overlay> {
    let win_w = (menu.effective_content_width(state.runtime.cols) + SCROLLBAR_WIDTH)
        .min(state.runtime.cols);
    let content_w = win_w.saturating_sub(SCROLLBAR_WIDTH);
    let screen_h = state.available_height();
    let placement = menu_placement(state);

    let win = layout_menu_inline(
        &menu.anchor,
        win_w,
        menu.win_height,
        state.runtime.cols,
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
        .map(|mc| mc.max_candidate_width.min(state.runtime.cols * 2 / 5));

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
        &state.config.scrollbar_thumb,
        &state.config.scrollbar_track,
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
    registry: &PluginView<'_>,
) -> Option<Overlay> {
    if menu.columns == 0 {
        return None;
    }

    let status_row = state.available_height();
    let wh = menu.win_height;
    let columns = menu.columns as usize;
    let stride = wh as usize;
    let col_w = (state.runtime.cols.saturating_sub(1) as usize / columns).max(1);
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
        &state.config.scrollbar_thumb,
        &state.config.scrollbar_track,
    );
    let content = Element::grid(grid_columns, grid_children);
    let row = Element::row(vec![
        FlexChild::flexible(content, 1.0),
        FlexChild::fixed(scrollbar),
    ]);

    Some(Overlay {
        element: Element::container(
            row,
            ElementStyle::from(resolve_menu_style(menu, false, state)),
        ),
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y: start_y,
            w: state.runtime.cols,
            h: wh,
        },
    })
}

fn build_menu_search(
    menu: &MenuState,
    state: &AppState,
    _registry: &PluginView<'_>,
) -> Option<Overlay> {
    let status_row = state.available_height();
    let y = status_row.saturating_sub(1);
    let screen_w = state.runtime.cols as usize;
    let first = menu.first_item;
    let has_prefix = first > 0;
    let normal_style = resolve_menu_style(menu, false, state);

    let mut atoms: Vec<Atom> = Vec::new();

    // "< " prefix
    if has_prefix {
        atoms.push(Atom::with_style("< ", normal_style.clone()));
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
                    atoms.push(Atom::with_style(" ".repeat(pad_len), normal_style.clone()));
                }
                atoms.push(Atom::with_style(">", normal_style.clone()));
            }
            break;
        }

        let item_style = resolve_menu_style(menu, Some(idx) == menu.selected, state);

        // Add item atoms with resolved style
        for atom in &menu.items[idx] {
            atoms.push(Atom::with_style(atom.contents.clone(), item_style.clone()));
        }
        x += item_w;

        // Gap
        if x < screen_w {
            atoms.push(Atom::with_style(" ", normal_style.clone()));
            x += 1;
        }
    }

    let element = Element::container(Element::StyledLine(atoms), ElementStyle::from(normal_style));

    Some(Overlay {
        element,
        anchor: OverlayAnchor::Absolute {
            x: 0,
            y,
            w: state.runtime.cols,
            h: 1,
        },
    })
}

/// Build a search menu as a vertical dropdown instead of the default inline bar.
fn build_menu_search_dropdown(
    menu: &MenuState,
    state: &AppState,
    registry: &PluginView<'_>,
) -> Option<Overlay> {
    let screen_h = state.available_height();
    let status_row = state.available_height();
    let max_h = MAX_DROPDOWN_HEIGHT.min(screen_h.saturating_sub(1));
    let win_h = (menu.items.len() as u16).min(max_h).max(1);
    let win_w = (menu.max_item_width + SCROLLBAR_WIDTH).min(state.runtime.cols);
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
        &state.config.scrollbar_thumb,
        &state.config.scrollbar_track,
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
    style: &Style,
    thumb: &str,
    track: &str,
) -> Element {
    builders::build_scrollbar(
        win_height,
        menu.items.len(),
        menu.columns,
        menu.first_item,
        style,
        thumb,
        track,
    )
}

// ---------------------------------------------------------------------------
// BuiltinMenuPlugin — lowest-priority MENU_RENDERER
// ---------------------------------------------------------------------------

use crate::plugin::{FrameworkAccess, PluginBackend, PluginCapabilities, PluginId};

/// Built-in plugin for menu overlay rendering.
///
/// Delegates to [`build_menu_overlay`] for all `MenuStyle` variants.
/// Registered as the lowest-priority `MENU_RENDERER` so that user plugins
/// with the same capability take precedence.
pub struct BuiltinMenuPlugin;

impl PluginBackend for BuiltinMenuPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.menu".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::MENU_RENDERER
    }

    fn render_menu_overlay(
        &self,
        state: &AppView<'_>,
        view: &PluginView<'_>,
    ) -> Option<crate::element::Overlay> {
        let app_state = state.as_app_state();
        let menu = app_state.observed.menu.as_ref()?;
        build_menu_overlay(menu, app_state, view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginRuntime;
    use crate::protocol::{Coord, NamedColor};
    use crate::state::MenuParams;

    fn make_completion_item(candidate: &str, padding: &str, docstring: &str) -> Vec<Atom> {
        vec![
            Atom::plain(candidate),
            Atom::plain(padding),
            Atom::with_style(
                docstring,
                Style {
                    fg: crate::protocol::Brush::Named(NamedColor::Cyan),
                    ..Style::default()
                },
            ),
        ]
    }

    #[test]
    fn test_truncate_atoms_no_op() {
        let atoms = vec![Atom::plain("hello")];
        let result = truncate_atoms(&atoms, 10, &Style::default(), "\u{2026}");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contents.as_str(), "hello");
    }

    #[test]
    fn test_truncate_atoms_with_ellipsis() {
        let atoms = vec![Atom::plain("hello_world_long")];
        let result = truncate_atoms(&atoms, 8, &Style::default(), "\u{2026}");
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
        let atoms = vec![Atom::plain("あいう")];
        let result = truncate_atoms(&atoms, 5, &Style::default(), "\u{2026}");
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
                selected_item_face: crate::protocol::Style::default(),
                menu_face: crate::protocol::Style::default(),
                style: MenuStyle::Inline,
                screen_w: 80,
                screen_h: 24,
                max_height: 10,
            },
        );
        let columns = menu.columns_split.as_ref().unwrap();
        let cand_w = columns.max_candidate_width.min(80 * 2 / 5);

        let registry = PluginRuntime::new();
        let state = AppState::default();
        let element =
            build_split_item_element(&menu, columns, 0, cand_w, 20, &registry.view(), &state);
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
            selected_item_style: crate::protocol::default_unresolved_style(),
            menu_style: crate::protocol::default_unresolved_style(),
            style: MenuStyle::Inline,
        });

        let menu = state.observed.menu.as_ref().unwrap();
        assert!(menu.columns_split.is_some());
        let registry = PluginRuntime::new();
        let overlay = build_menu_inline(menu, &state, &registry.view());
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
