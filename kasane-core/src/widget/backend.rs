//! WidgetBackend: PluginBackend implementation for declarative widgets.

use crate::element::{Element, FlexChild, PluginTag};
use crate::plugin::{
    AnnotateContext, AnnotationScope, AppView, BackgroundLayer, BlendMode, CapabilityDescriptor,
    ContribSizeHint, ContributeContext, Contribution, ElementPatch, PluginBackend,
    PluginCapabilities, PluginDiagnostic, PluginDiagnosticKind, PluginDiagnosticTarget, PluginId,
    SlotId, TransformContext, TransformTarget,
};
use crate::protocol::Atom;
use crate::state::DirtyFlags;

use super::parse::{WidgetNodeError, parse_widgets};
use super::types::{ContributionWidget, LineExpr, WidgetFile, WidgetKind, WidgetPatch};
use super::variables::{AppViewResolver, VariableResolver};

const PLUGIN_ID: &str = "kasane.widgets";

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
                    },
                    diagnostics: vec![diagnostic],
                    generation: 1,
                    plugin_tag: PluginTag::UNASSIGNED,
                }
            }
        }
    }

    /// Empty backend (no widgets, no capabilities). Used when widgets.kdl doesn't exist.
    pub fn empty() -> Self {
        Self {
            widgets: WidgetFile {
                widgets: Vec::new(),
                computed_deps: DirtyFlags::empty(),
            },
            diagnostics: Vec::new(),
            generation: 0,
            plugin_tag: PluginTag::UNASSIGNED,
        }
    }

    fn has_contribution(&self) -> bool {
        self.widgets
            .widgets
            .iter()
            .any(|w| matches!(w.kind, WidgetKind::Contribution(_)))
    }

    fn has_background(&self) -> bool {
        self.widgets
            .widgets
            .iter()
            .any(|w| matches!(w.kind, WidgetKind::Background(_)))
    }

    fn has_transform(&self) -> bool {
        self.widgets
            .widgets
            .iter()
            .any(|w| matches!(w.kind, WidgetKind::Transform(_)))
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
        if self.has_background() {
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
        let mut matching: Vec<(u16, Element)> = Vec::new();

        for widget in &self.widgets.widgets {
            let WidgetKind::Contribution(ref contrib) = widget.kind else {
                continue;
            };
            if contrib.slot != *region {
                continue;
            }
            // Evaluate top-level when condition
            if let Some(ref cond) = contrib.when
                && !cond.evaluate(&resolver)
            {
                continue;
            }

            if let Some(element) = build_contribution_element(contrib, &resolver) {
                matching.push((widget.index, element));
            }
        }

        match matching.len() {
            0 => None,
            1 => {
                let (index, element) = matching.into_iter().next().unwrap();
                Some(Contribution {
                    element,
                    priority: index as i16,
                    size_hint: ContribSizeHint::Auto,
                })
            }
            _ => {
                let min_index = matching.iter().map(|(i, _)| *i).min().unwrap_or(0);
                let children: Vec<FlexChild> = matching
                    .into_iter()
                    .map(|(_, el)| FlexChild::fixed(el))
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
            let WidgetKind::Background(ref bg) = widget.kind else {
                continue;
            };

            // Check when condition
            if let Some(ref cond) = bg.when
                && !cond.evaluate(&resolver)
            {
                continue;
            }

            // Check line expression
            match bg.line_expr {
                LineExpr::CursorLine => {
                    let cursor_line = state.cursor_line();
                    if cursor_line >= 0 && line == cursor_line as usize {
                        return Some(BackgroundLayer {
                            face: bg.face,
                            z_order: widget.index as i16,
                            blend: BlendMode::Opaque,
                        });
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
            let WidgetKind::Transform(ref transform) = widget.kind else {
                continue;
            };
            if transform.target != *target {
                continue;
            }

            // Check when condition
            if let Some(ref cond) = transform.when
                && !cond.evaluate(&resolver)
            {
                continue;
            }

            match &transform.patch {
                WidgetPatch::ModifyFace(face) => {
                    return Some(ElementPatch::ModifyFace { overlay: *face });
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
            match &widget.kind {
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

/// Build an Element from a contribution widget's parts.
fn build_contribution_element(
    contrib: &ContributionWidget,
    resolver: &dyn VariableResolver,
) -> Option<Element> {
    let mut atoms: Vec<Atom> = Vec::new();

    for part in &contrib.parts {
        // Check per-part when condition
        if let Some(ref cond) = part.when
            && !cond.evaluate(resolver)
        {
            continue;
        }

        let text = part.template.expand(resolver);
        let face = part.face.unwrap_or_default();
        atoms.push(Atom {
            face,
            contents: text,
        });
    }

    if atoms.is_empty() {
        return None;
    }

    Some(Element::styled_line(atoms))
}

fn node_error_to_diagnostic(error: &WidgetNodeError) -> PluginDiagnostic {
    PluginDiagnostic {
        target: PluginDiagnosticTarget::Plugin(PluginId(PLUGIN_ID.to_string())),
        kind: PluginDiagnosticKind::RuntimeError {
            method: "parse".to_string(),
        },
        message: format!("widget '{}': {}", error.name, error.message),
        previous: None,
        attempted: None,
    }
}
