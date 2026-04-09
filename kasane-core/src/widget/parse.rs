//! KDL document → Vec<WidgetDef> parsing.

use compact_str::CompactString;

use crate::plugin::SlotId;
use crate::render::theme::parse_face_spec;
use crate::state::DirtyFlags;

use kasane_plugin_model::TransformTarget;

use super::condition::parse_condition;
use crate::plugin::{ContribSizeHint, GutterSide};

use super::predicate::Predicate;
use super::types::{
    BackgroundWidget, ContributionWidget, FaceOrToken, FaceRule, GutterBranch, GutterWidget,
    InlinePattern, InlineWidget, LineExpr, Template, TransformWidget, VirtualTextWidget, WidgetDef,
    WidgetEffect, WidgetFile, WidgetKind, WidgetPart, WidgetPatch,
};
use super::variables::{edit_distance, validate_variable, variable_dirty_flag};
use super::visitor::{WidgetVisitor, walk_widget_kind};

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
    let mut seen_names = std::collections::HashSet::new();

    parse_nodes_into(
        nodes,
        None,
        &mut widgets,
        &mut errors,
        &mut index,
        &mut seen_names,
    )?;

    // Compute dependency flags
    let computed_deps = compute_deps(&widgets);

    Ok((
        WidgetFile {
            widgets,
            computed_deps,
            included_paths: Vec::new(),
        },
        errors,
    ))
}

/// Process a list of KDL nodes, handling `group` nodes by recursing into children.
fn parse_nodes_into(
    nodes: &[kdl::KdlNode],
    group_when: Option<&Predicate>,
    widgets: &mut Vec<WidgetDef>,
    errors: &mut Vec<WidgetNodeError>,
    index: &mut u16,
    seen_names: &mut std::collections::HashSet<CompactString>,
) -> Result<(), WidgetParseError> {
    for node in nodes {
        if widgets.len() >= MAX_WIDGETS {
            return Err(WidgetParseError::TooManyWidgets);
        }

        let name = CompactString::from(node.name().value());

        // Handle `group` nodes: recurse into children with inherited condition.
        if name == "group" {
            let group_cond = match parse_when(node) {
                Ok(cond) => cond,
                Err(msg) => {
                    errors.push(WidgetNodeError {
                        name: name.to_string(),
                        message: msg,
                    });
                    continue;
                }
            };
            // Merge outer group condition with this group's condition.
            let merged = merge_conditions(group_when, group_cond.as_ref());

            if let Some(children) = node.children() {
                parse_nodes_into(
                    children.nodes(),
                    merged.as_ref(),
                    widgets,
                    errors,
                    index,
                    seen_names,
                )?;
            } else {
                errors.push(WidgetNodeError {
                    name: name.to_string(),
                    message: "group has no children".to_string(),
                });
            }
            continue;
        }

        if !seen_names.insert(name.clone()) {
            errors.push(WidgetNodeError {
                name: name.to_string(),
                message: format!(
                    "duplicate widget name '{}' (previous definition will be overwritten)",
                    name
                ),
            });
        }

        let order = parse_order(node);

        match parse_widget_def(node) {
            Ok((effects, when)) => {
                // Merge group condition with widget's own condition.
                let effective_when = merge_conditions(group_when, when.as_ref());

                // Validate referenced variables for each effect
                for effect in &effects {
                    let line_context = matches!(effect.kind, WidgetKind::Gutter(_));
                    for var in collect_widget_variables(&effect.kind) {
                        if let Some(warning) = validate_variable(&var, line_context) {
                            errors.push(WidgetNodeError {
                                name: name.to_string(),
                                message: warning,
                            });
                        }
                    }
                }
                // Also validate the effective when condition variables
                if let Some(ref cond) = effective_when {
                    for var in cond.referenced_variables() {
                        if let Some(warning) = validate_variable(var, false) {
                            errors.push(WidgetNodeError {
                                name: name.to_string(),
                                message: warning,
                            });
                        }
                    }
                }
                widgets.push(WidgetDef {
                    name,
                    effects,
                    when: effective_when,
                    index: *index,
                    order,
                });
                *index = index.saturating_add(1);
            }
            Err(msg) => {
                errors.push(WidgetNodeError {
                    name: name.to_string(),
                    message: msg,
                });
            }
        }
    }
    Ok(())
}

/// Merge an outer (group) condition with an inner (widget) condition using AND.
fn merge_conditions(outer: Option<&Predicate>, inner: Option<&Predicate>) -> Option<Predicate> {
    match (outer, inner) {
        (Some(a), Some(b)) => Some(Predicate::And(Box::new(a.clone()), Box::new(b.clone()))),
        (Some(a), None) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (None, None) => None,
    }
}

// ---------------------------------------------------------------------------
// Visitor-based dependency and variable collection
// ---------------------------------------------------------------------------

/// Visitor that collects dirty flags from templates, predicates, and face rules.
struct DepsVisitor<'a> {
    flags: &'a mut DirtyFlags,
}

