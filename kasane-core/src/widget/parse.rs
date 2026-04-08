//! KDL document → Vec<WidgetDef> parsing.

use compact_str::CompactString;

use crate::plugin::SlotId;
use crate::render::theme::parse_face_spec;
use crate::state::DirtyFlags;

use kasane_plugin_model::TransformTarget;

use super::condition::parse_condition;
use crate::plugin::{ContribSizeHint, GutterSide};

use super::types::{
    BackgroundWidget, ContributionWidget, FaceOrToken, FaceRule, GutterBranch, GutterWidget,
    LineExpr, Template, TransformWidget, WidgetDef, WidgetFile, WidgetKind, WidgetPart,
    WidgetPatch,
};
use super::variables::{validate_variable, variable_dirty_flag};

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

    parse_widget_nodes(doc.nodes())
}

/// Parse a slice of KDL nodes into a WidgetFile.
///
/// Performs semantic validation per node: invalid nodes are skipped and
/// reported as `WidgetNodeError`s, valid nodes are collected into the file.
///
/// This is the lower-level entry point; use [`parse_widgets`] when starting
/// from a KDL source string.
pub fn parse_widget_nodes(
    nodes: &[kdl::KdlNode],
) -> Result<(WidgetFile, Vec<WidgetNodeError>), WidgetParseError> {
    let mut widgets = Vec::new();
    let mut errors = Vec::new();
    let mut index: u16 = 0;

    for node in nodes {
        if widgets.len() >= MAX_WIDGETS {
            return Err(WidgetParseError::TooManyWidgets);
        }

        let name = CompactString::from(node.name().value());

        match parse_widget_node(node) {
            Ok(kind) => {
                // Validate referenced variables
                let line_context = matches!(kind, WidgetKind::Gutter(_));
                for var in collect_widget_variables(&kind) {
                    if let Some(warning) = validate_variable(&var, line_context) {
                        errors.push(WidgetNodeError {
                            name: name.to_string(),
                            message: warning,
                        });
                    }
                }
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

/// Collect dirty flags from a slice of face rules.
fn face_rules_deps(rules: &[FaceRule], flags: &mut DirtyFlags) {
    for rule in rules {
        if let Some(ref cond) = rule.when {
            for var in cond.referenced_variables() {
                *flags |= variable_dirty_flag(var);
            }
        }
    }
}

/// Compute dirty flags for a single widget definition.
pub fn compute_widget_deps(widget: &WidgetDef) -> DirtyFlags {
    let mut flags = DirtyFlags::empty();
    compute_widget_deps_inner(&widget.kind, &mut flags);
    flags
}

fn compute_deps(widgets: &[WidgetDef]) -> DirtyFlags {
    let mut flags = DirtyFlags::empty();
    for widget in widgets {
        compute_widget_deps_inner(&widget.kind, &mut flags);
    }
    flags
}

fn compute_widget_deps_inner(kind: &WidgetKind, flags: &mut DirtyFlags) {
    match kind {
        WidgetKind::Contribution(c) => {
            for part in &c.parts {
                for var in part.template.referenced_variables() {
                    *flags |= variable_dirty_flag(var);
                }
                if let Some(ref cond) = part.when {
                    for var in cond.referenced_variables() {
                        *flags |= variable_dirty_flag(var);
                    }
                }
                face_rules_deps(&part.face_rules, flags);
            }
            if let Some(ref cond) = c.when {
                for var in cond.referenced_variables() {
                    *flags |= variable_dirty_flag(var);
                }
            }
        }
        WidgetKind::Background(b) => {
            match b.line_expr {
                LineExpr::CursorLine => *flags |= DirtyFlags::BUFFER_CURSOR,
                LineExpr::Selection => *flags |= DirtyFlags::BUFFER,
            }
            if let Some(ref cond) = b.when {
                for var in cond.referenced_variables() {
                    *flags |= variable_dirty_flag(var);
                }
            }
        }
        WidgetKind::Transform(t) => {
            if let Some(ref cond) = t.when {
                for var in cond.referenced_variables() {
                    *flags |= variable_dirty_flag(var);
                }
            }
            match &t.patch {
                WidgetPatch::ModifyFace(rules) | WidgetPatch::WrapContainer(rules) => {
                    face_rules_deps(rules, flags);
                }
            }
        }
        WidgetKind::Gutter(g) => {
            *flags |= DirtyFlags::BUFFER_CURSOR;
            for branch in &g.branches {
                for var in branch.template.referenced_variables() {
                    *flags |= variable_dirty_flag(var);
                }
                face_rules_deps(&branch.face_rules, flags);
                if let Some(ref cond) = branch.line_when {
                    for var in cond.referenced_variables() {
                        *flags |= variable_dirty_flag(var);
                    }
                }
            }
            if let Some(ref cond) = g.when {
                for var in cond.referenced_variables() {
                    *flags |= variable_dirty_flag(var);
                }
            }
        }
    }
}

/// Collect variable names from face rules.
fn collect_face_rules_variables(rules: &[FaceRule], vars: &mut Vec<String>) {
    for rule in rules {
        if let Some(ref cond) = rule.when {
            vars.extend(cond.referenced_variables().into_iter().map(String::from));
        }
    }
}

/// Collect all variable names referenced by a widget for validation.
fn collect_widget_variables(kind: &WidgetKind) -> Vec<String> {
    let mut vars = Vec::new();
    match kind {
        WidgetKind::Contribution(c) => {
            for part in &c.parts {
                vars.extend(part.template.referenced_variables().map(String::from));
                if let Some(ref cond) = part.when {
                    vars.extend(cond.referenced_variables().into_iter().map(String::from));
                }
                collect_face_rules_variables(&part.face_rules, &mut vars);
            }
            if let Some(ref cond) = c.when {
                vars.extend(cond.referenced_variables().into_iter().map(String::from));
            }
        }
        WidgetKind::Background(b) => {
            if let Some(ref cond) = b.when {
                vars.extend(cond.referenced_variables().into_iter().map(String::from));
            }
        }
        WidgetKind::Transform(t) => {
            if let Some(ref cond) = t.when {
                vars.extend(cond.referenced_variables().into_iter().map(String::from));
            }
            match &t.patch {
                WidgetPatch::ModifyFace(rules) | WidgetPatch::WrapContainer(rules) => {
                    collect_face_rules_variables(rules, &mut vars);
                }
            }
        }
        WidgetKind::Gutter(g) => {
            for branch in &g.branches {
                vars.extend(branch.template.referenced_variables().map(String::from));
                collect_face_rules_variables(&branch.face_rules, &mut vars);
                if let Some(ref cond) = branch.line_when {
                    vars.extend(cond.referenced_variables().into_iter().map(String::from));
                }
            }
            if let Some(ref cond) = g.when {
                vars.extend(cond.referenced_variables().into_iter().map(String::from));
            }
        }
    }
    vars.sort();
    vars.dedup();
    vars
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
        let face_rules = match get_string_entry(node, "face") {
            Some(spec) => vec![FaceRule {
                face: parse_face_or_token(spec)?,
                when: None,
            }],
            None => Vec::new(),
        };
        parts.push(WidgetPart {
            template,
            face_rules,
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
    let when = parse_when(node)?;

    // Collect face rules: either from `face=` shorthand or `face` child nodes
    let mut face_rules = Vec::new();

    if let Some(spec) = get_string_entry(node, "face") {
        // Shorthand: single unconditional face rule
        face_rules.push(FaceRule {
            face: parse_face_or_token(spec)?,
            when: None,
        });
    }

    if let Some(children) = node.children() {
        for child in children.nodes() {
            if child.name().value() == "face" {
                face_rules.push(parse_face_rule(child)?);
            }
        }
    }

    Ok(WidgetPart {
        template,
        face_rules,
        when,
    })
}

/// Parse a `face` child node into a `FaceRule`.
///
/// ```kdl
/// face "@mode_insert" when="editor_mode == 'insert'"
/// face "default,yellow"
/// ```
fn parse_face_rule(node: &kdl::KdlNode) -> Result<FaceRule, String> {
    let spec = node
        .entry(0)
        .and_then(|e| e.value().as_string())
        .ok_or("face rule missing face spec (first positional argument)")?;
    let face = parse_face_or_token(spec)?;
    let when = parse_when(node)?;
    Ok(FaceRule { face, when })
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
    let face = parse_face_or_token(face_str)?;

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

    let when = parse_when(node)?;
    let patch_str = get_string_entry(node, "patch").unwrap_or("modify-face");

    // Collect face rules: `face=` shorthand or `face` child nodes
    let mut face_rules = Vec::new();

    if let Some(face_str) = get_string_entry(node, "face") {
        face_rules.push(FaceRule {
            face: parse_face_or_token(face_str)?,
            when: None,
        });
    }

    if let Some(children) = node.children() {
        for child in children.nodes() {
            if child.name().value() == "face" {
                face_rules.push(parse_face_rule(child)?);
            }
        }
    }

    if face_rules.is_empty() {
        return Err("transform widget missing face (use face= or face children)".to_string());
    }

    let patch = match patch_str {
        "modify-face" => WidgetPatch::ModifyFace(face_rules),
        "wrap" => WidgetPatch::WrapContainer(face_rules),
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

    let when = parse_when(node)?;
    let mut branches = Vec::new();

    // Shorthand: `text=`/`face=`/`line-when=` on node creates single branch
    if let Some(text) = get_string_entry(node, "text") {
        let template = Template::parse(text).map_err(|e| format!("template: {e}"))?;
        let face_rules = match get_string_entry(node, "face") {
            Some(spec) => vec![FaceRule {
                face: parse_face_or_token(spec)?,
                when: None,
            }],
            None => Vec::new(),
        };
        let line_when = match get_string_entry(node, "line-when") {
            Some(expr) => {
                let parsed =
                    parse_condition(expr).map_err(|e| format!("line-when condition: {e}"))?;
                Some(parsed)
            }
            None => None,
        };
        branches.push(GutterBranch {
            template,
            face_rules,
            line_when,
        });
    }

    // Children: `branch` nodes
    if let Some(children) = node.children() {
        for child in children.nodes() {
            if child.name().value() == "branch" {
                branches.push(parse_gutter_branch(child)?);
            }
        }
    }

    if branches.is_empty() {
        return Err("gutter widget has no branches (use text= or branch children)".to_string());
    }

    Ok(WidgetKind::Gutter(GutterWidget {
        side,
        branches,
        when,
    }))
}

/// Parse a `branch` child node into a `GutterBranch`.
fn parse_gutter_branch(node: &kdl::KdlNode) -> Result<GutterBranch, String> {
    let text = get_string_entry(node, "text").ok_or("branch missing 'text' attribute")?;
    let template = Template::parse(text).map_err(|e| format!("template: {e}"))?;

    let mut face_rules = Vec::new();
    if let Some(spec) = get_string_entry(node, "face") {
        face_rules.push(FaceRule {
            face: parse_face_or_token(spec)?,
            when: None,
        });
    }
    if let Some(children) = node.children() {
        for child in children.nodes() {
            if child.name().value() == "face" {
                face_rules.push(parse_face_rule(child)?);
            }
        }
    }

    let line_when = match get_string_entry(node, "line-when") {
        Some(expr) => {
            let parsed = parse_condition(expr).map_err(|e| format!("line-when condition: {e}"))?;
            Some(parsed)
        }
        None => None,
    };

    Ok(GutterBranch {
        template,
        face_rules,
        line_when,
    })
}

/// Parse a face spec that may be a direct face or a `@token` theme reference.
fn parse_face_or_token(spec: &str) -> Result<FaceOrToken, String> {
    if let Some(name) = spec.strip_prefix('@') {
        if name.is_empty() {
            return Err("empty theme token name after '@'".to_string());
        }
        Ok(FaceOrToken::Token(crate::element::StyleToken::new(
            name.replace('_', "."),
        )))
    } else {
        parse_face_spec(spec)
            .map(FaceOrToken::Direct)
            .ok_or_else(|| format!("invalid face: '{spec}'"))
    }
}

/// Get a string value from a KDL node's named entry (attribute).
fn get_string_entry<'a>(node: &'a kdl::KdlNode, name: &str) -> Option<&'a str> {
    node.entry(name).and_then(|e| e.value().as_string())
}
