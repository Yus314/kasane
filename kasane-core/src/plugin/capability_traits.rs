//! Capability-scoped trait skeletons that will gradually replace the
//! 73-method [`PluginBackend`](super::PluginBackend) god-trait.
//!
//! Each trait here covers a single concern that PluginBackend currently
//! bundles together. They are introduced as forward-looking targets so
//! later R1.x sub-phases can migrate handlers individually without
//! perturbing the runtime dispatch surface.
//!
//! **Status: scaffolding only.** Nothing in the runtime calls these
//! traits yet — `PluginBackend` remains the dispatch ABI. Phase R1.2
//! starts wiring `Lifecycle` through the registry, R1.3 picks up
//! `Annotator`, etc. The order is intentional: small, well-bounded
//! traits land first so the migration pattern is established before
//! tackling `InputHandler` (the largest concern).
//!
//! ## Why 11 traits?
//!
//! `PluginBackend`'s 73 methods cluster into roughly 11 cohesive
//! concerns. Splitting them means:
//!
//! - new extension points stay confined to the trait that owns the
//!   concern (today they touch 5+ files);
//! - WASM adapters can implement only the capabilities they advertise
//!   instead of stubbing all 73 methods;
//! - tests can supply minimal trait objects rather than hand-rolling
//!   a full `PluginBackend` mock;
//! - capability bitflags become a derivation of "which traits are
//!   non-default" rather than a hand-maintained sidecar.
//!
//! ## Method signatures
//!
//! Signatures here mirror the corresponding methods on `PluginBackend`.
//! Default implementations return the same "no-op" values, so a
//! `PluginBackend`-shaped impl that opts into one of these traits stays
//! semantically identical.
//!
//! `Any + 'static` bounds are inherited so the runtime can downcast
//! trait objects through `HandlerTable` once R1.2+ starts using them.

use std::any::Any;
use std::collections::HashSet;

use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use crate::display::unit::DisplayUnit;
use crate::display::{ContentAnnotation, DisplayDirective, ProjectionDescriptor, ProjectionId};
use crate::element::{Element, InteractiveId, Overlay};
use crate::input::{DropEvent, KeyEvent, MouseEvent};
use crate::layout::Rect;
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::channel::ChannelValue;
use super::extension_point::{ExtensionDefinition, ExtensionOutput, ExtensionPointId};
use super::pubsub::TopicBus;
use super::{
    AnnotateContext, AppView, BackgroundLayer, BuiltinTarget, Command, ContributeContext,
    Contribution, Effects, ElementPatch, GutterSide, IoEvent, KeyHandleResult,
    KeyPreDispatchResult, LineAnnotation, MousePreDispatchResult, OrnamentBatch, OverlayContext,
    OverlayContribution, PluginAuthorities, PluginDiagnostic, PluginId, PluginView,
    RenderOrnamentContext, SlotId, TextInputPreDispatchResult, TransformContext,
    TransformDescriptor, TransformSubject, TransformTarget, VirtualTextItem,
};

// =============================================================================
// 1. Lifecycle — bootstrap, shutdown, persistence, state-change broadcast.
// =============================================================================

/// Bootstrap and shutdown hooks plus state-change observation.
///
/// Plugins implement this to react to the framework lifecycle without
/// interpreting input. `on_state_changed_effects` fires whenever any
/// [`DirtyFlags`] match the plugin's declared interests.
///
/// **Method names match `PluginBackend` exactly** so the blanket impl
/// below can delegate without translation. Once R1.3 moves the actual
/// bodies, we can rename the post-`_effects` suffix away.
pub trait Lifecycle: Any {
    fn on_init_effects(&mut self, _state: &AppView<'_>) -> Effects {
        Effects::default()
    }
    fn on_active_session_ready_effects(&mut self, _state: &AppView<'_>) -> Effects {
        Effects::default()
    }
    fn on_shutdown(&mut self) {}

    /// Serialize plugin state for hot-reload persistence. `None` means
    /// the plugin has no persistable state.
    fn persist_state(&self) -> Option<Vec<u8>> {
        None
    }
    /// Restore plugin state from a prior `persist_state()` payload.
    /// Returns `true` on success.
    fn restore_state(&mut self, _data: &[u8]) -> bool {
        false
    }

    fn on_state_changed_effects(&mut self, _state: &AppView<'_>, _dirty: DirtyFlags) -> Effects {
        Effects::default()
    }
}

