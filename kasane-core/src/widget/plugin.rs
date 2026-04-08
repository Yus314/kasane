//! WidgetPlugin: Plugin trait implementation for declarative widgets.
//!
//! Each declarative widget becomes a WidgetPlugin, registered via
//! `HandlerRegistry` to participate in the full plugin system.

use compact_str::CompactString;

use crate::plugin::{
    BackgroundLayer, BlendMode, Contribution, ElementPatch, HandlerRegistry, Plugin,
    PluginDiagnostic, PluginId, PluginRuntime, VirtualTextItem, bridge::PluginBridge,
};
use crate::render::InlineDecoration;
use crate::state::DirtyFlags;

use super::backend::{
    build_contribution_element, node_error_to_diagnostic, resolve_face, resolve_face_rules,
};
use super::parse::{WidgetNodeError, compute_widget_deps};
use super::predicate::Predicate;
use super::types::{InlinePattern, LineExpr, WidgetDef, WidgetFile, WidgetKind, WidgetPatch};
use super::variables::{AppViewResolver, LineContextResolver};

/// Prefix for per-widget plugin IDs: `"kasane.widget.<name>"`.
const PLUGIN_PREFIX: &str = "kasane.widget.";

/// Widget state — stateless (all data comes from the widget definition).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WidgetState;

/// A declarative widget effect registered as a Plugin.
///
/// Each effect within a `WidgetDef` becomes its own `WidgetPlugin` instance.
/// For single-effect widgets, the plugin ID is `kasane.widget.<name>`.
/// For multi-effect widgets, the plugin ID is `kasane.widget.<name>.<idx>`.
pub struct WidgetPlugin {
    plugin_id: CompactString,
    kind: WidgetKind,
    /// Shared global when condition from the parent `WidgetDef`.
    shared_when: Option<Predicate>,
    index: u16,
    deps: DirtyFlags,
}

impl WidgetPlugin {
    /// Build the plugin ID for a given widget name (single-effect).
    pub fn plugin_id_for(name: &str) -> PluginId {
        PluginId(format!("{PLUGIN_PREFIX}{name}"))
    }

    /// Create WidgetPlugins from a WidgetDef.
    pub fn from_def(def: WidgetDef) -> Vec<Self> {
        let overall_deps = compute_widget_deps(&def);
        if def.effects.len() == 1 {
            let effect = def.effects.into_iter().next().unwrap();
            vec![Self {
                plugin_id: CompactString::from(format!("{PLUGIN_PREFIX}{}", def.name)),
                kind: effect.kind,
                shared_when: def.when,
                index: def.index,
                deps: overall_deps,
            }]
        } else {
            def.effects
                .into_iter()
                .enumerate()
                .map(|(i, effect)| Self {
                    plugin_id: CompactString::from(format!("{PLUGIN_PREFIX}{}.{i}", def.name)),
                    kind: effect.kind,
                    shared_when: def.when.clone(),
                    index: def.index,
                    deps: overall_deps,
                })
                .collect()
        }
    }
}

impl Plugin for WidgetPlugin {
    type State = WidgetState;

    fn id(&self) -> PluginId {
        PluginId(self.plugin_id.to_string())
    }

