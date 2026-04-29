//! Shared widget evaluation functions and legacy `WidgetBackend` (test-only).

use crate::element::{Element, ElementStyle};
use crate::plugin::{AppView, PluginDiagnostic, PluginId};
use crate::protocol::{Atom, Face, Style};

use super::parse::WidgetNodeError;
use super::types::{ContributionWidget, FaceOrToken, FaceRule};
use super::variables::VariableResolver;

const PLUGIN_ID: &str = "kasane.widgets";

/// Convert a `FaceOrToken` to a `Style` without resolving tokens against the theme.
pub(super) fn to_style(face_or_token: &FaceOrToken) -> ElementStyle {
    match face_or_token {
        FaceOrToken::Direct(face) => ElementStyle::from(*face),
        FaceOrToken::Token(token) => ElementStyle::Token(token.clone()),
    }
}

/// Resolve a `FaceOrToken` to a concrete `Face` using the current theme.
pub(super) fn resolve_face(face_or_token: &FaceOrToken, state: &AppView<'_>) -> Face {
    match face_or_token {
        FaceOrToken::Direct(face) => *face,
        FaceOrToken::Token(token) => state
            .theme_style(token)
            .map(|s| s.to_face())
            .unwrap_or_default(),
    }
}

/// Evaluate face rules and return the face for the first matching rule.
pub(super) fn resolve_face_rules(
    rules: &[FaceRule],
    resolver: &dyn VariableResolver,
    state: &AppView<'_>,
) -> Face {
    for rule in rules {
        if rule
            .when
            .as_ref()
            .is_none_or(|c| c.evaluate_with_resolver(resolver))
        {
            return resolve_face(&rule.face, state);
        }
    }
    Face::default()
}

/// Try to resolve face rules to a `Style` without eagerly resolving tokens.
///
/// Returns `Some(style)` if the rules have exactly one unconditional entry,
/// preserving `Style::Token` for deferred theme resolution. Returns `None`
/// if the rules require conditional evaluation (caller should fall back to
/// `resolve_face_rules`).
fn try_resolve_style(rules: &[FaceRule]) -> Option<ElementStyle> {
    match rules {
        [single] if single.when.is_none() => Some(to_style(&single.face)),
        _ => None,
    }
}

/// Build an Element from a contribution widget's parts.
pub(super) fn build_contribution_element(
    contrib: &ContributionWidget,
    resolver: &dyn VariableResolver,
    state: &AppView<'_>,
) -> Option<Element> {
    let mut atoms: Vec<Atom> = Vec::new();

    for part in &contrib.parts {
        // Check per-part when condition
        if let Some(ref cond) = part.when
            && !cond.evaluate_with_resolver(resolver)
        {
            continue;
        }

        let text = part.template.expand(resolver);
        let face = resolve_face_rules(&part.face_rules, resolver, state);
        atoms.push(Atom::with_style(text, Style::from_face(&face)));
    }

    if atoms.is_empty() {
        return None;
    }

    // Single-atom optimization: if the sole part has an unconditional Token face,
    // emit Element::Text with Style::Token so the paint phase resolves it via the
    // theme. This avoids eagerly resolving the token here and allows theme changes
    // to take effect without re-evaluating the widget.
    if atoms.len() == 1 {
        let active_parts: Vec<_> = contrib
            .parts
            .iter()
            .filter(|p| {
                p.when
                    .as_ref()
                    .is_none_or(|c| c.evaluate_with_resolver(resolver))
            })
            .collect();
        if active_parts.len() == 1
            && let Some(style) = try_resolve_style(&active_parts[0].face_rules)
        {
            let atom = atoms.into_iter().next().unwrap();
            return Some(Element::Text(atom.contents, style));
        }
    }

    Some(Element::styled_line(atoms))
}

pub fn node_error_to_diagnostic(error: &WidgetNodeError) -> PluginDiagnostic {
    PluginDiagnostic::config_error(PluginId(PLUGIN_ID.to_string()), &error.name, &error.message)
}

// ---------------------------------------------------------------------------
// Legacy monolithic WidgetBackend — kept only for existing tests.
// New code should use WidgetPlugin (plugin.rs) which registers via HandlerRegistry.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(super) use legacy::WidgetBackend;

#[cfg(test)]
mod legacy {
    use crate::element::{Element, FlexChild, PluginTag};
    use crate::plugin::{
        AnnotateContext, AnnotationScope, AppView, BackgroundLayer, BlendMode,
        CapabilityDescriptor, ContribSizeHint, ContributeContext, Contribution, ElementPatch,
        GutterSide, PluginBackend, PluginCapabilities, PluginDiagnostic, PluginDiagnosticKind,
        PluginDiagnosticTarget, PluginId, SlotId, TransformContext, TransformTarget,
    };
    use crate::state::DirtyFlags;

