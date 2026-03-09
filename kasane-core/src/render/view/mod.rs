mod info;
mod menu;
#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests;

use crate::element::{Element, FlexChild, Overlay, OverlayAnchor, Style};
use crate::layout::line_display_width;
use crate::plugin::{DecorateTarget, PluginRegistry, ReplaceTarget, Slot};
use crate::protocol::{Atom, Face, InfoStyle, Line, MenuStyle};
use crate::render::ViewCache;
use crate::state::AppState;

/// Build the full Element tree from application state (backward-compatible).
pub fn view(state: &AppState, registry: &PluginRegistry) -> Element {
    view_cached(state, registry, &mut ViewCache::new())
}

/// Build the full Element tree with subtree memoization via ViewCache.
pub fn view_cached(state: &AppState, registry: &PluginRegistry, cache: &mut ViewCache) -> Element {
    crate::perf::perf_span!("view");

    // Section 1: Base (buffer + status bar + plugin slots)
    let base = match cache.base {
        Some(ref cached) => cached.clone(),
        None => {
            let b = build_base(state, registry);
            cache.base = Some(b.clone());
            b
        }
    };

    // Collect overlays
    let mut overlays = Vec::new();

    // Section 2: Menu overlay
    let menu_overlay = match cache.menu_overlay {
        Some(ref cached) => cached.clone(),
        None => {
            let m = build_menu_section(state, registry);
            cache.menu_overlay = Some(m.clone());
            m
        }
    };
    if let Some(overlay) = menu_overlay {
        overlays.push(overlay);
    }

    // Section 3: Info overlays
    let info_overlays = match cache.info_overlays {
        Some(ref cached) => cached.clone(),
        None => {
            let infos = build_info_section(state, registry);
            cache.info_overlays = Some(infos.clone());
            infos
        }
    };
    overlays.extend(info_overlays);

    // Section 4: Plugin overlays (always rebuilt — no plugins yet)
    let plugin_overlays = registry.collect_slot(Slot::Overlay, state);
    for el in plugin_overlays {
        overlays.push(Overlay {
            element: el,
            anchor: OverlayAnchor::Absolute {
                x: 0,
                y: 0,
                w: state.cols,
                h: state.rows,
            },
        });
    }

    if overlays.is_empty() {
        base
    } else {
        Element::stack(base, overlays)
    }
}

/// Build the base layout: buffer + status bar + plugin slots.
#[crate::kasane_component(deps(BUFFER, STATUS, OPTIONS))]
fn build_base(state: &AppState, registry: &PluginRegistry) -> Element {
    let buffer_rows = state.available_height() as usize;

    // Collect plugin slots
    let above_buffer = registry.collect_slot(Slot::AboveBuffer, state);
    let below_buffer = registry.collect_slot(Slot::BelowBuffer, state);
    let buffer_left = registry.collect_slot(Slot::BufferLeft, state);
    let buffer_right = registry.collect_slot(Slot::BufferRight, state);
    let above_status = registry.collect_slot(Slot::AboveStatus, state);
    let status_left = registry.collect_slot(Slot::StatusLeft, state);
    let status_right = registry.collect_slot(Slot::StatusRight, state);

    // Build buffer row (center area + optional sidebars)
    let buffer_element = Element::buffer_ref(0..buffer_rows);
    let buffer_row = if buffer_left.is_empty() && buffer_right.is_empty() {
        let decorated = registry.apply_decorator(DecorateTarget::Buffer, buffer_element, state);
        FlexChild::flexible(decorated, 1.0)
    } else {
        let decorated = registry.apply_decorator(DecorateTarget::Buffer, buffer_element, state);
        let mut row_children = Vec::new();
        for el in buffer_left {
            row_children.push(FlexChild::fixed(el));
        }
        row_children.push(FlexChild::flexible(decorated, 1.0));
        for el in buffer_right {
            row_children.push(FlexChild::fixed(el));
        }
        FlexChild::flexible(Element::row(row_children), 1.0)
    };

    // Build status bar (with replacement + decorator support)
    let status_bar = registry
        .get_replacement(ReplaceTarget::StatusBar, state)
        .unwrap_or_else(|| build_status_bar(state, status_left, status_right));
    let status_bar = registry.apply_decorator(DecorateTarget::StatusBar, status_bar, state);

    // Build main column (status bar position: top or bottom)
    let mut column_children = Vec::new();
    if state.status_at_top {
        column_children.push(FlexChild::fixed(status_bar));
        for el in above_status {
            column_children.push(FlexChild::fixed(el));
        }
        for el in above_buffer {
            column_children.push(FlexChild::fixed(el));
        }
        column_children.push(buffer_row);
        for el in below_buffer {
            column_children.push(FlexChild::fixed(el));
        }
    } else {
        for el in above_buffer {
            column_children.push(FlexChild::fixed(el));
        }
        column_children.push(buffer_row);
        for el in below_buffer {
            column_children.push(FlexChild::fixed(el));
        }
        for el in above_status {
            column_children.push(FlexChild::fixed(el));
        }
        column_children.push(FlexChild::fixed(status_bar));
    }

    Element::column(column_children)
}

