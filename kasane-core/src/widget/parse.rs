//! KDL document → Vec<WidgetDef> parsing.

use compact_str::CompactString;

use crate::plugin::SlotId;
use crate::render::theme::parse_face_spec;
use crate::state::DirtyFlags;

use kasane_plugin_model::TransformTarget;

use super::condition::parse_condition;
use crate::plugin::{ContribSizeHint, GutterSide};

use super::types::{
    BackgroundWidget, ContributionWidget, GutterWidget, LineExpr, Template, TransformWidget,
    WidgetDef, WidgetFile, WidgetKind, WidgetPart, WidgetPatch,
};
use super::variables::variable_dirty_flag;

/// Errors during widget file parsing.
#[derive(Debug)]
pub enum WidgetParseError {
    /// KDL syntax error.
    Syntax(String),
    /// Too many widget definitions.
    TooManyWidgets,
}

impl std::fmt::Display for WidgetParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(msg) => write!(f, "KDL syntax error: {msg}"),
            Self::TooManyWidgets => write!(f, "too many widgets (max 64)"),
        }
    }
}

/// Semantic error for a single widget node (skipped, not fatal).
#[derive(Debug)]
pub struct WidgetNodeError {
    pub name: String,
    pub message: String,
}

const MAX_WIDGETS: usize = 64;

/// Parse a KDL source string into a WidgetFile.
///
/// Stage 1: KDL syntax parsing (failure = entire file rejected).
/// Stage 2: Semantic validation per node (invalid nodes skipped, valid collected).
///
/// Returns the parsed file and any per-node errors for diagnostics.
pub fn parse_widgets(source: &str) -> Result<(WidgetFile, Vec<WidgetNodeError>), WidgetParseError> {
    let doc: kdl::KdlDocument = source
        .parse()
        .map_err(|e: kdl::KdlError| WidgetParseError::Syntax(e.to_string()))?;

    let mut widgets = Vec::new();
    let mut errors = Vec::new();
    let mut index: u16 = 0;

    for node in doc.nodes() {
        if widgets.len() >= MAX_WIDGETS {
            return Err(WidgetParseError::TooManyWidgets);
        }

        let name = CompactString::from(node.name().value());

        match parse_widget_node(node) {
            Ok(kind) => {
                widgets.push(WidgetDef { name, kind, index });
                index = index.saturating_add(1);
            }
            Err(msg) => {
                errors.push(WidgetNodeError {
                    name: name.to_string(),
                    message: msg,
                });
            }
        }
    }

    // Compute dependency flags
    let computed_deps = compute_deps(&widgets);

    Ok((
        WidgetFile {
            widgets,
            computed_deps,
        },
        errors,
    ))
}

fn compute_deps(widgets: &[WidgetDef]) -> DirtyFlags {
    let mut flags = DirtyFlags::empty();
    for widget in widgets {
        match &widget.kind {
            WidgetKind::Contribution(c) => {
                for part in &c.parts {
                    for var in part.template.referenced_variables() {
                        flags |= variable_dirty_flag(var);
                    }
                    if let Some(ref cond) = part.when {
                        for var in cond.referenced_variables() {
                            flags |= variable_dirty_flag(var);
                        }
                    }
                }
                if let Some(ref cond) = c.when {
                    for var in cond.referenced_variables() {
                        flags |= variable_dirty_flag(var);
                    }
                }
            }
            WidgetKind::Background(b) => {
                match b.line_expr {
                    LineExpr::CursorLine => flags |= DirtyFlags::BUFFER_CURSOR,
                    // Selection depends on both content (face analysis) and cursor
                    LineExpr::Selection => flags |= DirtyFlags::BUFFER,
                }
                if let Some(ref cond) = b.when {
                    for var in cond.referenced_variables() {
                        flags |= variable_dirty_flag(var);
                    }
                }
            }
            WidgetKind::Transform(t) => {
                if let Some(ref cond) = t.when {
                    for var in cond.referenced_variables() {
                        flags |= variable_dirty_flag(var);
                    }
                }
            }
            WidgetKind::Gutter(g) => {
                // Gutter always depends on cursor position (for relative_line, is_cursor_line)
                flags |= DirtyFlags::BUFFER_CURSOR;
                for var in g.template.referenced_variables() {
                    flags |= variable_dirty_flag(var);
                }
                if let Some(ref cond) = g.when {
                    for var in cond.referenced_variables() {
                        flags |= variable_dirty_flag(var);
                    }
                }
                if let Some(ref cond) = g.line_when {
                    for var in cond.referenced_variables() {
                        flags |= variable_dirty_flag(var);
                    }
                }
            }
        }
    }
    flags
}

