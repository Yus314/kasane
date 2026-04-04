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
    default_scroll_candidate_to_wit, drop_event_to_wit, io_event_to_wit, key_event_to_wit,
    mouse_event_to_wit, wit_key_event_to_key_event, wit_key_group_decls_to_compiled_key_map,
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

use kasane_core::plugin::{
    CellDecoration, CursorEffect, CursorEffectOrn, CursorStyleOrn, DecorationTarget, FaceMerge,
    OrnamentBatch as CoreOrnamentBatch, OrnamentModality, RenderOrnamentContext, SurfaceOrn,
    SurfaceOrnAnchor, SurfaceOrnKind,
};
use kasane_core::render::{CursorStyle, CursorStyleHint};

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

// ---------------------------------------------------------------------------
// Render ornament conversions (WIT ↔ native)
// ---------------------------------------------------------------------------

pub(crate) fn render_ornament_context_to_wit(ctx: &RenderOrnamentContext) -> wit::OrnamentContext {
    wit::OrnamentContext {
        screen_cols: ctx.screen_cols,
        screen_rows: ctx.screen_rows,
        visible_line_start: ctx.visible_line_start,
        visible_line_end: ctx.visible_line_end,
    }
}

pub(crate) fn wit_ornament_batch_to_ornament_batch(w: &wit::OrnamentBatch) -> CoreOrnamentBatch {
    CoreOrnamentBatch {
        emphasis: w
            .emphasis
            .iter()
            .map(wit_cell_decoration_to_decoration)
            .collect(),
        cursor_style: w.cursor_style.as_ref().map(wit_cursor_style_orn),
        cursor_effects: w.cursor_effects.iter().map(wit_cursor_effect_orn).collect(),
        surfaces: w
            .surfaces
            .iter()
            .map(wit_surface_orn_to_surface_orn)
            .collect(),
    }
}

fn wit_cursor_style_orn(w: &wit::CursorStyleOrn) -> CursorStyleOrn {
    CursorStyleOrn {
        hint: wit_u8_to_cursor_style_hint(w.shape),
        priority: w.priority,
        modality: wit_ornament_modality_to_modality(w.modality),
    }
}

fn wit_cursor_effect_orn(w: &wit::CursorEffectOrn) -> CursorEffectOrn {
    CursorEffectOrn {
        kind: wit_cursor_effect_to_effect(w.kind),
        face: wit_face_to_face(&w.face),
        priority: w.priority,
        modality: wit_ornament_modality_to_modality(w.modality),
    }
}

fn wit_cursor_effect_to_effect(w: wit::CursorEffect) -> CursorEffect {
    match w {
        wit::CursorEffect::Halo => CursorEffect::Halo,
        wit::CursorEffect::Ring => CursorEffect::Ring,
        wit::CursorEffect::Emphasis => CursorEffect::Emphasis,
    }
}

fn wit_u8_to_cursor_style_hint(code: u8) -> CursorStyleHint {
    let shape = match code {
        0 => CursorStyle::Block,
        1 => CursorStyle::Bar,
        2 => CursorStyle::Underline,
        3 => CursorStyle::Outline,
        _ => CursorStyle::Block,
    };
    shape.into()
}

fn wit_surface_orn_to_surface_orn(w: &wit::SurfaceOrn) -> SurfaceOrn {
    SurfaceOrn {
        anchor: wit_surface_orn_anchor_to_anchor(&w.anchor),
        kind: wit_surface_orn_kind_to_kind(w.kind),
        face: wit_face_to_face(&w.face),
        priority: w.priority,
        modality: wit_ornament_modality_to_modality(w.modality),
    }
}

fn wit_surface_orn_anchor_to_anchor(w: &wit::SurfaceOrnAnchor) -> SurfaceOrnAnchor {
    match w {
        wit::SurfaceOrnAnchor::FocusedSurface => SurfaceOrnAnchor::FocusedSurface,
        wit::SurfaceOrnAnchor::SurfaceKey(key) => SurfaceOrnAnchor::SurfaceKey(key.clone()),
    }
}

fn wit_surface_orn_kind_to_kind(w: wit::SurfaceOrnKind) -> SurfaceOrnKind {
    match w {
        wit::SurfaceOrnKind::FocusFrame => SurfaceOrnKind::FocusFrame,
        wit::SurfaceOrnKind::InactiveTint => SurfaceOrnKind::InactiveTint,
    }
}

fn wit_ornament_modality_to_modality(w: wit::OrnamentModality) -> OrnamentModality {
    match w {
        wit::OrnamentModality::Must => OrnamentModality::Must,
        wit::OrnamentModality::May => OrnamentModality::May,
        wit::OrnamentModality::Approximate => OrnamentModality::Approximate,
    }
}

// ---------------------------------------------------------------------------
// ChannelValue conversions (WIT ↔ native)
// ---------------------------------------------------------------------------

use kasane_core::plugin::channel::ChannelValue;

pub(crate) fn channel_value_to_wit(cv: &ChannelValue) -> wit::ChannelValue {
    wit::ChannelValue {
        data: cv.data().to_vec(),
        type_hint: cv.type_hint().to_string(),
    }
}

pub(crate) fn wit_channel_value_to_core(wv: &wit::ChannelValue) -> ChannelValue {
    ChannelValue::from_raw(wv.data.clone(), wv.type_hint.clone())
}

// ---------------------------------------------------------------------------
// SettingValue conversions (WIT → native)
// ---------------------------------------------------------------------------

use kasane_core::plugin::setting::SettingValue;

pub(crate) fn wit_setting_value_to_setting_value(wv: &wit::SettingValue) -> SettingValue {
    match wv {
        wit::SettingValue::BoolVal(b) => SettingValue::Bool(*b),
        wit::SettingValue::IntegerVal(i) => SettingValue::Integer(*i),
        wit::SettingValue::FloatVal(f) => SettingValue::Float(*f),
        wit::SettingValue::StringVal(s) => SettingValue::Str(s.as_str().into()),
    }
}

#[cfg(test)]
mod tests;