    use super::super::parse::parse_widgets;
    use super::super::types::{LineExpr, WidgetFile, WidgetKind, WidgetPatch};
    use super::super::variables::{AppViewResolver, LineContextResolver};
    use super::{
        PLUGIN_ID, build_contribution_element, node_error_to_diagnostic, resolve_face,
        resolve_face_rules,
    };

    /// Declarative widget backend implementing PluginBackend.
    pub struct WidgetBackend {
        widgets: WidgetFile,
        diagnostics: Vec<PluginDiagnostic>,
        generation: u64,
        plugin_tag: PluginTag,
    }

    impl WidgetBackend {
        /// Parse KDL source into a WidgetBackend. Parse errors become diagnostics.
        pub fn from_source(source: &str) -> Self {
            match parse_widgets(source) {
                Ok((file, errors)) => {
                    let diagnostics: Vec<PluginDiagnostic> =
                        errors.iter().map(node_error_to_diagnostic).collect();
                    if !errors.is_empty() {
                        tracing::warn!(
                            count = errors.len(),
                            "widget parse: {} nodes skipped",
                            errors.len()
                        );
                    }
                    Self {
                        widgets: file,
                        diagnostics,
                        generation: 1,
                        plugin_tag: PluginTag::UNASSIGNED,
                    }
                }
                Err(e) => {
                    let diagnostic = PluginDiagnostic {
                        target: PluginDiagnosticTarget::Plugin(PluginId(PLUGIN_ID.to_string())),
                        kind: PluginDiagnosticKind::RuntimeError {
                            method: "parse".to_string(),
                        },
                        message: e.to_string(),
                        previous: None,
                        attempted: None,
                    };
                    Self {
                        widgets: WidgetFile {
                            widgets: Vec::new(),
                            computed_deps: DirtyFlags::empty(),
                            included_paths: Vec::new(),
                        },
                        diagnostics: vec![diagnostic],
                        generation: 1,
                        plugin_tag: PluginTag::UNASSIGNED,
                    }
                }
            }
        }

        /// Create from a pre-parsed [`WidgetFile`] (used by unified kasane.kdl startup).
        pub fn from_widgets(file: WidgetFile) -> Self {
            Self {
                widgets: file,
                diagnostics: Vec::new(),
                generation: 1,
                plugin_tag: PluginTag::UNASSIGNED,
            }
        }

        /// Reload widgets from new source, keeping previous state on parse failure.
        ///
        /// Returns `true` if the new source was accepted, `false` if it was rejected
        /// (in which case the previous widgets remain active).
        pub fn reload_from_source(&mut self, source: &str) -> bool {
            match parse_widgets(source) {
                Ok((file, errors)) => {
                    let diagnostics: Vec<PluginDiagnostic> =
                        errors.iter().map(node_error_to_diagnostic).collect();
                    if !errors.is_empty() {
                        tracing::warn!(
                            count = errors.len(),
                            "widget reload: {} nodes skipped",
                            errors.len()
                        );
                    }
                    self.widgets = file;
                    self.diagnostics = diagnostics;
                    self.generation += 1;
                    true
                }
                Err(e) => {
                    tracing::warn!("widget reload rejected (keeping previous): {e}");
                    self.diagnostics.push(PluginDiagnostic {
                        target: PluginDiagnosticTarget::Plugin(PluginId(PLUGIN_ID.to_string())),
                        kind: PluginDiagnosticKind::RuntimeError {
                            method: "reload".to_string(),
                        },
                        message: format!("reload rejected (keeping previous): {e}"),
                        previous: None,
                        attempted: None,
                    });
                    false
                }
            }
        }

        /// Reload from a pre-parsed [`WidgetFile`] (used by unified kasane.kdl hot-reload).
        pub fn reload_from_widgets(&mut self, file: WidgetFile) {
            self.widgets = file;
            self.diagnostics.clear();
            self.generation += 1;
        }

        /// Empty backend (no widgets, no capabilities). Used when kasane.kdl doesn't exist.
        pub fn empty() -> Self {
            Self {
                widgets: WidgetFile {
                    widgets: Vec::new(),
                    computed_deps: DirtyFlags::empty(),
                    included_paths: Vec::new(),
                },
                diagnostics: Vec::new(),
                generation: 0,
                plugin_tag: PluginTag::UNASSIGNED,
            }
        }

        fn has_kind(&self, pred: impl Fn(&WidgetKind) -> bool) -> bool {
            self.widgets
                .widgets
                .iter()
                .flat_map(|w| &w.effects)
                .any(|e| pred(&e.kind))
        }