/// Parse a single KDL node into a WidgetKind.
fn parse_widget_node(node: &kdl::KdlNode) -> Result<WidgetKind, String> {
    let kind_str = get_string_entry(node, "kind").unwrap_or("contribution");

    match kind_str {
        "contribution" => parse_contribution_node(node),
        "background" => parse_background_node(node),
        "transform" => parse_transform_node(node),
        "gutter" => parse_gutter_node(node),
        other => Err(format!("unknown widget kind: '{other}'")),
    }
}

fn parse_contribution_node(node: &kdl::KdlNode) -> Result<WidgetKind, String> {
    let slot = parse_slot(node)?;
    let when = parse_when(node)?;
    let size_hint = parse_size_hint(node)?;

    let mut parts = Vec::new();

    // Shorthand: `text=` on node creates single-part widget
    if let Some(text) = get_string_entry(node, "text") {
        let template = Template::parse(text).map_err(|e| format!("template: {e}"))?;
        let face = get_string_entry(node, "face")
            .map(|s| parse_face_spec(s).ok_or_else(|| format!("invalid face: '{s}'")))
            .transpose()?;
        parts.push(WidgetPart {
            template,
            face,
            when: None,
        });
    }

    // Children: `part` nodes
    if let Some(children) = node.children() {
        for child in children.nodes() {
            if child.name().value() == "part" {
                parts.push(parse_widget_part(child)?);
            }
        }
    }

    if parts.is_empty() {
        return Err("contribution widget has no parts (use text= or part children)".to_string());
    }

    Ok(WidgetKind::Contribution(ContributionWidget {
        slot,
        parts,
        when,
        size_hint,
    }))
}

fn parse_widget_part(node: &kdl::KdlNode) -> Result<WidgetPart, String> {
    let text = get_string_entry(node, "text").ok_or("part missing 'text' attribute")?;
    let template = Template::parse(text).map_err(|e| format!("template: {e}"))?;
    let face = get_string_entry(node, "face")
        .map(|s| parse_face_spec(s).ok_or_else(|| format!("invalid face: '{s}'")))
        .transpose()?;
    let when = parse_when(node)?;

    Ok(WidgetPart {
        template,
        face,
        when,
    })
}

fn parse_background_node(node: &kdl::KdlNode) -> Result<WidgetKind, String> {
    let line_str = get_string_entry(node, "line").unwrap_or("cursor");
    let line_expr = match line_str {
        "cursor" => LineExpr::CursorLine,
        "selection" => LineExpr::Selection,
        other => return Err(format!("unknown line expression: '{other}'")),
    };

    let face_str =
        get_string_entry(node, "face").ok_or("background widget missing 'face' attribute")?;
    let face = parse_face_spec(face_str).ok_or_else(|| format!("invalid face: '{face_str}'"))?;

    let when = parse_when(node)?;

    Ok(WidgetKind::Background(BackgroundWidget {
        line_expr,
        face,
        when,
    }))
}

fn parse_transform_node(node: &kdl::KdlNode) -> Result<WidgetKind, String> {
    let target_str =
        get_string_entry(node, "target").ok_or("transform widget missing 'target' attribute")?;
    let target = parse_transform_target(target_str)?;

    let face_str =
        get_string_entry(node, "face").ok_or("transform widget missing 'face' attribute")?;
    let face = parse_face_spec(face_str).ok_or_else(|| format!("invalid face: '{face_str}'"))?;

    let when = parse_when(node)?;

    let patch_str = get_string_entry(node, "patch").unwrap_or("modify-face");
    let patch = match patch_str {
        "modify-face" => WidgetPatch::ModifyFace(face),
        "wrap" => WidgetPatch::WrapContainer(face),
        other => return Err(format!("unknown patch kind: '{other}'")),
    };

    Ok(WidgetKind::Transform(TransformWidget {
        target,
        patch,
        when,
    }))
}

