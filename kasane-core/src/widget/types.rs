//! Data types for declarative widget definitions.

use compact_str::CompactString;

use crate::element::StyleToken;
use crate::plugin::{ContribSizeHint, GutterSide, SlotId};
use crate::protocol::Face;
use crate::state::DirtyFlags;

use kasane_plugin_model::TransformTarget;

/// A face that is either a direct Face value or a reference to a theme token.
pub enum FaceOrToken {
    Direct(Face),
    Token(StyleToken),
}

/// A parsed widget file containing all widget definitions.
pub struct WidgetFile {
    pub widgets: Vec<WidgetDef>,
    /// Union of all referenced variables' dirty flags.
    pub computed_deps: DirtyFlags,
}

/// A single widget definition.
pub struct WidgetDef {
    pub name: CompactString,
    pub kind: WidgetKind,
    /// File order index → implicit priority.
    pub index: u16,
}

/// The kind of widget.
pub enum WidgetKind {
    Contribution(ContributionWidget),
    Background(BackgroundWidget),
    Transform(TransformWidget),
    Gutter(GutterWidget),
}

/// A widget that contributes an element to a slot.
pub struct ContributionWidget {
    pub slot: SlotId,
    pub parts: Vec<WidgetPart>,
    pub when: Option<CondExpr>,
    pub size_hint: ContribSizeHint,
}

/// A single part of a contribution widget (text segment with optional face/condition).
pub struct WidgetPart {
    pub template: Template,
    pub face: Option<FaceOrToken>,
    pub when: Option<CondExpr>,
}

/// A widget that provides a background layer for a line.
pub struct BackgroundWidget {
    pub line_expr: LineExpr,
    pub face: FaceOrToken,
    pub when: Option<CondExpr>,
}

/// Expression determining which line a background widget applies to.
pub enum LineExpr {
    CursorLine,
    Selection,
}

/// A widget that applies a transform patch.
pub struct TransformWidget {
    pub target: TransformTarget,
    pub patch: WidgetPatch,
    pub when: Option<CondExpr>,
}

/// Declarative transform operations available in widgets.
pub enum WidgetPatch {
    ModifyFace(FaceOrToken),
    WrapContainer(FaceOrToken),
}

/// A widget that provides gutter annotations per line.
pub struct GutterWidget {
    pub side: GutterSide,
    pub template: Template,
    pub face: Option<FaceOrToken>,
    pub when: Option<CondExpr>,
    pub line_when: Option<CondExpr>,
}

/// A template string with literal and variable segments.
///
/// Example: `" {cursor_line}:{cursor_col} "`
pub struct Template {
    pub segments: Vec<TemplateSegment>,
}

/// A segment of a template.
pub enum TemplateSegment {
    Literal(CompactString),
    Variable {
        name: CompactString,
        format: Option<TemplateFmt>,
    },
}

/// Alignment direction for formatted template variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateAlign {
    Right,
    Left,
}

/// Formatting options for template variables.
pub struct TemplateFmt {
    /// Pad to this width.
    pub width: Option<usize>,
    /// Alignment direction (default: right).
    pub align: TemplateAlign,
    /// Truncate to this many characters (with `…` suffix).
    pub truncate: Option<usize>,
}

/// Condition expression for `when=` attributes.
pub enum CondExpr {
    /// Variable is truthy (non-empty, non-"0").
    Truthy(CompactString),
    /// Variable compared to a value.
    Compare {
        variable: CompactString,
        op: CmpOp,
        value: CompactString,
    },
    And(Box<CondExpr>, Box<CondExpr>),
    Or(Box<CondExpr>, Box<CondExpr>),
    Not(Box<CondExpr>),
}

/// Comparison operators for condition expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}