    fn register(&self, r: &mut HandlerRegistry<WidgetState>) {
        r.declare_interests(self.deps);

        // Clone shared_when for use in closures.
        let shared_when = self.shared_when.clone();
        let index = self.index;
        match &self.kind {
            WidgetKind::Contribution(contrib) => {
                let contrib = contrib.clone();
                let shared_when = shared_when.clone();
                let slot = contrib.slot.clone();
                r.on_contribute(slot, move |_state, app, _ctx| {
                    let resolver = AppViewResolver::new(app);
                    if let Some(ref cond) = shared_when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return None;
                    }
                    if let Some(ref cond) = contrib.when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return None;
                    }
                    let element = build_contribution_element(&contrib, &resolver, app)?;
                    Some(Contribution {
                        element,
                        priority: index as i16,
                        size_hint: contrib.size_hint,
                    })
                });
            }
            WidgetKind::Background(bg) => {
                let bg = bg.clone();
                let shared_when = shared_when.clone();
                r.on_annotate_background(move |_state, line, app, _ctx| {
                    let resolver = AppViewResolver::new(app);
                    if let Some(ref cond) = shared_when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return None;
                    }
                    if let Some(ref cond) = bg.when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return None;
                    }
                    match bg.line_expr {
                        LineExpr::CursorLine => {
                            let cursor_line = app.cursor_line();
                            if cursor_line >= 0 && line == cursor_line as usize {
                                return Some(BackgroundLayer {
                                    face: resolve_face(&bg.face, app),
                                    z_order: index as i16,
                                    blend: BlendMode::Opaque,
                                });
                            }
                        }
                        LineExpr::Selection => {
                            for sel in app.selections() {
                                let lo = sel.anchor.line.min(sel.cursor.line) as usize;
                                let hi = sel.anchor.line.max(sel.cursor.line) as usize;
                                if line >= lo && line <= hi {
                                    return Some(BackgroundLayer {
                                        face: resolve_face(&bg.face, app),
                                        z_order: index as i16,
                                        blend: BlendMode::Opaque,
                                    });
                                }
                            }
                        }
                    }
                    None
                });
            }
            WidgetKind::Transform(transform) => {
                let transform = transform.clone();
                let shared_when = shared_when.clone();
                let targets = vec![transform.target.clone()];
                r.on_transform_for(index as i16, &targets, move |_state, _target, app, _ctx| {
                    let resolver = AppViewResolver::new(app);
                    if let Some(ref cond) = shared_when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return ElementPatch::Identity;
                    }
                    if let Some(ref cond) = transform.when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return ElementPatch::Identity;
                    }
                    match &transform.patch {
                        WidgetPatch::ModifyFace(rules) => ElementPatch::ModifyFace {
                            overlay: resolve_face_rules(rules, &resolver, app),
                        },
                        WidgetPatch::WrapContainer(rules) => ElementPatch::WrapContainer {
                            face: resolve_face_rules(rules, &resolver, app),
                        },
                    }
                });
            }
            WidgetKind::Gutter(gutter) => {
                let gutter = gutter.clone();
                let shared_when = shared_when.clone();
                r.on_annotate_gutter(gutter.side, index as i16, move |_state, line, app, _ctx| {
                    let app_resolver = AppViewResolver::new(app);
                    if let Some(ref cond) = shared_when
                        && !cond.evaluate_with_resolver(&app_resolver)
                    {
                        return None;
                    }
                    if let Some(ref cond) = gutter.when
                        && !cond.evaluate_with_resolver(&app_resolver)
                    {
                        return None;
                    }
                    let cursor_line = app.cursor_line().max(0) as usize;
                    let line_resolver = LineContextResolver::new(app, line, cursor_line);
                    for branch in &gutter.branches {
                        if let Some(ref cond) = branch.line_when
                            && !cond.evaluate_with_resolver(&line_resolver)
                        {
                            continue;
                        }
                        let text = branch.template.expand(&line_resolver);
                        let face = resolve_face_rules(&branch.face_rules, &line_resolver, app);
                        let element =
                            crate::element::Element::styled_line(vec![crate::protocol::Atom {
                                face,
                                contents: text,
                            }]);
                        return Some(element);
                    }
                    None
                });
            }
            WidgetKind::Inline(inline) => {
                let inline = inline.clone();
                let shared_when = shared_when.clone();
                r.on_annotate_inline(move |_state, line, app, _ctx| {
                    let resolver = AppViewResolver::new(app);
                    if let Some(ref cond) = shared_when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return None;
                    }
                    if let Some(ref cond) = inline.when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return None;
                    }
                    build_inline_decoration(&inline.pattern, &inline.face, line, app)
                });
            }
            WidgetKind::VirtualText(vt) => {
                let vt = vt.clone();
                let shared_when = shared_when.clone();
                r.on_virtual_text(move |_state, _line, app, _ctx| {
                    let resolver = AppViewResolver::new(app);
                    if let Some(ref cond) = shared_when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return Vec::new();
                    }
                    if let Some(ref cond) = vt.when
                        && !cond.evaluate_with_resolver(&resolver)
                    {
                        return Vec::new();
                    }
                    let text = vt.template.expand(&resolver);
                    if text.is_empty() {
                        return Vec::new();
                    }
                    let face = resolve_face_rules(&vt.face_rules, &resolver, app);
                    vec![VirtualTextItem {
                        atoms: vec![crate::protocol::Atom {
                            face,
                            contents: text,
                        }],
                        priority: index as i16,
                    }]
                });
            }
        }
    }
}