impl WidgetVisitor for DepsVisitor<'_> {
    fn visit_template(&mut self, template: &Template) {
        for var in template.referenced_variables() {
            *self.flags |= variable_dirty_flag(var);
        }
    }
    fn visit_predicate(&mut self, predicate: &Predicate) {
        for var in predicate.referenced_variables() {
            *self.flags |= variable_dirty_flag(var);
        }
    }
    fn visit_face_rules(&mut self, rules: &[FaceRule]) {
        for rule in rules {
            if let Some(ref cond) = rule.when {
                for var in cond.referenced_variables() {
                    *self.flags |= variable_dirty_flag(var);
                }
            }
        }
    }
    fn visit_line_expr(&mut self, expr: &LineExpr) {
        match expr {
            LineExpr::CursorLine => *self.flags |= DirtyFlags::BUFFER_CURSOR,
            LineExpr::Selection => *self.flags |= DirtyFlags::BUFFER,
        }
    }
}

/// Visitor that collects all variable names for validation.
struct VarCollector {
    vars: Vec<String>,
}

impl WidgetVisitor for VarCollector {
    fn visit_template(&mut self, template: &Template) {
        self.vars.extend(
            template
                .referenced_variables()
                .into_iter()
                .map(String::from),
        );
    }
    fn visit_predicate(&mut self, predicate: &Predicate) {
        self.vars.extend(
            predicate
                .referenced_variables()
                .into_iter()
                .map(String::from),
        );
    }
    fn visit_face_rules(&mut self, rules: &[FaceRule]) {
        for rule in rules {
            if let Some(ref cond) = rule.when {
                self.vars
                    .extend(cond.referenced_variables().into_iter().map(String::from));
            }
        }
    }
}

/// Compute dirty flags for a single widget definition (all effects).
pub fn compute_widget_deps(widget: &WidgetDef) -> DirtyFlags {
    let mut flags = DirtyFlags::empty();
    for effect in &widget.effects {
        compute_widget_deps_inner(&effect.kind, &mut flags);
    }
    // Include shared when condition deps.
    if let Some(ref cond) = widget.when {
        for var in cond.referenced_variables() {
            flags |= variable_dirty_flag(var);
        }
    }
    flags
}

fn compute_deps(widgets: &[WidgetDef]) -> DirtyFlags {
    let mut flags = DirtyFlags::empty();
    for widget in widgets {
        for effect in &widget.effects {
            compute_widget_deps_inner(&effect.kind, &mut flags);
        }
        if let Some(ref cond) = widget.when {
            for var in cond.referenced_variables() {
                flags |= variable_dirty_flag(var);
            }
        }
    }
    flags
}

fn compute_widget_deps_inner(kind: &WidgetKind, flags: &mut DirtyFlags) {
    // Base flags that are always needed for certain kinds.
    match kind {
        WidgetKind::Gutter(_) => *flags |= DirtyFlags::BUFFER_CURSOR,
        WidgetKind::Inline(_) | WidgetKind::VirtualText(_) => *flags |= DirtyFlags::BUFFER,
        _ => {}
    }
    let mut visitor = DepsVisitor { flags };
    walk_widget_kind(kind, &mut visitor);
}

/// Collect all variable names referenced by a widget for validation.
fn collect_widget_variables(kind: &WidgetKind) -> Vec<String> {
    let mut visitor = VarCollector { vars: Vec::new() };
    walk_widget_kind(kind, &mut visitor);
    visitor.vars.sort();
    visitor.vars.dedup();
    visitor.vars
}

/// Widget effect kind names that can appear as child nodes in a multi-effect widget.
const EFFECT_KINDS: &[&str] = &[
    "contribution",
    "background",
    "transform",
    "gutter",
    "inline",
    "virtual-text",
];

/// Parse a KDL node into a widget definition (single or multi-effect).
///
/// Single-effect (traditional): `kind=` attribute selects the type.
/// Multi-effect: child nodes whose names are effect kinds define multiple effects.
fn parse_widget_def(node: &kdl::KdlNode) -> Result<(Vec<WidgetEffect>, Option<Predicate>), String> {
    // Check if this is a multi-effect widget: children include effect-kind nodes.
    let has_effect_children = node.children().is_some_and(|children| {
        children
            .nodes()
            .iter()
            .any(|child| EFFECT_KINDS.contains(&child.name().value()))
    });

    if has_effect_children && get_string_entry(node, "kind").is_none() {
        // Multi-effect widget: shared when + multiple effect children.
        let when = parse_when(node)?;
        let mut effects = Vec::new();
        if let Some(children) = node.children() {
            for child in children.nodes() {
                let child_name = child.name().value();
                if EFFECT_KINDS.contains(&child_name) {
                    let kind = parse_effect_node(child, child_name)?;
                    effects.push(WidgetEffect { kind });
                }
                // Non-effect children (e.g. "part", "branch", "face") are
                // handled within the individual effect parsers, not here.
            }
        }
        if effects.is_empty() {
            return Err("multi-effect widget has no effect children".to_string());
        }
        Ok((effects, when))
    } else {
        // Single-effect widget (traditional).
        let kind = parse_single_effect(node)?;
        Ok((vec![WidgetEffect { kind }], None))
    }
}

