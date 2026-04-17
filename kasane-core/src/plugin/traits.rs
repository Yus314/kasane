use std::any::Any;

use crate::element::{InteractiveId, PluginTag};
use crate::input::{CompiledKeyMap, DropEvent, KeyEvent, KeyResponse, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::{self, DirtyFlags};

use super::extension_point::{ExtensionOutput, ExtensionPointId};
use super::pubsub::TopicBus;
use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use crate::display::unit::DisplayUnit;

use super::{
    AnnotateContext, AppView, BackgroundLayer, Command, ContributeContext, Contribution,
    DisplayDirective, Effects, ElementPatch, GutterSide, IoEvent, LineAnnotation, OrnamentBatch,
    OverlayContext, OverlayContribution, PluginAuthorities, PluginCapabilities, PluginDiagnostic,
    PluginId, RenderOrnamentContext, SlotId, TransformContext, TransformDescriptor,
    TransformSubject, TransformTarget, VirtualTextItem,
};

/// Result of key middleware dispatch.
#[derive(Default)]
pub enum KeyHandleResult {
    Consumed(Vec<Command>),
    Transformed(KeyEvent),
    #[default]
    Passthrough,
}

impl From<KeyResponse> for KeyHandleResult {
    fn from(response: KeyResponse) -> Self {
        match response {
            KeyResponse::Pass => KeyHandleResult::Passthrough,
            KeyResponse::Consume => KeyHandleResult::Consumed(vec![]),
            KeyResponse::ConsumeRedraw => {
                KeyHandleResult::Consumed(vec![Command::RequestRedraw(state::DirtyFlags::ALL)])
            }
            KeyResponse::ConsumeWith(commands) => KeyHandleResult::Consumed(commands),
        }
    }
}

/// Internal framework trait. Plugin authors should use [`Plugin`] instead.
#[doc(hidden)]
pub trait PluginBackend: Any {
    fn id(&self) -> PluginId;

    /// Inject the framework-assigned plugin tag for interactive ID ownership.
    fn set_plugin_tag(&mut self, _tag: PluginTag) {}

    // --- Lifecycle hooks ---

    fn on_init_effects(&mut self, _state: &AppView<'_>) -> Effects {
        Effects::default()
    }
    fn on_active_session_ready_effects(&mut self, _state: &AppView<'_>) -> Effects {
        Effects::default()
    }
    fn on_shutdown(&mut self) {}
    fn on_state_changed_effects(&mut self, _state: &AppView<'_>, _dirty: DirtyFlags) -> Effects {
        Effects::default()
    }
    /// Handle an I/O event (process output, etc.).
    fn on_io_event_effects(&mut self, _event: &IoEvent, _state: &AppView<'_>) -> Effects {
        Effects::default()
    }

    // --- Input hooks ---

    /// Observe a key event (notification only, cannot consume).
    fn observe_key(&mut self, _key: &KeyEvent, _state: &AppView<'_>) {}
    /// Observe committed text input (notification only, cannot consume).
    fn observe_text_input(&mut self, _text: &str, _state: &AppView<'_>) {}
    /// Observe a mouse event (notification only, cannot consume).
    fn observe_mouse(&mut self, _event: &MouseEvent, _state: &AppView<'_>) {}
    /// Observe a drop event (notification only, cannot consume).
    fn observe_drop(&mut self, _event: &DropEvent, _state: &AppView<'_>) {}

    // --- Update / Input handling ---

    fn update_effects(&mut self, _msg: &mut dyn Any, _state: &AppView<'_>) -> Effects {
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
    fn handle_default_scroll(
        &mut self,
        _candidate: DefaultScrollCandidate,
        _state: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        None
    }

    // --- Key map dispatch (Phase 2+) ---

    /// Return the compiled key map for framework-side binding resolution.
    ///
    /// Plugins that use the `key_map {}` declarative system return `Some` here.
    /// The framework uses this to skip WASM calls for unmatched keys and to
    /// manage chord state centrally.
    fn compiled_key_map(&self) -> Option<&CompiledKeyMap> {
        None
    }

    /// Invoke a named action from the key map.
    ///
    /// Called by the framework when `compiled_key_map().match_key()` or chord
    /// resolution identifies an action. The plugin executes the action body
    /// and returns a [`KeyResponse`].
    fn invoke_action(
        &mut self,
        _action_id: &str,
        _key: &KeyEvent,
        _state: &AppView<'_>,
    ) -> KeyResponse {
        KeyResponse::Pass
    }

    /// Refresh the `active` flags on key groups.
    ///
    /// Called by the framework when the plugin's state hash changes, before
    /// key matching. The plugin evaluates `when()` predicates and updates
    /// each group's `active` field in the compiled key map.
    fn refresh_key_groups(&mut self, _state: &AppView<'_>) {}

    // --- View contributions ---

    /// Hash of plugin-internal state for view caching (L1).
    /// Default: 0 (no state-based caching).
    fn state_hash(&self) -> u64 {
        0
    }

    /// Declare which `DirtyFlags` this plugin's view methods depend on.
    ///
    /// When the framework detects that neither the plugin's state hash nor any
    /// of the declared flags have changed, it can skip re-collecting this
    /// plugin's view contributions. Default: `DirtyFlags::ALL` (always re-collect).
    fn view_deps(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }

    // --- Render ornaments ---

    /// Return backend-independent physical ornament proposals for the current frame.
    fn render_ornaments(
        &self,
        _state: &AppView<'_>,
        _ctx: &RenderOrnamentContext,
    ) -> OrnamentBatch {
        OrnamentBatch::default()
    }

    // --- Menu item transformation ---

    /// Transform a menu item before rendering. Return None for no change.
    fn transform_menu_item(
        &self,
        _item: &[crate::protocol::Atom],
        _index: usize,
        _selected: bool,
        _state: &AppView<'_>,
    ) -> Option<Vec<crate::protocol::Atom>> {
        None
    }

    /// Declare which capabilities this plugin supports.
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::all()
    }

    /// Start a named process task, returning spawn commands.
    ///
    /// Framework-managed tasks registered via [`HandlerRegistry::on_process_task`]
    /// are looked up by name. Returns the initial `SpawnProcess` command(s).
    /// Default: returns empty (no tasks registered).
    fn start_process_task(&mut self, _name: &str) -> Vec<Command> {
        vec![]
    }

    /// Host-level authorities required for privileged deferred effects.
    fn authorities(&self) -> PluginAuthorities {
        PluginAuthorities::empty()
    }

    /// Whether this plugin is allowed to spawn external processes.
    ///
    /// Native plugins default to `true`. WASM plugins check their resolved
    /// capability grants (the `process` capability must be requested and not
    /// denied by user configuration).
    fn allows_process_spawn(&self) -> bool {
        true
    }

    // --- Surface system hooks (Phase S) ---

    /// Return surfaces owned by this plugin.
    /// Called during bootstrap preflight before `on_init()`.
    fn surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
        vec![]
    }

    /// Legacy plugin-wide placement request for plugin-owned surfaces.
    /// Evaluated during bootstrap preflight before `on_init()`.
    fn workspace_request(&self) -> Option<crate::workspace::Placement> {
        None
    }

    /// Notification that the workspace layout has changed.
    fn on_workspace_changed(&mut self, _query: &crate::workspace::WorkspaceQuery<'_>) {}

    // === Contribute ===

    /// Contribute an element to a region with layout context and priority.
    fn contribute_to(
        &self,
        _region: &SlotId,
        _state: &AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        None
    }

    // === Transform ===

    /// Transform a subject for the given target. The subject may be an Element
    /// or an Overlay (for menu/info targets), and may be the default or a result
    /// from a previous plugin in the chain.
    ///
    /// Default: pass through unchanged.
    fn transform(
        &self,
        _target: &TransformTarget,
        subject: TransformSubject,
        _state: &AppView<'_>,
        _ctx: &TransformContext,
    ) -> TransformSubject {
        subject
    }

    /// Priority for transform chain ordering (higher = applied earlier / inner).
    fn transform_priority(&self) -> i16 {
        0
    }

    /// Declare the transform scope and targets for debug-time conflict detection.
    fn transform_descriptor(&self) -> Option<TransformDescriptor> {
        None
    }

    /// Return a declarative transform patch for the given target.
    ///
    /// Plugins backed by [`HandlerRegistry`] return the raw [`ElementPatch`] for
    /// algebraic composition. Legacy plugins return `None` and are applied
    /// imperatively via [`transform()`](Self::transform).
    fn transform_patch(
        &self,
        _target: &TransformTarget,
        _state: &AppView<'_>,
        _ctx: &TransformContext,
    ) -> Option<ElementPatch> {
        None
    }

    // === Annotate ===

    /// Annotate a buffer line with gutter elements and/or background layer.
    ///
    /// **Deprecated** — prefer the decomposed annotation methods
    /// ([`annotate_gutter`], [`annotate_background`], [`annotate_inline`],
    /// [`annotate_virtual_text`]) which allow per-concern caching and avoid
    /// bundling unrelated annotation types. Legacy (WASM) plugins still use
    /// this method; native plugins should register individual handlers via
    /// [`HandlerRegistry`].
    fn annotate_line_with_ctx(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        None
    }

    /// Whether this plugin supports decomposed annotation methods.
    ///
    /// When true, `collect_annotations()` calls the per-concern methods
    /// directly instead of `annotate_line_with_ctx()`.
    /// `PluginBridge` returns true; legacy plugins return false.
    fn has_decomposed_annotations(&self) -> bool {
        false
    }

    /// Return a gutter element for the given line and side.
    fn annotate_gutter(
        &self,
        _side: GutterSide,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<(i16, crate::element::Element)> {
        None
    }

    /// Return a background layer for the given line.
    fn annotate_background(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<BackgroundLayer> {
        None
    }

    /// Return an inline decoration for the given line.
    fn annotate_inline(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<crate::render::InlineDecoration> {
        None
    }

    /// Return virtual text items for the given line.
    fn annotate_virtual_text(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Vec<VirtualTextItem> {
        vec![]
    }

    // === Display Transform ===

    /// Priority for display directive composition (higher = wins overlap conflicts).
    fn display_directive_priority(&self) -> i16 {
        0
    }

    /// Return display transformation directives (fold, hide, insert virtual text).
    fn display_directives(&self, _state: &AppView<'_>) -> Vec<DisplayDirective> {
        vec![]
    }

    // === Projection Mode ===

    /// Return projection descriptors defined by this plugin.
    fn projection_descriptors(&self) -> &[crate::display::ProjectionDescriptor] {
        &[]
    }

    /// Return display directives for a specific active projection.
    fn projection_directives(
        &self,
        _id: &crate::display::ProjectionId,
        _state: &AppView<'_>,
    ) -> Vec<DisplayDirective> {
        vec![]
    }

    // === Navigation (DU-4) ===

    /// Override navigation policy for a display unit.
    /// Return `None` to defer to the next plugin or built-in default.
    fn navigation_policy(&self, _unit: &DisplayUnit) -> Option<NavigationPolicy> {
        None
    }

    /// Handle a navigation action on a display unit.
    /// Return `None` to defer to the next plugin or built-in fallback.
    fn navigation_action(
        &mut self,
        _unit: &DisplayUnit,
        _action: NavigationAction,
    ) -> Option<ActionResult> {
        None
    }

    // === Overlay ===

    /// Contribute an overlay with collision-avoidance context.
    fn contribute_overlay_with_ctx(
        &self,
        _state: &AppView<'_>,
        _ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        None
    }

    // --- Pub/Sub ---

    /// Collect publications from this plugin onto the topic bus.
    /// Only `PluginBridge` (native plugins with HandlerTable) overrides this.
    fn collect_publications(&self, _bus: &mut TopicBus, _state: &AppView<'_>) {}

    /// Deliver subscribed topic values to this plugin, returning true if state changed.
    /// Only `PluginBridge` (native plugins with HandlerTable) overrides this.
    fn deliver_subscriptions(&mut self, _bus: &TopicBus) -> bool {
        false
    }

    // --- Extension Points ---

    /// Return extension point definitions from this plugin.
    fn extension_definitions(&self) -> &[super::extension_point::ExtensionDefinition] {
        &[]
    }

    /// Evaluate extension contributions from this plugin for a given extension point.
    fn evaluate_extension(
        &self,
        _id: &ExtensionPointId,
        _input: &super::channel::ChannelValue,
        _state: &AppView<'_>,
    ) -> Vec<ExtensionOutput> {
        vec![]
    }

    // --- Capability Descriptor ---

    /// Return a structured capability descriptor for interference detection.
    fn capability_descriptor(&self) -> Option<super::CapabilityDescriptor> {
        None
    }

    // --- Diagnostics ---

    /// Drain any pending runtime diagnostics accumulated since the last call.
    fn drain_diagnostics(&mut self) -> Vec<PluginDiagnostic> {
        Vec::new()
    }
}