/// Build the menu overlay section.
#[crate::kasane_component(deps(MENU_STRUCTURE, MENU_SELECTION))]
fn build_menu_section(state: &AppState, registry: &PluginRegistry) -> Option<Overlay> {
    let menu_state = state.menu.as_ref()?;
    let replace_target = match menu_state.style {
        MenuStyle::Prompt => ReplaceTarget::MenuPrompt,
        MenuStyle::Inline => ReplaceTarget::MenuInline,
        MenuStyle::Search => ReplaceTarget::MenuSearch,
    };
    let menu_overlay = match registry.get_replacement(replace_target, state) {
        Some(replacement) => menu::build_replacement_menu_overlay(replacement, menu_state, state),
        None => menu::build_menu_overlay(menu_state, state),
    };
    menu_overlay.map(|mut overlay| {
        overlay.element = registry.apply_decorator(DecorateTarget::Menu, overlay.element, state);
        overlay
    })
}

/// Build info overlay section with collision avoidance.
#[crate::kasane_component(deps(INFO))]
fn build_info_section(state: &AppState, registry: &PluginRegistry) -> Vec<Overlay> {
    let menu_rect = super::menu::get_menu_rect(state);
    let mut avoid_rects: Vec<crate::layout::Rect> = Vec::new();
    if let Some(mr) = menu_rect {
        avoid_rects.push(mr);
    }
    // Add cursor position as a 1×1 avoid rect (collision avoidance)
    avoid_rects.push(crate::layout::Rect {
        x: state.cursor_pos.column as u16,
        y: state.cursor_pos.line as u16,
        w: 1,
        h: 1,
    });

    let mut overlays = Vec::new();
    for (info_idx, info_state) in state.infos.iter().enumerate() {
        let replace_target = match info_state.style {
            InfoStyle::Prompt => Some(ReplaceTarget::InfoPrompt),
            InfoStyle::Modal => Some(ReplaceTarget::InfoModal),
            _ => None,
        };
        let info_overlay = match replace_target.and_then(|t| registry.get_replacement(t, state)) {
            Some(replacement) => {
                info::build_replacement_info_overlay(replacement, info_state, state, &avoid_rects)
            }
            None => info::build_info_overlay_indexed(info_state, state, &avoid_rects, info_idx),
        };
        if let Some(mut overlay) = info_overlay {
            // Track this overlay's rect for subsequent infos to avoid
            if let OverlayAnchor::Absolute { x, y, w, h } = &overlay.anchor {
                avoid_rects.push(crate::layout::Rect {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                });
            }
            overlay.element =
                registry.apply_decorator(DecorateTarget::Info, overlay.element, state);
            overlays.push(overlay);
        }
    }
    overlays
}

#[crate::kasane_component(deps(STATUS, BUFFER))]
fn build_status_bar(
    state: &AppState,
    status_left: Vec<Element>,
    status_right: Vec<Element>,
) -> Element {
    let status_line =
        build_styled_line_with_base(&state.status_line, &state.status_default_face, 0);
    let mode_line =
        build_styled_line_with_base(&state.status_mode_line, &state.status_default_face, 0);
    let mode_width = line_display_width(&state.status_mode_line) as u16;

    // Status bar: [...status_left, status_line(flex:1.0), ...status_right, mode_line(fixed)]
    let mut children = Vec::new();
    for el in status_left {
        children.push(FlexChild::fixed(el));
    }
    children.push(FlexChild::flexible(status_line, 1.0));
    for el in status_right {
        children.push(FlexChild::fixed(el));
    }
    // Cursor count badge: show when there are multiple selections
    if state.cursor_count > 1 {
        let badge_text = format!(" {} sel ", state.cursor_count);
        let badge = Element::text(badge_text, state.status_default_face);
        children.push(FlexChild::fixed(badge));
    }

    if mode_width > 0 {
        children.push(FlexChild::fixed(mode_line));
    }

    Element::container(
        Element::row(children),
        Style::from(state.status_default_face),
    )
}

/// Build a StyledLine element from a protocol Line, resolving faces against a base.
pub(super) fn build_styled_line_with_base(
    line: &Line,
    base_face: &Face,
    _max_width: u16,
) -> Element {
    let resolved: Vec<Atom> = line
        .iter()
        .map(|atom| Atom {
            face: super::grid::resolve_face(&atom.face, base_face),
            contents: atom.contents.clone(),
        })
        .collect();
    Element::StyledLine(resolved)
}