/// Blanket impl: every `PluginBackend` automatically satisfies
/// `Lifecycle` by delegating to the same-named methods on the existing
/// god trait. Call sites that only need lifecycle hooks can take
/// `&mut dyn Lifecycle` and benefit from a narrower interface without
/// churn at any of the 24+ `impl PluginBackend for X` blocks.
///
/// R1.3 will start migrating the *bodies* over: handlers will register
/// `Lifecycle` impls directly through `HandlerRegistry`, the
/// `PluginBackend::*_effects` methods will delegate the other way
/// (Lifecycle → PluginBackend), and eventually the methods drop off
/// `PluginBackend` entirely once R1.9 strips it.
impl<T: super::PluginBackend + ?Sized> Lifecycle for T {
    fn on_init_effects(&mut self, state: &AppView<'_>) -> Effects {
        super::PluginBackend::on_init_effects(self, state)
    }
    fn on_active_session_ready_effects(&mut self, state: &AppView<'_>) -> Effects {
        super::PluginBackend::on_active_session_ready_effects(self, state)
    }
    fn on_shutdown(&mut self) {
        super::PluginBackend::on_shutdown(self)
    }
    fn persist_state(&self) -> Option<Vec<u8>> {
        super::PluginBackend::persist_state(self)
    }
    fn restore_state(&mut self, data: &[u8]) -> bool {
        super::PluginBackend::restore_state(self, data)
    }
    fn on_state_changed_effects(&mut self, state: &AppView<'_>, dirty: DirtyFlags) -> Effects {
        super::PluginBackend::on_state_changed_effects(self, state, dirty)
    }
}

// =============================================================================
// 2. InputHandler — observation, pre-dispatch, middleware, fallback.
// =============================================================================