        fn has_contribution(&self) -> bool {
            self.has_kind(|k| matches!(k, WidgetKind::Contribution(_)))
        }

        fn has_background(&self) -> bool {
            self.has_kind(|k| matches!(k, WidgetKind::Background(_)))
        }

        fn has_transform(&self) -> bool {
            self.has_kind(|k| matches!(k, WidgetKind::Transform(_)))
        }

        fn has_gutter(&self) -> bool {
            self.has_kind(|k| matches!(k, WidgetKind::Gutter(_)))
        }
    }

    impl PluginBackend for WidgetBackend {
        fn id(&self) -> PluginId {
            PluginId(PLUGIN_ID.to_string())
        }

        fn set_plugin_tag(&mut self, tag: PluginTag) {
            self.plugin_tag = tag;
        }

        fn capabilities(&self) -> PluginCapabilities {
            let mut caps = PluginCapabilities::empty();
            if self.has_contribution() {
                caps |= PluginCapabilities::CONTRIBUTOR;
            }
            if self.has_background() || self.has_gutter() {
                caps |= PluginCapabilities::ANNOTATOR;
            }
            if self.has_transform() {
                caps |= PluginCapabilities::TRANSFORMER;
            }
            caps
        }

        fn state_hash(&self) -> u64 {
            self.generation
        }

        fn view_deps(&self) -> DirtyFlags {
            self.widgets.computed_deps
        }

        fn has_decomposed_annotations(&self) -> bool {
            true
        }

        fn contribute_to(
            &self,
            region: &SlotId,
            state: &AppView<'_>,
            _ctx: &ContributeContext,
        ) -> Option<Contribution> {
            let resolver = AppViewResolver::new(state);

            // Collect all contribution widgets matching this slot
            let mut matching: Vec<(i16, ContribSizeHint, Element)> = Vec::new();

            for widget in &self.widgets.widgets {
                if let Some(ref cond) = widget.when
                    && !cond.evaluate_with_resolver(&resolver)
                {
                    continue;
                }
                for effect in &widget.effects {
                    let WidgetKind::Contribution(ref contrib) = effect.kind else {
                        continue;
                    };
                    if contrib.slot != *region {
                        continue;
                    }
                    if let Some(ref cond) = contrib.when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        continue;
                    }

                    if let Some(element) = build_contribution_element(contrib, &resolver, state) {
                        matching.push((widget.priority(), contrib.size_hint, element));
                    }
                }
            }

            match matching.len() {
                0 => None,
                1 => {
                    let (index, size_hint, element) = matching.into_iter().next().unwrap();
                    Some(Contribution {
                        element,
                        priority: index as i16,
                        size_hint,
                    })
                }
                _ => {
                    let min_index = matching.iter().map(|(i, _, _)| *i).min().unwrap_or(0);
                    let children: Vec<FlexChild> = matching
                        .into_iter()
                        .map(|(_, _, el)| FlexChild::fixed(el))
                        .collect();
                    Some(Contribution {
                        element: Element::row(children),
                        priority: min_index as i16,
                        size_hint: ContribSizeHint::Auto,
                    })
                }
            }
        }

