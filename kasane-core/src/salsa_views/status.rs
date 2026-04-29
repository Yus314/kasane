use crate::element::{Element, FlexChild};
use crate::layout::line_display_width;
use crate::render::view::build_styled_line_with_base;
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::*;

/// Pure status bar element: status_line + mode_line in a row.
#[salsa::tracked(no_eq)]
pub fn pure_status_element(db: &dyn KasaneDb, status: StatusInput) -> Element {
    let status_line = status.status_line(db);
    let mode_line = status.status_mode_line(db);
    let default_style = status.status_default_style(db);

    let status_el = build_styled_line_with_base(status_line, &default_style, 0);
    let mode_el = build_styled_line_with_base(mode_line, &default_style, 0);
    let mode_width = line_display_width(mode_line) as u16;

    let mut children = Vec::new();
    children.push(FlexChild::flexible(status_el, 1.0));
    if mode_width > 0 {
        children.push(FlexChild::fixed(mode_el));
    }
    Element::row(children)
}
