//! SingleWidgetBackend: one PluginBackend per widget definition.
//!
//! Each declarative widget gets its own plugin slot in the registry,
//! enabling per-widget DirtyFlags gating and independent caching.

use compact_str::CompactString;

use crate::element::{Element, PluginTag};
use crate::plugin::{
    AnnotateContext, AnnotationScope, AppView, BackgroundLayer, BlendMode, CapabilityDescriptor,
    ContributeContext, Contribution, ElementPatch, GutterSide, PluginBackend, PluginCapabilities,
    PluginDiagnostic, PluginId, SlotId, TransformContext, TransformTarget,
};
use crate::protocol::Atom;
use crate::state::DirtyFlags;

use crate::plugin::PluginRuntime;

use super::backend::{
    build_contribution_element, node_error_to_diagnostic, resolve_face, resolve_face_rules,
};
use super::parse::{WidgetNodeError, compute_widget_deps};
use super::types::{LineExpr, WidgetDef, WidgetFile, WidgetKind, WidgetPatch};
use super::variables::{AppViewResolver, LineContextResolver};

/// Prefix for per-widget plugin IDs: `"kasane.widget.<name>"`.
const PLUGIN_PREFIX: &str = "kasane.widget.";

/// A single widget registered as its own PluginBackend.
pub struct SingleWidgetBackend {
    name: CompactString,
    def: WidgetDef,
    deps: DirtyFlags,
    generation: u64,
    plugin_tag: PluginTag,
    diagnostics: Vec<PluginDiagnostic>,
}

impl SingleWidgetBackend {
    pub fn new(def: WidgetDef, generation: u64) -> Self {
        let name = def.name.clone();
        let deps = compute_widget_deps(&def);
        Self {
            name,
            def,
            deps,
            generation,
            plugin_tag: PluginTag::UNASSIGNED,
            diagnostics: Vec::new(),
        }
    }

    pub fn with_diagnostics(mut self, diagnostics: Vec<PluginDiagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// The widget name (used to derive the plugin ID).
    pub fn widget_name(&self) -> &str {
        &self.name
    }

    /// Build the plugin ID for a given widget name.
    pub fn plugin_id_for(name: &str) -> PluginId {
        PluginId(format!("{PLUGIN_PREFIX}{name}"))
    }
}

impl PluginBackend for SingleWidgetBackend {
    fn id(&self) -> PluginId {
        Self::plugin_id_for(&self.name)
    }

    fn set_plugin_tag(&mut self, tag: PluginTag) {
        self.plugin_tag = tag;
    }

    fn capabilities(&self) -> PluginCapabilities {
        let mut caps = PluginCapabilities::empty();
        match &self.def.kind {
            WidgetKind::Contribution(_) => caps |= PluginCapabilities::CONTRIBUTOR,
            WidgetKind::Background(_) | WidgetKind::Gutter(_) => {
                caps |= PluginCapabilities::ANNOTATOR
            }
            WidgetKind::Transform(_) => caps |= PluginCapabilities::TRANSFORMER,
        }
        caps
    }

    fn state_hash(&self) -> u64 {
        self.generation
    }

    fn view_deps(&self) -> DirtyFlags {
        self.deps
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
        let WidgetKind::Contribution(contrib) = &self.def.kind else {
            return None;
        };
        if contrib.slot != *region {
            return None;
        }

        let resolver = AppViewResolver::new(state);
        if let Some(ref cond) = contrib.when
            && !cond.evaluate(&resolver)
        {
            return None;
        }

        let element = build_contribution_element(contrib, &resolver, state)?;
        Some(Contribution {
            element,
            priority: self.def.index as i16,
            size_hint: contrib.size_hint,
        })
    }

