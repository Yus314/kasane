//! Type conversions between WIT-generated types and kasane-core types.

// ---------------------------------------------------------------------------
// Enum conversion macros (defined before `mod` so submodules can use them)
// ---------------------------------------------------------------------------

/// Generate two functions that map 1:1 between a WIT enum and a native enum.
macro_rules! bidirectional_enum {
    ($to_native:ident: $wit_ty:ty => $native_ty:ty,
     $to_wit:ident: $native_ty2:ty => $wit_ty2:ty,
     { $($variant:ident),* $(,)? }) => {
        fn $to_native(w: $wit_ty) -> $native_ty {
            match w { $( <$wit_ty>::$variant => <$native_ty>::$variant, )* }
        }
        fn $to_wit(n: $native_ty) -> $wit_ty {
            match n { $( <$native_ty>::$variant => <$wit_ty>::$variant, )* }
        }
    };
}

/// Generate a single function that maps 1:1 between two enums with identical variant names.
macro_rules! enum_convert {
    ($vis:vis $fn_name:ident: $from_ty:ty => $to_ty:ty, { $($variant:ident),* $(,)? }) => {
        $vis fn $fn_name(v: $from_ty) -> $to_ty {
            match v { $( <$from_ty>::$variant => <$to_ty>::$variant, )* }
        }
    };
}

mod command;
mod context;
mod display;
mod element;
pub(crate) mod input;
mod workspace;

pub(crate) use command::*;
pub(crate) use context::*;
pub(crate) use display::*;
pub(crate) use element::*;
pub(crate) use input::{
    default_scroll_candidate_to_wit, io_event_to_wit, key_event_to_wit, mouse_event_to_wit,
    wit_key_event_to_key_event, wit_key_group_decls_to_compiled_key_map,
    wit_key_response_to_key_response, wit_scroll_plan_to_scroll_plan,
    wit_scroll_policy_result_to_result,
};
pub(crate) use workspace::*;

use crate::bindings::kasane::plugin::types as wit;
use kasane_core::layout::Rect;
use kasane_core::protocol::{Atom, Attributes, Color, Face, NamedColor};

// ---------------------------------------------------------------------------
// Face / Color conversions (WIT ↔ native)
// ---------------------------------------------------------------------------

bidirectional_enum! {
    wit_named_to_named: wit::NamedColor => NamedColor,
    named_to_wit: NamedColor => wit::NamedColor,
    {
        Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
        BrightBlack, BrightRed, BrightGreen, BrightYellow,
        BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
    }
}

pub(crate) fn wit_face_to_face(wf: &wit::Face) -> Face {
    Face {
        fg: wit_color_to_color(&wf.fg),
        bg: wit_color_to_color(&wf.bg),
        underline: wit_color_to_color(&wf.underline),
        attributes: Attributes::from_bits_truncate(wf.attributes),
    }
}

fn wit_color_to_color(wc: &wit::Color) -> Color {
    match wc {
        wit::Color::DefaultColor => Color::Default,
        wit::Color::Named(n) => Color::Named(wit_named_to_named(*n)),
        wit::Color::Rgb(rgb) => Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
    }
}

// ---------------------------------------------------------------------------
// Atom conversion (WIT → native)
// ---------------------------------------------------------------------------

pub(crate) fn wit_atom_to_atom(wa: &wit::Atom) -> Atom {
    Atom {
        face: wit_face_to_face(&wa.face),
        contents: wa.contents.as_str().into(),
    }
}

// ---------------------------------------------------------------------------
// Face / Color / Atom conversions (native → WIT)
// ---------------------------------------------------------------------------

pub(crate) fn color_to_wit(c: &Color) -> wit::Color {
    match c {
        Color::Default => wit::Color::DefaultColor,
        Color::Named(n) => wit::Color::Named(named_to_wit(*n)),
        Color::Rgb { r, g, b } => wit::Color::Rgb(wit::RgbColor {
            r: *r,
            g: *g,
            b: *b,
        }),
    }
}

pub(crate) fn face_to_wit(f: &Face) -> wit::Face {
    wit::Face {
        fg: color_to_wit(&f.fg),
        bg: color_to_wit(&f.bg),
        underline: color_to_wit(&f.underline),
        attributes: f.attributes.bits(),
    }
}

pub(crate) fn atom_to_wit(a: &Atom) -> wit::Atom {
    wit::Atom {
        face: face_to_wit(&a.face),
        contents: a.contents.to_string(),
    }
}

pub(crate) fn atoms_to_wit(atoms: &[Atom]) -> Vec<wit::Atom> {
    atoms.iter().map(atom_to_wit).collect()
}

pub(crate) fn wit_atoms_to_atoms(atoms: &[wit::Atom]) -> Vec<Atom> {
    atoms.iter().map(wit_atom_to_atom).collect()
}

// ---------------------------------------------------------------------------
// Rect conversions (used by element + context submodules)
// ---------------------------------------------------------------------------

pub(crate) fn wit_rect_to_rect(rect: &wit::Rect) -> Rect {
    Rect {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

pub(crate) fn rect_to_wit(rect: &Rect) -> wit::Rect {
    wit::Rect {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

// ---------------------------------------------------------------------------
// From impls for Coord / Rect (enables `.into()` at call sites)
// ---------------------------------------------------------------------------

use kasane_core::protocol::Coord;

impl From<Coord> for wit::Coord {
    fn from(c: Coord) -> Self {
        wit::Coord {
            line: c.line,
            column: c.column,
        }
    }
}

impl From<wit::Coord> for Coord {
    fn from(c: wit::Coord) -> Self {
        Coord {
            line: c.line,
            column: c.column,
        }
    }
}

impl From<Rect> for wit::Rect {
    fn from(r: Rect) -> Self {
        wit::Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }
    }
}

impl From<wit::Rect> for Rect {
    fn from(r: wit::Rect) -> Self {
        Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }
    }
}

// ---------------------------------------------------------------------------
// Cell decoration conversions (WIT → native)
// ---------------------------------------------------------------------------

use kasane_core::plugin::{CellDecoration, DecorationTarget, FaceMerge};

pub(crate) fn wit_cell_decorations_to_decorations(
    wits: &[wit::CellDecoration],
) -> Vec<CellDecoration> {
    wits.iter().map(wit_cell_decoration_to_decoration).collect()
}

fn wit_cell_decoration_to_decoration(w: &wit::CellDecoration) -> CellDecoration {
    CellDecoration {
        target: wit_decoration_target_to_target(&w.target),
        face: wit_face_to_face(&w.face),
        merge: wit_face_merge_to_merge(w.merge),
        priority: w.priority,
    }
}

fn wit_decoration_target_to_target(w: &wit::DecorationTarget) -> DecorationTarget {
    match w {
        wit::DecorationTarget::Cell(coord) => DecorationTarget::Cell((*coord).into()),
        wit::DecorationTarget::CellRange(range) => DecorationTarget::Range {
            start: range.start.into(),
            end: range.end.into(),
        },
        wit::DecorationTarget::Column(col) => DecorationTarget::Column { column: *col },
    }
}

fn wit_face_merge_to_merge(code: u8) -> FaceMerge {
    match code {
        0 => FaceMerge::Replace,
        1 => FaceMerge::Overlay,
        _ => FaceMerge::Background,
    }
}

#[cfg(test)]
mod tests;
