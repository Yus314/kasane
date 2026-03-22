use std::any::Any;

use crate::element::InteractiveId;
use crate::input::{KeyEvent, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;

use super::{
    AnnotateContext, AppView, BootstrapEffects, Command, ContributeContext, Contribution,
    DisplayDirective, IoEvent, LineAnnotation, OverlayContext, OverlayContribution, PaintHook,
    PluginAuthorities, PluginCapabilities, PluginId, RuntimeEffects, SessionReadyEffects, SlotId,
    TransformContext, TransformDescriptor, TransformSubject, TransformTarget,
};

/// Result of key middleware dispatch.
#[derive(Default)]
pub enum KeyHandleResult {
    Consumed(Vec<Command>),
    Transformed(KeyEvent),
    #[default]
    Passthrough,
}

/// Internal framework trait. Plugin authors should use [`Plugin`] instead.
#[doc(hidden)]
pub trait PluginBackend: Any {
    fn id(&self) -> PluginId;

    // --- Lifecycle hooks ---

    fn on_init_effects(&mut self, _state: &AppView<'_>) -> BootstrapEffects {
        BootstrapEffects::default()
    }
    fn on_active_session_ready_effects(&mut self, _state: &AppView<'_>) -> SessionReadyEffects {
        SessionReadyEffects::default()
    }
    fn on_shutdown(&mut self) {}
    fn on_state_changed_effects(
        &mut self,
        _state: &AppView<'_>,
        _dirty: DirtyFlags,
    ) -> RuntimeEffects {
        RuntimeEffects::default()
    }
    /// Handle an I/O event (process output, etc.).
    fn on_io_event_effects(&mut self, _event: &IoEvent, _state: &AppView<'_>) -> RuntimeEffects {
        RuntimeEffects::default()
    }

    // --- Input hooks ---

    /// Observe a key event (notification only, cannot consume).
    fn observe_key(&mut self, _key: &KeyEvent, _state: &AppView<'_>) {}
    /// Observe a mouse event (notification only, cannot consume).
    fn observe_mouse(&mut self, _event: &MouseEvent, _state: &AppView<'_>) {}

    // --- Update / Input handling ---

    fn update_effects(&mut self, _msg: &mut dyn Any, _state: &AppView<'_>) -> RuntimeEffects {
        RuntimeEffects::default()
    }
    fn handle_key(&mut self, _key: &KeyEvent, _state: &AppView<'_>) -> Option<Vec<Command>> {
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
    fn handle_default_scroll(
        &mut self,
        _candidate: DefaultScrollCandidate,
        _state: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        None
    }

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

    // --- Cursor style ---

    /// Override the cursor style. Return None to defer to the default logic.
    /// First non-None result from any plugin is used.
    fn cursor_style_override(&self, _state: &AppView<'_>) -> Option<crate::render::CursorStyle> {
        None
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

    // --- Paint hooks (Phase 5) ---

    /// Return paint hooks owned by this plugin.
    /// Called once during initialization; returned hooks are registered for use
    /// in the rendering pipeline (applied after the standard paint pass).
    fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        vec![]
    }

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

    // === Annotate ===

    /// Annotate a buffer line with gutter elements and/or background layer.
    fn annotate_line_with_ctx(
        &self,
        _line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        None
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

    // === Overlay ===

    /// Contribute an overlay with collision-avoidance context.
    fn contribute_overlay_with_ctx(
        &self,
        _state: &AppView<'_>,
        _ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        None
    }
}
