use std::any::Any;

use crate::element::{Element, InteractiveId};
use crate::input::{KeyEvent, MouseEvent};
use crate::pane::{PaneId, PanePermissions};
use crate::state::{AppState, DirtyFlags};

use super::{
    AnnotateContext, Command, ContributeContext, Contribution, IoEvent, LineAnnotation,
    OverlayContext, OverlayContribution, PaintHook, PluginCapabilities, PluginId, SlotId,
    TransformContext, TransformTarget,
};

pub trait Plugin: Any {
    fn id(&self) -> PluginId;

    // --- Lifecycle hooks ---

    fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
        vec![]
    }
    fn on_shutdown(&mut self) {}
    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        vec![]
    }
    /// Handle an I/O event (process output, etc.).
    fn on_io_event(&mut self, _event: &IoEvent, _state: &AppState) -> Vec<Command> {
        vec![]
    }

    // --- Input hooks ---

    /// Observe a key event (notification only, cannot consume).
    fn observe_key(&mut self, _key: &KeyEvent, _state: &AppState) {}
    /// Observe a mouse event (notification only, cannot consume).
    fn observe_mouse(&mut self, _event: &MouseEvent, _state: &AppState) {}

    // --- Update / Input handling ---

    fn update(&mut self, _msg: Box<dyn Any>, _state: &AppState) -> Vec<Command> {
        vec![]
    }
    fn handle_key(&mut self, _key: &KeyEvent, _state: &AppState) -> Option<Vec<Command>> {
        None
    }
    fn handle_mouse(
        &mut self,
        _event: &MouseEvent,
        _id: InteractiveId,
        _state: &AppState,
    ) -> Option<Vec<Command>> {
        None
    }

    // --- View contributions ---

    /// Hash of plugin-internal state for view caching (L1).
    /// Default: 0 (no state-based caching).
    fn state_hash(&self) -> u64 {
        0
    }

    // --- Cursor style ---

    /// Override the cursor style. Return None to defer to the default logic.
    /// First non-None result from any plugin is used.
    fn cursor_style_override(&self, _state: &AppState) -> Option<crate::render::CursorStyle> {
        None
    }

    // --- Menu item transformation ---

    /// Transform a menu item before rendering. Return None for no change.
    fn transform_menu_item(
        &self,
        _item: &[crate::protocol::Atom],
        _index: usize,
        _selected: bool,
        _state: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        None
    }

    /// Declare which capabilities this plugin supports.
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::all()
    }

    /// Whether this plugin is allowed to spawn external processes.
    ///
    /// Native plugins default to `true`. WASM plugins check their resolved
    /// capability grants (the `process` capability must be requested and not
    /// denied by user configuration).
    fn allows_process_spawn(&self) -> bool {
        true
    }

    /// DirtyFlags dependencies for overlay contributions.
    /// Default: ALL (always recompute).
    fn overlay_deps(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }

    // --- Pane lifecycle hooks (Phase 5) ---

    /// Called when a new pane is created.
    fn on_pane_created(&mut self, _pane_id: PaneId, _state: &AppState) {}

    /// Called when a pane is closed.
    fn on_pane_closed(&mut self, _pane_id: PaneId) {}

    /// Called when focus changes between panes.
    fn on_focus_changed(&mut self, _from: Option<PaneId>, _to: PaneId, _state: &AppState) {}

    /// Render plugin-owned pane content. Return `None` if this plugin does not
    /// own the given pane.
    fn render_pane(&self, _pane_id: PaneId, _cols: u16, _rows: u16) -> Option<Element> {
        None
    }

    /// Handle a key event for a plugin-owned pane.
    fn handle_pane_key(&mut self, _pane_id: PaneId, _key: &KeyEvent) -> Option<Vec<Command>> {
        None
    }

    /// Pane permission flags for this plugin. Default: none.
    fn pane_permissions(&self) -> PanePermissions {
        PanePermissions::empty()
    }

    // --- Surface system hooks (Phase S) ---

    /// Return surfaces owned by this plugin.
    /// Called once during initialization; returned surfaces are registered in the SurfaceRegistry.
    fn surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
        vec![]
    }

    /// Request where plugin-owned surfaces should be placed in the workspace.
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
        _state: &AppState,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        None
    }

    /// DirtyFlags dependencies for `contribute_to()`.
    fn contribute_deps(&self, _region: &SlotId) -> DirtyFlags {
        DirtyFlags::ALL
    }

    // === Transform ===

    /// Transform an element for the given target. The element may be the default
    /// or a result from a previous plugin in the chain.
    ///
    /// Default: pass through unchanged.
    fn transform(
        &self,
        _target: &TransformTarget,
        element: Element,
        _state: &AppState,
        _ctx: &TransformContext,
    ) -> Element {
        element
    }

    /// Priority for transform chain ordering (higher = applied earlier / inner).
    fn transform_priority(&self) -> i16 {
        0
    }

    /// DirtyFlags dependencies for `transform()` on a given target.
    fn transform_deps(&self, _target: &TransformTarget) -> DirtyFlags {
        DirtyFlags::ALL
    }

    // === Annotate ===

    /// Annotate a buffer line with gutter elements and/or background layer.
    fn annotate_line_with_ctx(
        &self,
        _line: usize,
        _state: &AppState,
        _ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        None
    }

    /// DirtyFlags dependencies for `annotate_line_with_ctx()`.
    fn annotate_deps(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }

    // === Overlay ===

    /// Contribute an overlay with collision-avoidance context.
    fn contribute_overlay_with_ctx(
        &self,
        _state: &AppState,
        _ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        None
    }
}
