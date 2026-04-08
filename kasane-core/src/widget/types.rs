//! Data types for declarative widget definitions.

use compact_str::CompactString;

use crate::element::StyleToken;
use crate::plugin::{ContribSizeHint, GutterSide, SlotId};
use crate::protocol::Face;
use crate::state::DirtyFlags;

use kasane_plugin_model::TransformTarget;

use super::predicate::Predicate;

/// A face that is either a direct Face value or a reference to a theme token.
#[derive(Clone)]
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
#[derive(Clone)]
pub enum WidgetKind {
    Contribution(ContributionWidget),
    Background(BackgroundWidget),
    Transform(TransformWidget),
    Gutter(GutterWidget),
    Inline(InlineWidget),
    VirtualText(VirtualTextWidget),
}

/// A widget that contributes an element to a slot.
#[derive(Clone)]
pub struct ContributionWidget {
    pub slot: SlotId,
    pub parts: Vec<WidgetPart>,
    pub when: Option<Predicate>,
    pub size_hint: ContribSizeHint,
}

/// A face rule: a face with an optional condition. First matching rule wins.
#[derive(Clone)]
pub struct FaceRule {
    pub face: FaceOrToken,
    pub when: Option<Predicate>,
}

/// A single part of a contribution widget (text segment with face rules and condition).
#[derive(Clone)]
pub struct WidgetPart {
    pub template: Template,
    /// Face rules evaluated in order; first match wins. Empty = default face.
    pub face_rules: Vec<FaceRule>,
    pub when: Option<Predicate>,
}

/// A widget that provides a background layer for a line.
#[derive(Clone)]
pub struct BackgroundWidget {
    pub line_expr: LineExpr,
    pub face: FaceOrToken,
    pub when: Option<Predicate>,
}

/// Expression determining which line a background widget applies to.
#[derive(Clone)]
pub enum LineExpr {
    CursorLine,
    Selection,
}

/// A widget that applies a transform patch.
#[derive(Clone)]
pub struct TransformWidget {
    pub target: TransformTarget,
    pub patch: WidgetPatch,
    pub when: Option<Predicate>,
}

/// Declarative transform operations available in widgets.
#[derive(Clone)]
pub enum WidgetPatch {
    ModifyFace(Vec<FaceRule>),
    WrapContainer(Vec<FaceRule>),
}

/// A branch in a gutter widget: template + face rules with a per-line condition.
#[derive(Clone)]
pub struct GutterBranch {
    pub template: Template,
    pub face_rules: Vec<FaceRule>,
    /// Per-line condition (evaluated per line).
    pub line_when: Option<Predicate>,
}

/// A widget that provides gutter annotations per line.
#[derive(Clone)]
pub struct GutterWidget {
    pub side: GutterSide,
    /// Branches evaluated in order; first matching (line_when) branch wins.
    pub branches: Vec<GutterBranch>,
    /// Global on/off condition.
    pub when: Option<Predicate>,
}

/// A widget that inserts inline decorations based on pattern matching.
///
/// Matches a substring pattern against each visible line and applies a face
/// to the matched range.
#[derive(Clone)]
pub struct InlineWidget {
    /// Substring pattern to match in line content.
    pub pattern: CompactString,
    /// Face to apply to matched ranges.
    pub face: FaceOrToken,
    /// Global on/off condition.
    pub when: Option<Predicate>,
}

/// A widget that appends virtual text at the end of lines.
#[derive(Clone)]
pub struct VirtualTextWidget {
    /// Template for the virtual text content.
    pub template: Template,
    /// Face rules for the virtual text.
    pub face_rules: Vec<FaceRule>,
    /// Global on/off condition.
    pub when: Option<Predicate>,
}

/// A template string with literal and variable segments.
///
/// Example: `" {cursor_line}:{cursor_col} "`
#[derive(Clone)]
pub struct Template {
    pub segments: Vec<TemplateSegment>,
}

/// A segment of a template.
#[derive(Clone)]
pub enum TemplateSegment {
    Literal(CompactString),
    Variable {
        name: CompactString,
        format: Option<TemplateFmt>,
    },
    /// Inline conditional: `{?condition:then_text}` or `{?condition:then_text:else_text}`.
    Conditional {
        predicate: super::predicate::Predicate,
        then_segments: Vec<TemplateSegment>,
        else_segments: Vec<TemplateSegment>,
    },
}

/// Alignment direction for formatted template variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateAlign {
    Right,
    Left,
}

/// Formatting options for template variables.
#[derive(Clone)]
pub struct TemplateFmt {
    /// Pad to this width.
    pub width: Option<usize>,
    /// Alignment direction (default: right).
    pub align: TemplateAlign,
    /// Truncate to this many characters (with `…` suffix).
    pub truncate: Option<usize>,
}

/// A typed value for variable resolution and comparison.
///
/// Eliminates per-frame string→number parsing by carrying type information
/// through the variable resolution → condition evaluation → template expansion pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Str(CompactString),
    Bool(bool),
    Empty,
}

impl Value {
    /// Truthiness: `Int(0)`, `Bool(false)`, `Empty`, `Str("")` → false; everything else → true.
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Int(n) => *n != 0,
            Self::Str(s) => !s.is_empty(),
            Self::Bool(b) => *b,
            Self::Empty => false,
        }
    }

    /// Convert to a display string for template expansion.
    pub fn to_display(&self) -> CompactString {
        match self {
            Self::Int(n) => {
                let mut buf = itoa::Buffer::new();
                CompactString::from(buf.format(*n))
            }
            Self::Str(s) => s.clone(),
            Self::Bool(true) => CompactString::from("true"),
            Self::Bool(false) => CompactString::default(),
            Self::Empty => CompactString::default(),
        }
    }

    /// Compare two values. Int×Int → numeric; Str×Str → lexicographic;
    /// Bool×Bool → ordinal; mixed types → coerce both to string.
    pub fn compare(&self, op: CmpOp, rhs: &Value) -> bool {
        match (self, rhs) {
            (Self::Int(l), Self::Int(r)) => cmp_ord(l.cmp(r), op),
            (Self::Str(l), Self::Str(r)) => cmp_ord(l.cmp(r), op),
            (Self::Bool(l), Self::Bool(r)) => cmp_ord(l.cmp(r), op),
            // Mixed types: coerce to string for comparison
            _ => {
                let l = self.to_display();
                let r = rhs.to_display();
                cmp_ord(l.cmp(&r), op)
            }
        }
    }
}

fn cmp_ord(ord: std::cmp::Ordering, op: CmpOp) -> bool {
    match op {
        CmpOp::Eq => ord.is_eq(),
        CmpOp::Ne => ord.is_ne(),
        CmpOp::Gt => ord.is_gt(),
        CmpOp::Lt => ord.is_lt(),
        CmpOp::Ge => ord.is_ge(),
        CmpOp::Le => ord.is_le(),
    }
}

/// Condition expression for `when=` attributes.
///
/// This is a type alias for the unified `Predicate` type.
/// Widget conditions use the variable-based subset of `Predicate`.
pub type CondExpr = Predicate;

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