/// Parse a single-effect widget node using the `kind=` attribute.
fn parse_single_effect(node: &kdl::KdlNode) -> Result<WidgetKind, String> {
    let kind_str = get_string_entry(node, "kind").unwrap_or("contribution");
    parse_effect_node(node, kind_str)
}

/// Parse an effect node by kind name.
fn parse_effect_node(node: &kdl::KdlNode, kind: &str) -> Result<WidgetKind, String> {
    match kind {
        "contribution" => parse_contribution_node(node),
        "background" => parse_background_node(node),
        "transform" => parse_transform_node(node),
        "gutter" => parse_gutter_node(node),
        "inline" => parse_inline_node(node),
        "virtual-text" => parse_virtual_text_node(node),
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

/// Known slot names for fuzzy matching.
const SLOT_NAMES: &[&str] = &[
    "status-left",
    "status-right",
    "buffer-left",
    "buffer-right",
    "above-buffer",
    "below-buffer",
    "above-status",
];

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
        other => Err(format_unknown_with_suggestion("slot", other, SLOT_NAMES)),
    }
}

/// Known transform target names for fuzzy matching.
const TRANSFORM_TARGET_NAMES: &[&str] = &[
    "status",
    "status-bar",
    "buffer",
    "menu",
    "menu-prompt",
    "menu-inline",
    "menu-search",
    "info",
    "info-prompt",
    "info-modal",
];

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
        other => Err(format_unknown_with_suggestion(
            "transform target",
            other,
            TRANSFORM_TARGET_NAMES,
        )),
    }
}

/// Format an "unknown X" error with a fuzzy suggestion if a close match exists.
fn format_unknown_with_suggestion(kind: &str, input: &str, candidates: &[&str]) -> String {
    let mut best: Option<(&str, usize)> = None;
    for &candidate in candidates {
        let dist = edit_distance(input, candidate);
        if dist <= 3 && (best.is_none() || dist < best.unwrap().1) {
            best = Some((candidate, dist));
        }
    }
    if let Some((suggestion, _)) = best {
        format!("unknown {kind}: '{input}', did you mean '{suggestion}'?")
    } else {
        format!("unknown {kind}: '{input}'")
    }
}

fn parse_inline_node(node: &kdl::KdlNode) -> Result<WidgetKind, String> {
    let pattern_str = get_string_entry(node, "pattern")
        .ok_or_else(|| "inline widget requires 'pattern' attribute".to_string())?;
    let face_spec = get_string_entry(node, "face")
        .ok_or_else(|| "inline widget requires 'face' attribute".to_string())?;
    let face = parse_face_or_token(face_spec)?;
    let when = parse_when(node)?;

    // Detect regex patterns: `/pattern/`.
    let pattern =
        if pattern_str.starts_with('/') && pattern_str.ends_with('/') && pattern_str.len() >= 2 {
            let regex_str = &pattern_str[1..pattern_str.len() - 1];
            let regex = regex_lite::Regex::new(regex_str)
                .map_err(|e| format!("invalid regex pattern: {e}"))?;
            InlinePattern::Regex(std::sync::Arc::new(regex))
        } else {
            InlinePattern::Substring(CompactString::from(pattern_str))
        };

    Ok(WidgetKind::Inline(InlineWidget {
        pattern,
        face,
        when,
    }))
}

fn parse_virtual_text_node(node: &kdl::KdlNode) -> Result<WidgetKind, String> {
    let text = get_string_entry(node, "text")
        .ok_or_else(|| "virtual-text widget requires 'text' attribute".to_string())?;
    let template = Template::parse(text).map_err(|e| format!("template: {e}"))?;
    let face_rules = match get_string_entry(node, "face") {
        Some(spec) => vec![FaceRule {
            face: parse_face_or_token(spec)?,
            when: None,
        }],
        None => Vec::new(),
    };
    let when = parse_when(node)?;

    Ok(WidgetKind::VirtualText(VirtualTextWidget {
        template,
        face_rules,
        when,
    }))
}

fn parse_when(node: &kdl::KdlNode) -> Result<Option<super::types::CondExpr>, String> {
    match get_string_entry(node, "when") {
        Some(expr) => {
            let parsed = parse_condition(expr).map_err(|e| format!("condition: {e}"))?;
            Ok(Some(parsed))
        }
        None => {
            // Handle bare boolean: when=#true / when=#false (KDL bool, not string).
            if let Some(entry) = node.entry("when")
                && let Some(b) = entry.value().as_bool()
            {
                return if b {
                    // when=#true is a no-op (always active)
                    Ok(None)
                } else {
                    // when=#false → always disabled
                    Err("when=false makes widget permanently disabled; \
                          use when=\"false\" (quoted) for a condition, \
                          or remove the widget"
                        .to_string())
                };
            }
            Ok(None)
        }
    }
}

fn parse_order(node: &kdl::KdlNode) -> Option<i16> {
    node.entry("order")
        .and_then(|e| e.value().as_integer())
        .and_then(|v| i16::try_from(v).ok())
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