    fn annotate_background(
        &self,
        line: usize,
        state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<BackgroundLayer> {
        let WidgetKind::Background(bg) = &self.def.kind else {
            return None;
        };

        let resolver = AppViewResolver::new(state);
        if let Some(ref cond) = bg.when
            && !cond.evaluate(&resolver)
        {
            return None;
        }

        match bg.line_expr {
            LineExpr::CursorLine => {
                let cursor_line = state.cursor_line();
                if cursor_line >= 0 && line == cursor_line as usize {
                    return Some(BackgroundLayer {
                        face: resolve_face(&bg.face, state),
                        z_order: self.def.index as i16,
                        blend: BlendMode::Opaque,
                    });
                }
            }
            LineExpr::Selection => {
                for sel in state.selections() {
                    let lo = sel.anchor.line.min(sel.cursor.line) as usize;
                    let hi = sel.anchor.line.max(sel.cursor.line) as usize;
                    if line >= lo && line <= hi {
                        return Some(BackgroundLayer {
                            face: resolve_face(&bg.face, state),
                            z_order: self.def.index as i16,
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
        let WidgetKind::Transform(transform) = &self.def.kind else {
            return None;
        };
        if transform.target != *target {
            return None;
        }

        let resolver = AppViewResolver::new(state);
        if let Some(ref cond) = transform.when
            && !cond.evaluate(&resolver)
        {
            return None;
        }

        match &transform.patch {
            WidgetPatch::ModifyFace(rules) => Some(ElementPatch::ModifyFace {
                overlay: resolve_face_rules(rules, &resolver, state),
            }),
            WidgetPatch::WrapContainer(rules) => Some(ElementPatch::WrapContainer {
                face: resolve_face_rules(rules, &resolver, state),
            }),
        }
    }

    fn annotate_gutter(
        &self,
        side: GutterSide,
        line: usize,
        state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<(i16, Element)> {
        let WidgetKind::Gutter(gutter) = &self.def.kind else {
            return None;
        };
        if gutter.side != side {
            return None;
        }

        let app_resolver = AppViewResolver::new(state);
        if let Some(ref cond) = gutter.when
            && !cond.evaluate(&app_resolver)
        {
            return None;
        }

        let cursor_line = state.cursor_line().max(0) as usize;
        let line_resolver = LineContextResolver::new(state, line, cursor_line);

        for branch in &gutter.branches {
            if let Some(ref cond) = branch.line_when
                && !cond.evaluate(&line_resolver)
            {
                continue;
            }

            let text = branch.template.expand(&line_resolver);
            let face = resolve_face_rules(&branch.face_rules, &line_resolver, state);
            let element = Element::styled_line(vec![Atom {
                face,
                contents: text,
            }]);
            return Some((self.def.index as i16, element));
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

        match &self.def.kind {
            WidgetKind::Contribution(c) => {
                slots.push(c.slot.clone());
            }
            WidgetKind::Background(_) => {
                annotation_scopes.push(AnnotationScope::Background);
            }
            WidgetKind::Transform(t) => {
                targets.push(t.target.clone());
            }
            WidgetKind::Gutter(g) => {
                annotation_scopes.push(match g.side {
                    GutterSide::Left => AnnotationScope::LeftGutter,
                    GutterSide::Right => AnnotationScope::RightGutter,
                });
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

/// Register all widgets from a parsed WidgetFile as individual plugins.
///
/// Each widget becomes its own `SingleWidgetBackend` with independent
/// `view_deps()` and `state_hash()`. Returns the set of widget names
/// registered (for use in subsequent hot-reloads).
pub fn register_all_widgets(
    file: WidgetFile,
    errors: &[WidgetNodeError],
    registry: &mut PluginRuntime,
) -> Vec<String> {
    let diagnostics: Vec<PluginDiagnostic> = errors.iter().map(node_error_to_diagnostic).collect();

    let names: Vec<String> = file.widgets.iter().map(|w| w.name.to_string()).collect();

    for (i, def) in file.widgets.into_iter().enumerate() {
        let backend = if i == 0 && !diagnostics.is_empty() {
            SingleWidgetBackend::new(def, 1).with_diagnostics(diagnostics.clone())
        } else {
            SingleWidgetBackend::new(def, 1)
        };
        registry.register_backend(Box::new(backend));
    }

    names
}

/// Hot-reload widgets: diff old vs. new, register/replace/unload as needed.
///
/// - Same name → `register_backend()` replaces in-place (preserves plugin tag)
/// - New name → `register_backend()` creates a new slot
/// - Removed name → `remove_plugin()` drops the slot
///
/// Returns the new set of widget names.
pub fn hot_reload_widgets(
    old_names: &[String],
    file: WidgetFile,
    errors: &[WidgetNodeError],
    registry: &mut PluginRuntime,
) -> Vec<String> {
    let diagnostics: Vec<PluginDiagnostic> = errors.iter().map(node_error_to_diagnostic).collect();

    let new_names: Vec<String> = file.widgets.iter().map(|w| w.name.to_string()).collect();

    // Each re-registration bumps via register_backend's HASH_SENTINEL reset anyway.
    for (i, def) in file.widgets.into_iter().enumerate() {
        let backend = if i == 0 && !diagnostics.is_empty() {
            SingleWidgetBackend::new(def, 2).with_diagnostics(diagnostics.clone())
        } else {
            SingleWidgetBackend::new(def, 2)
        };
        registry.register_backend(Box::new(backend));
    }

    // Remove widgets that are no longer present
    for old_name in old_names {
        if !new_names.iter().any(|n| n == old_name) {
            let id = SingleWidgetBackend::plugin_id_for(old_name);
            registry.remove_plugin(&id);
        }
    }

    new_names
}