/// Build an `InlineDecoration` for a single line by finding all non-overlapping
/// pattern matches and applying a face override to each.
fn build_inline_decoration(
    pattern: &InlinePattern,
    face_or_token: &super::types::FaceOrToken,
    line: usize,
    app: &crate::plugin::AppView<'_>,
) -> Option<InlineDecoration> {
    let lines = app.lines();
    if line >= lines.len() {
        return None;
    }

    let face = resolve_face(face_or_token, app);

    // Concatenate atom contents to get line text.
    let line_atoms = &lines[line];
    let mut full_text = String::new();
    for atom in line_atoms {
        full_text.push_str(&atom.contents);
    }

    if full_text.is_empty() {
        return None;
    }

    let mut ops = Vec::new();
    match pattern {
        InlinePattern::Substring(pat) => {
            if pat.is_empty() {
                return None;
            }
            let mut search_start = 0;
            while let Some(pos) = full_text[search_start..].find(pat.as_str()) {
                let byte_start = search_start + pos;
                let byte_end = byte_start + pat.len();
                ops.push(crate::render::InlineOp::Style {
                    range: byte_start..byte_end,
                    face,
                });
                search_start = byte_end;
            }
        }
        InlinePattern::Regex(re) => {
            for m in re.find_iter(&full_text) {
                ops.push(crate::render::InlineOp::Style {
                    range: m.start()..m.end(),
                    face,
                });
            }
        }
    }

    if ops.is_empty() {
        None
    } else {
        Some(InlineDecoration::new(ops))
    }
}

/// Register all widgets from a parsed WidgetFile as individual plugins.
///
/// Each effect within a widget becomes a separate `WidgetPlugin` registered
/// via `PluginBridge`. Returns the set of plugin IDs (for use in hot-reloads).
pub fn register_all_widgets(
    file: WidgetFile,
    errors: &[WidgetNodeError],
    registry: &mut PluginRuntime,
) -> Vec<String> {
    let diagnostics: Vec<PluginDiagnostic> = errors.iter().map(node_error_to_diagnostic).collect();

    let mut plugin_ids: Vec<String> = Vec::new();
    let mut first = true;

    for def in file.widgets {
        for wp in WidgetPlugin::from_def(def) {
            plugin_ids.push(wp.plugin_id.to_string());
            let bridge = if first && !diagnostics.is_empty() {
                first = false;
                PluginBridge::new(wp).with_diagnostics(diagnostics.clone())
            } else {
                first = false;
                PluginBridge::new(wp)
            };
            registry.register_backend(Box::new(bridge));
        }
    }

    plugin_ids
}

/// Hot-reload widgets: diff old vs. new, register/replace/unload as needed.
///
/// Returns the new set of plugin IDs.
pub fn hot_reload_widgets(
    old_ids: &[String],
    file: WidgetFile,
    errors: &[WidgetNodeError],
    registry: &mut PluginRuntime,
) -> Vec<String> {
    let diagnostics: Vec<PluginDiagnostic> = errors.iter().map(node_error_to_diagnostic).collect();

    let mut new_ids: Vec<String> = Vec::new();
    let mut first = true;

    for def in file.widgets {
        for wp in WidgetPlugin::from_def(def) {
            new_ids.push(wp.plugin_id.to_string());
            let bridge = if first && !diagnostics.is_empty() {
                first = false;
                PluginBridge::new(wp).with_diagnostics(diagnostics.clone())
            } else {
                first = false;
                PluginBridge::new(wp)
            };
            registry.register_backend(Box::new(bridge));
        }
    }

    // Remove plugins that are no longer present
    for old_id in old_ids {
        if !new_ids.iter().any(|n| n == old_id) {
            let id = PluginId(old_id.clone());
            registry.remove_plugin(&id);
        }
    }

    new_ids
}