fn parse_slot(node: &kdl::KdlNode) -> Result<SlotId, String> {
    let slot_str =
        get_string_entry(node, "slot").ok_or("contribution widget missing 'slot' attribute")?;
    match slot_str {
        "status-left" => Ok(SlotId::STATUS_LEFT),
        "status-right" => Ok(SlotId::STATUS_RIGHT),
        "buffer-left" => Ok(SlotId::BUFFER_LEFT),
        "buffer-right" => Ok(SlotId::BUFFER_RIGHT),
        "above-buffer" => Ok(SlotId::ABOVE_BUFFER),
        "below-buffer" => Ok(SlotId::BELOW_BUFFER),
        "above-status" => Ok(SlotId::ABOVE_STATUS),
        other => Err(format!("unknown slot: '{other}'")),
    }
}

fn parse_transform_target(s: &str) -> Result<TransformTarget, String> {
    match s {
        "status" | "status-bar" => Ok(TransformTarget::STATUS_BAR),
        "buffer" => Ok(TransformTarget::BUFFER),
        "menu" => Ok(TransformTarget::MENU),
        "menu-prompt" => Ok(TransformTarget::MENU_PROMPT),
        "menu-inline" => Ok(TransformTarget::MENU_INLINE),
        "menu-search" => Ok(TransformTarget::MENU_SEARCH),
        "info" => Ok(TransformTarget::INFO),
        "info-prompt" => Ok(TransformTarget::INFO_PROMPT),
        "info-modal" => Ok(TransformTarget::INFO_MODAL),
        other => Err(format!("unknown transform target: '{other}'")),
    }
}

fn parse_when(node: &kdl::KdlNode) -> Result<Option<super::types::CondExpr>, String> {
    match get_string_entry(node, "when") {
        Some(expr) => {
            let parsed = parse_condition(expr).map_err(|e| format!("condition: {e}"))?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

fn parse_size_hint(node: &kdl::KdlNode) -> Result<ContribSizeHint, String> {
    match get_string_entry(node, "size") {
        None | Some("auto") => Ok(ContribSizeHint::Auto),
        Some(s) => {
            if let Some(n) = s.strip_suffix("col") {
                let val: u16 = n
                    .parse()
                    .map_err(|_| format!("invalid size value: '{s}'"))?;
                Ok(ContribSizeHint::Fixed(val))
            } else if let Some(n) = s.strip_suffix("fr") {
                let val: f32 = n
                    .parse()
                    .map_err(|_| format!("invalid size value: '{s}'"))?;
                Ok(ContribSizeHint::Flex(val))
            } else {
                Err(format!(
                    "invalid size format: '{s}' (expected 'auto', 'Ncol', or 'Nfr')"
                ))
            }
        }
    }
}

fn parse_gutter_node(node: &kdl::KdlNode) -> Result<WidgetKind, String> {
    let side_str = get_string_entry(node, "side").unwrap_or("left");
    let side = match side_str {
        "left" => GutterSide::Left,
        "right" => GutterSide::Right,
        other => return Err(format!("unknown gutter side: '{other}'")),
    };

    let text = get_string_entry(node, "text").ok_or("gutter widget missing 'text' attribute")?;
    let template = Template::parse(text).map_err(|e| format!("template: {e}"))?;

    let face = get_string_entry(node, "face")
        .map(|s| parse_face_spec(s).ok_or_else(|| format!("invalid face: '{s}'")))
        .transpose()?;

    let when = parse_when(node)?;
    let line_when = match get_string_entry(node, "line-when") {
        Some(expr) => {
            let parsed = parse_condition(expr).map_err(|e| format!("line-when condition: {e}"))?;
            Some(parsed)
        }
        None => None,
    };

    Ok(WidgetKind::Gutter(GutterWidget {
        side,
        template,
        face,
        when,
        line_when,
    }))
}

/// Get a string value from a KDL node's named entry (attribute).
fn get_string_entry<'a>(node: &'a kdl::KdlNode, name: &str) -> Option<&'a str> {
    node.entry(name).and_then(|e| e.value().as_string())
}