/// Key, text, mouse, and drop event handling across all dispatch
/// stages (observe → pre-dispatch → middleware → fallback).
///
/// Today `PluginBackend` carries all four stages as separate methods
/// per event type (12+ entry points). R2 (dispatch priority chain)
/// will collapse the stages into a single `handle()` returning a
/// `Verdict`; this trait lays the conceptual ground for that work.
pub trait InputHandler: Any {
    // --- Observation (notification-only) ---
    fn observe_key(&mut self, _key: &KeyEvent, _state: &AppView<'_>) {}
    fn observe_text_input(&mut self, _text: &str, _state: &AppView<'_>) {}
    fn observe_mouse(&mut self, _event: &MouseEvent, _state: &AppView<'_>) {}
    fn observe_drop(&mut self, _event: &DropEvent, _state: &AppView<'_>) {}

    // --- Pre-dispatch (before middleware) ---
    fn handle_key_pre_dispatch(
        &mut self,
        _key: &KeyEvent,
        _state: &AppView<'_>,
    ) -> KeyPreDispatchResult {
        KeyPreDispatchResult::Pass {
            commands: vec![],
            state_updates: super::StateUpdates::default(),
        }
    }
    fn handle_mouse_pre_dispatch(
        &mut self,
        _event: &MouseEvent,
        _state: &AppView<'_>,
    ) -> MousePreDispatchResult {
        MousePreDispatchResult::Pass {
            commands: vec![],
            state_updates: super::StateUpdates::default(),
        }
    }
    fn handle_text_input_pre_dispatch(
        &mut self,
        _text: &str,
        _state: &AppView<'_>,
    ) -> TextInputPreDispatchResult {
        TextInputPreDispatchResult::Pass
    }

    // --- Update / dispatch (consuming) ---
    fn update(&mut self, _msg: &mut dyn Any, _state: &AppView<'_>) -> Effects {
        Effects::default()
    }
    fn handle_key(&mut self, _key: &KeyEvent, _state: &AppView<'_>) -> Option<Vec<Command>> {
        None
    }
    fn handle_text_input(&mut self, _text: &str, _state: &AppView<'_>) -> Option<Vec<Command>> {
        None
    }
    fn handle_key_middleware(&mut self, key: &KeyEvent, state: &AppView<'_>) -> KeyHandleResult {
        match self.handle_key(key, state) {
            Some(commands) => KeyHandleResult::Consumed(commands),
            None => KeyHandleResult::Passthrough,
        }
    }
    fn handle_mouse(
        &mut self,
        _event: &MouseEvent,
        _id: InteractiveId,
        _state: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        None
    }
    fn handle_drop(
        &mut self,
        _event: &DropEvent,
        _id: InteractiveId,
        _state: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        None
    }
    fn handle_mouse_fallback(
        &mut self,
        _event: &MouseEvent,
        _scroll_amount: i32,
        _state: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        None
    }
    fn handle_default_scroll(
        &mut self,
        _candidate: DefaultScrollCandidate,
        _state: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        None
    }
}

// =============================================================================
// 3. Contributor — slot-based element contribution.
// =============================================================================

/// Contribute elements into well-known or plugin-defined slots.
pub trait Contributor: Any {
    fn contribute_to(
        &self,
        _region: &SlotId,
        _state: &AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        None
    }

    /// Overlay contribution with collision-avoidance context.
    fn contribute_overlay_with_ctx(
        &self,
        _state: &AppView<'_>,
        _ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        None
    }
}

// =============================================================================
// 4. Transformer — element / overlay transform with priority + descriptor.
// =============================================================================

/// Transform Element / Overlay subjects in priority order.
pub trait Transformer: Any {
    fn transform(
        &self,
        _target: &TransformTarget,
        subject: TransformSubject,
        _state: &AppView<'_>,
        _ctx: &TransformContext,
    ) -> TransformSubject {
        subject
    }

    fn transform_priority(&self) -> i16 {
        0
    }

    fn transform_descriptor(&self) -> Option<TransformDescriptor> {
        None
    }

    /// Algebraic patch flavour for plugins backed by `HandlerRegistry`.
    fn transform_patch(
        &self,
        _target: &TransformTarget,
        _state: &AppView<'_>,
        _ctx: &TransformContext,
    ) -> Option<ElementPatch> {
        None
    }

    fn transform_menu_item(
        &self,
        _item: &[crate::protocol::Atom],
        _index: usize,
        _selected: bool,
        _state: &AppView<'_>,
    ) -> Option<Vec<crate::protocol::Atom>> {
        None
    }
}

// =============================================================================
// 5. Annotator — gutter / background / inline / virtual-text decoration.
// =============================================================================

/// Per-line decoration: gutters, backgrounds, inline marks, virtual text.
///
/// Three generations coexist for now (legacy `annotate_line_with_ctx`,
/// decomposed `decorate_*`, unified `unified_display`); R5 unifies them.
pub trait Annotator: Any {
    /// **Legacy.** Returns a bundled `LineAnnotation`. Used by WASM
    /// plugins until R5 / R6 rewires WIT.
    fn annotate_line_with_ctx(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        None
    }

    fn has_decomposed_annotations(&self) -> bool {
        false
    }

    fn decorate_gutter(
        &self,
        _side: GutterSide,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<(i16, Element)> {
        None
    }

    fn decorate_background(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<BackgroundLayer> {
        None
    }

    fn decorate_inline(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<crate::render::InlineDecoration> {
        None
    }

    fn annotate_virtual_text(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Vec<VirtualTextItem> {
        vec![]
    }

    fn content_annotations(
        &self,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Vec<ContentAnnotation> {
        vec![]
    }
}

// =============================================================================
// 6. DisplayTransform — display directives + projections + navigation + scroll.
// =============================================================================

/// Spatial display transforms (fold, hide, virtual lines, projections).
pub trait DisplayTransform: Any {
    fn display_directive_priority(&self) -> i16 {
        0
    }
    fn display_directives(&self, _state: &AppView<'_>) -> Vec<DisplayDirective> {
        vec![]
    }

    fn has_unified_display(&self) -> bool {
        false
    }
    fn unified_display(&self, _state: &AppView<'_>) -> Vec<DisplayDirective> {
        vec![]
    }

    fn projection_descriptors(&self) -> &[ProjectionDescriptor] {
        &[]
    }
    fn projection_directives(
        &self,
        _id: &ProjectionId,
        _state: &AppView<'_>,
    ) -> Vec<DisplayDirective> {
        vec![]
    }

    fn navigation_policy(&self, _unit: &DisplayUnit) -> Option<NavigationPolicy> {
        None
    }
    fn navigation_action(
        &mut self,
        _unit: &DisplayUnit,
        _action: NavigationAction,
    ) -> Option<ActionResult> {
        None
    }

    fn compute_display_scroll_offset(
        &self,
        _cursor_display_y: usize,
        _viewport_height: usize,
        _default_offset: usize,
        _state: &AppView<'_>,
    ) -> Option<usize> {
        None
    }
}

// =============================================================================
// 7. Renderer — overlays, ornaments, inline-box paint.
// =============================================================================

/// Backend-independent rendering hooks (menu, info, ornaments,
/// inline-box content).
pub trait Renderer: Any {
    fn render_ornaments(
        &self,
        _state: &AppView<'_>,
        _ctx: &RenderOrnamentContext,
    ) -> OrnamentBatch {
        OrnamentBatch::default()
    }

    fn paint_inline_box(&self, _box_id: u64, _state: &AppView<'_>) -> Option<Element> {
        None
    }

    fn render_menu_overlay(&self, _state: &AppView<'_>, _view: &PluginView<'_>) -> Option<Overlay> {
        None
    }

    fn render_info_overlays(
        &self,
        _state: &AppView<'_>,
        _avoid: &[Rect],
        _view: &PluginView<'_>,
    ) -> Option<Vec<Overlay>> {
        None
    }
}

// =============================================================================
// 8. Io — process / IO event handling and process-task spawning.
// =============================================================================

/// I/O event handling and declarative process tasks.
pub trait Io: Any {
    fn on_io_event(&mut self, _event: &IoEvent, _state: &AppView<'_>) -> Effects {
        Effects::default()
    }

    fn start_process_task(&mut self, _name: &str) -> Vec<Command> {
        vec![]
    }

    /// Whether this plugin is allowed to spawn external processes.
    /// Native plugins default to `true`. WASM plugins consult resolved
    /// capability grants.
    fn allows_process_spawn(&self) -> bool {
        true
    }
}

// =============================================================================
// 9. PubSubMember — topic-based inter-plugin pub/sub.
// =============================================================================

/// Publish to and consume from the topic bus.
pub trait PubSubMember: Any {
    fn collect_publications(&self, _bus: &mut TopicBus, _state: &AppView<'_>) {}
    fn deliver_subscriptions(&mut self, _bus: &TopicBus) -> bool {
        false
    }
}

// =============================================================================
// 10. ExtensionParticipant — plugin-defined extension points.
// =============================================================================

/// Define and / or consume plugin-level extension points.
pub trait ExtensionParticipant: Any {
    fn extension_definitions(&self) -> &[ExtensionDefinition] {
        &[]
    }

    fn evaluate_extension(
        &self,
        _id: &ExtensionPointId,
        _input: &ChannelValue,
        _state: &AppView<'_>,
    ) -> Vec<ExtensionOutput> {
        vec![]
    }
}

// =============================================================================
// 11. WorkspaceMember — surfaces, placement, save/restore.
// =============================================================================

/// Workspace participation: own surfaces, request placement, persist
/// per-plugin layout data.
pub trait WorkspaceMember: Any {
    fn surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
        vec![]
    }

    fn workspace_request(&self) -> Option<crate::workspace::Placement> {
        None
    }

    fn on_workspace_changed(&mut self, _query: &WorkspaceQuery<'_>) {}

    fn workspace_save(&self) -> Option<serde_json::Value> {
        None
    }
    fn workspace_restore(&mut self, _data: &serde_json::Value) {}
}

// =============================================================================
// Cross-cutting introspection — id, capability bits, host authorities,
// suppressed builtins, diagnostics. These remain on `PluginBackend` for
// now since they describe the plugin as a whole rather than a single
// capability.
// =============================================================================

/// Metadata that every plugin exposes regardless of the capability mix.
///
/// Once R7 derives `PluginCapabilities` from registered handlers, this
/// trait shrinks to just `id` + `set_plugin_tag` + diagnostics.
pub trait PluginMeta: Any {
    fn id(&self) -> PluginId;

    fn set_plugin_tag(&mut self, _tag: crate::element::PluginTag) {}

    /// Hash of plugin-internal state for view caching.
    fn state_hash(&self) -> u64 {
        0
    }

    fn view_deps(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }

    fn capabilities(&self) -> super::PluginCapabilities {
        super::PluginCapabilities::all()
    }

    fn authorities(&self) -> PluginAuthorities {
        PluginAuthorities::empty()
    }

    fn suppressed_builtins(&self) -> &HashSet<BuiltinTarget> {
        static EMPTY: std::sync::LazyLock<HashSet<BuiltinTarget>> =
            std::sync::LazyLock::new(HashSet::new);
        &EMPTY
    }

    fn capability_descriptor(&self) -> Option<super::CapabilityDescriptor> {
        None
    }

    fn drain_diagnostics(&mut self) -> Vec<PluginDiagnostic> {
        Vec::new()
    }
}