        fn annotate_background(
            &self,
            line: usize,
            state: &AppView<'_>,
            _ctx: &AnnotateContext,
        ) -> Option<BackgroundLayer> {
            let resolver = AppViewResolver::new(state);

            for widget in &self.widgets.widgets {
                if let Some(ref cond) = widget.when
                    && !cond.evaluate_with_resolver(&resolver)
                {
                    continue;
                }
                for effect in &widget.effects {
                    let WidgetKind::Background(ref bg) = effect.kind else {
                        continue;
                    };

                    if let Some(ref cond) = bg.when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        continue;
                    }

                    match bg.line_expr {
                        LineExpr::CursorLine => {
                            let cursor_line = state.cursor_line();
                            if cursor_line >= 0 && line == cursor_line as usize {
                                return Some(BackgroundLayer {
                                    style: crate::protocol::Style::from_face(&resolve_face(
                                        &bg.face, state,
                                    )),
                                    z_order: widget.priority(),
                                    blend: BlendMode::Opaque,
                                });
                            }
                        }
                        LineExpr::Selection => {
                            for sel in state.inference().selections() {
                                let lo = sel.anchor.line.min(sel.cursor.line) as usize;
                                let hi = sel.anchor.line.max(sel.cursor.line) as usize;
                                if line >= lo && line <= hi {
                                    return Some(BackgroundLayer {
                                        style: crate::protocol::Style::from_face(&resolve_face(
                                            &bg.face, state,
                                        )),
                                        z_order: widget.priority(),
                                        blend: BlendMode::Opaque,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            None
        }

        fn transform_patch(
            &self,
            target: &TransformTarget,
            state: &AppView<'_>,
            _ctx: &TransformContext,
        ) -> Option<ElementPatch> {
            let resolver = AppViewResolver::new(state);

            for widget in &self.widgets.widgets {
                if let Some(ref cond) = widget.when
                    && !cond.evaluate_with_resolver(&resolver)
                {
                    continue;
                }
                for effect in &widget.effects {
                    let WidgetKind::Transform(ref transform) = effect.kind else {
                        continue;
                    };
                    if transform.target != *target {
                        continue;
                    }

                    if let Some(ref cond) = transform.when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        continue;
                    }

                    match &transform.patch {
                        WidgetPatch::ModifyFace(rules) => {
                            return Some(ElementPatch::ModifyStyle {
                                overlay: std::sync::Arc::new(
                                    crate::protocol::UnresolvedStyle::from_face(
                                        &resolve_face_rules(rules, &resolver, state),
                                    ),
                                ),
                            });
                        }
                        WidgetPatch::WrapContainer(rules) => {
                            return Some(ElementPatch::WrapContainer {
                                style: std::sync::Arc::new(
                                    crate::protocol::UnresolvedStyle::from_face(
                                        &resolve_face_rules(rules, &resolver, state),
                                    ),
                                ),
                            });
                        }
                    }
                }
            }

            None
        }

        fn annotate_gutter(
            &self,
            side: GutterSide,
            line: usize,
            state: &AppView<'_>,
            _ctx: &AnnotateContext,
        ) -> Option<(i16, Element)> {
            let app_resolver = AppViewResolver::new(state);
            let cursor_line = state.cursor_line().max(0) as usize;

            for widget in &self.widgets.widgets {
                if let Some(ref cond) = widget.when
                    && !cond.evaluate_with_resolver(&app_resolver)
                {
                    continue;
                }
                for effect in &widget.effects {
                    let WidgetKind::Gutter(ref gutter) = effect.kind else {
                        continue;
                    };
                    if gutter.side != side {
                        continue;
                    }

                    if let Some(ref cond) = gutter.when
                        && !cond.evaluate_with_resolver(&app_resolver)
                    {
                        continue;
                    }

                    let line_resolver = LineContextResolver::new(state, line, cursor_line);
                    for branch in &gutter.branches {
                        if let Some(ref cond) = branch.line_when
                            && !cond.evaluate_with_resolver(&line_resolver)
                        {
                            continue;
                        }

                        let text = branch.template.expand(&line_resolver);
                        let face = resolve_face_rules(&branch.face_rules, &line_resolver, state);
                        let element =
                            Element::styled_line(vec![crate::protocol::Atom::with_style(
                                text,
                                crate::protocol::Style::from_face(&face),
                            )]);
                        return Some((widget.priority(), element));
                    }
                }
            }

            None
        }

        fn drain_diagnostics(&mut self) -> Vec<PluginDiagnostic> {
            std::mem::take(&mut self.diagnostics)
        }

        fn capability_descriptor(&self) -> Option<CapabilityDescriptor> {
            let mut slots = Vec::new();
            let mut targets = Vec::new();
            let mut annotation_scopes = Vec::new();

            for widget in &self.widgets.widgets {
                for effect in &widget.effects {
                    match &effect.kind {
                        WidgetKind::Contribution(c) => {
                            if !slots.contains(&c.slot) {
                                slots.push(c.slot.clone());
                            }
                        }
                        WidgetKind::Background(_) => {
                            let scope = AnnotationScope::Background;
                            if !annotation_scopes.contains(&scope) {
                                annotation_scopes.push(scope);
                            }
                        }
                        WidgetKind::Transform(t) => {
                            if !targets.contains(&t.target) {
                                targets.push(t.target.clone());
                            }
                        }
                        WidgetKind::Gutter(g) => {
                            let scope = match g.side {
                                GutterSide::Left => AnnotationScope::LeftGutter,
                                GutterSide::Right => AnnotationScope::RightGutter,
                            };
                            if !annotation_scopes.contains(&scope) {
                                annotation_scopes.push(scope);
                            }
                        }
                        WidgetKind::Inline(_) | WidgetKind::VirtualText(_) => {
                            // Legacy backend doesn't implement inline/virtual-text;
                            // these are handled by WidgetPlugin only.
                        }
                    }
                }
            }

            Some(CapabilityDescriptor {
                contribution_slots: slots,
                transform_targets: targets,
                annotation_scopes,
                ..CapabilityDescriptor::default()
            })
        }
    }
}
