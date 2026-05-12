//! Type-erased handler dispatch table.
//!
//! `HandlerTable` stores handler closures produced by [`HandlerRegistry::into_table()`]
//! after type erasure. Each field holds an optional handler (or vec of handlers) for
//! a specific extension point category.
//!
//! This module is framework-internal. Plugin authors interact with
//! [`HandlerRegistry`](super::handler_registry::HandlerRegistry) instead.

use std::any::Any;

use crate::element::{Element, InteractiveId, Overlay};
use crate::input::{CompiledKeyMap, DropEvent, KeyEvent, KeyResponse, MouseEvent};
use crate::protocol::Atom;
use crate::render::InlineDecoration;
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use crate::display::content_annotation::ContentAnnotation;
use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use crate::display::unit::DisplayUnit;

use crate::display::projection::ProjectionDescriptor;

use super::element_patch::ElementPatch;
use super::process_task::ProcessTaskEntry;
use super::pubsub::{PublishEntry, SubscribeEntry};
use super::traits::{
    KeyHandleResult, KeyPreDispatchResult, MousePreDispatchResult, TextInputPreDispatchResult,
};
use super::{
    AnnotateContext, AppView, BackgroundLayer, Command, ContributeContext, Contribution,
    DisplayDirective, Effects, IoEvent, OrnamentBatch, OverlayContext, OverlayContribution,
    PluginCapabilities, PluginState, RenderOrnamentContext, SlotId, TransformContext,
    TransformTarget, VirtualTextItem,
};

// =============================================================================
// Gutter side enum
// =============================================================================

/// Which side of the buffer gutter an annotation handler targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GutterSide {
    Left,
    Right,
}

// =============================================================================
// Erased handler type aliases
// =============================================================================

// Lifecycle handlers (produce new state + effects)
pub(crate) type ErasedInitHandler =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>) -> (Box<dyn PluginState>, Effects) + Send + Sync>;
pub(crate) type ErasedSessionReadyHandler =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>) -> (Box<dyn PluginState>, Effects) + Send + Sync>;
pub(crate) type ErasedStateChangedHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>, DirtyFlags) -> (Box<dyn PluginState>, Effects)
        + Send
        + Sync,
>;
pub(crate) type ErasedIoEventHandler = Box<
    dyn Fn(&dyn PluginState, &IoEvent, &AppView<'_>) -> (Box<dyn PluginState>, Effects)
        + Send
        + Sync,
>;
pub(crate) type ErasedWorkspaceChangedHandler =
    Box<dyn Fn(&dyn PluginState, &WorkspaceQuery<'_>) -> Box<dyn PluginState> + Send + Sync>;
pub(crate) type ErasedWorkspaceSaveHandler =
    Box<dyn Fn(&dyn PluginState) -> Option<serde_json::Value> + Send + Sync>;
pub(crate) type ErasedWorkspaceRestoreHandler =
    Box<dyn Fn(&dyn PluginState, &serde_json::Value) -> Box<dyn PluginState> + Send + Sync>;
/// Opaque-bytes counterpart to [`ErasedWorkspaceSaveHandler`]. Used by
/// adapters whose persistence contract is bytes (e.g. WASM plugins via
/// `persist-state` / `restore-state` WIT exports) rather than structured
/// JSON.
pub(crate) type ErasedPersistStateHandler =
    Box<dyn Fn(&dyn PluginState) -> Option<Vec<u8>> + Send + Sync>;
pub(crate) type ErasedRestoreStateHandler =
    Box<dyn Fn(&dyn PluginState, &[u8]) -> bool + Send + Sync>;
pub(crate) type ErasedShutdownHandler = Box<dyn Fn(&dyn PluginState) + Send + Sync>;
pub(crate) type ErasedSurfacesFactory =
    Box<dyn Fn(&dyn PluginState) -> Vec<Box<dyn crate::surface::Surface>> + Send + Sync>;
pub(crate) type ErasedLensFactory =
    Box<dyn Fn() -> Vec<std::sync::Arc<dyn crate::lens::Lens>> + Send + Sync>;
pub(crate) type ErasedUpdateHandler = Box<
    dyn Fn(&dyn PluginState, &mut dyn Any, &AppView<'_>) -> (Box<dyn PluginState>, Effects)
        + Send
        + Sync,
>;

/// HandlerRegistry-driven `on_command_error` (ADR-044). Fires when a
/// Kakoune command attributed to this plugin fails. The handler receives
/// the parsed [`super::error_attribution::PluginErrorEvent`] and returns
/// updated state + effects.
pub(crate) type ErasedCommandErrorHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &super::error_attribution::PluginErrorEvent,
            &AppView<'_>,
        ) -> (Box<dyn PluginState>, Effects)
        + Send
        + Sync,
>;

/// HandlerRegistry-driven `on_subscription` (ADR-044). Fires once per
/// subscribed topic during the pub/sub delivery phase with **all** values
/// published on that topic this tick. Mirrors the
/// WIT `on-subscription(topic, values) -> runtime-effects` export so
/// native and WASM plugins observe the same per-topic batch shape.
///
/// Independent of the per-value [`SubscribeEntry`] path: a plugin may
/// register typed [`super::handler_registry::HandlerRegistry::subscribe`]
/// handlers (per-value state mutation) and an `on_subscription` handler
/// (per-topic effects emission) at the same time.
pub(crate) type ErasedSubscriptionHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &str,
            &[super::ChannelValue],
            &AppView<'_>,
        ) -> (Box<dyn PluginState>, Effects)
        + Send
        + Sync,
>;

// Input handlers
pub(crate) type ErasedKeyHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &KeyEvent,
            &AppView<'_>,
        ) -> Option<(Box<dyn PluginState>, Vec<Command>)>
        + Send
        + Sync,
>;
pub(crate) type ErasedKeyMiddlewareHandler = Box<
    dyn Fn(&dyn PluginState, &KeyEvent, &AppView<'_>) -> (Box<dyn PluginState>, KeyHandleResult)
        + Send
        + Sync,
>;
pub(crate) type ErasedObserveKeyHandler =
    Box<dyn Fn(&dyn PluginState, &KeyEvent, &AppView<'_>) -> Box<dyn PluginState> + Send + Sync>;
pub(crate) type ErasedObserveTextInputHandler =
    Box<dyn Fn(&dyn PluginState, &str, &AppView<'_>) -> Box<dyn PluginState> + Send + Sync>;
pub(crate) type ErasedObserveMouseHandler =
    Box<dyn Fn(&dyn PluginState, &MouseEvent, &AppView<'_>) -> Box<dyn PluginState> + Send + Sync>;
pub(crate) type ErasedTextInputHandler = Box<
    dyn Fn(&dyn PluginState, &str, &AppView<'_>) -> Option<(Box<dyn PluginState>, Vec<Command>)>
        + Send
        + Sync,
>;
pub(crate) type ErasedHandleMouseHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &MouseEvent,
            InteractiveId,
            &AppView<'_>,
        ) -> Option<(Box<dyn PluginState>, Vec<Command>)>
        + Send
        + Sync,
>;
pub(crate) type ErasedObserveDropHandler =
    Box<dyn Fn(&dyn PluginState, &DropEvent, &AppView<'_>) -> Box<dyn PluginState> + Send + Sync>;
pub(crate) type ErasedHandleDropHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &DropEvent,
            InteractiveId,
            &AppView<'_>,
        ) -> Option<(Box<dyn PluginState>, Vec<Command>)>
        + Send
        + Sync,
>;
pub(crate) type ErasedDefaultScrollHandler = Box<
    dyn Fn(
            &dyn PluginState,
            DefaultScrollCandidate,
            &AppView<'_>,
        ) -> Option<(Box<dyn PluginState>, ScrollPolicyResult)>
        + Send
        + Sync,
>;
pub(crate) type ErasedKeyPreDispatchHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &KeyEvent,
            &AppView<'_>,
        ) -> (Box<dyn PluginState>, KeyPreDispatchResult)
        + Send
        + Sync,
>;
pub(crate) type ErasedMousePreDispatchHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &MouseEvent,
            &AppView<'_>,
        ) -> (Box<dyn PluginState>, MousePreDispatchResult)
        + Send
        + Sync,
>;
pub(crate) type ErasedTextInputPreDispatchHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &str,
            &AppView<'_>,
        ) -> (Box<dyn PluginState>, TextInputPreDispatchResult)
        + Send
        + Sync,
>;
pub(crate) type ErasedMouseFallbackHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &MouseEvent,
            i32,
            &AppView<'_>,
        ) -> (Box<dyn PluginState>, Option<Vec<Command>>)
        + Send
        + Sync,
>;

// Key map handlers (Phase 2 — declarative key bindings)
pub(crate) type ErasedKeyMapBuilder = Box<dyn Fn(&dyn PluginState) -> CompiledKeyMap + Send + Sync>;
pub(crate) type ErasedActionHandler = Box<
    dyn Fn(&dyn PluginState, &str, &KeyEvent, &AppView<'_>) -> (Box<dyn PluginState>, KeyResponse)
        + Send
        + Sync,
>;
pub(crate) type ErasedGroupRefreshHandler =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>, &mut CompiledKeyMap) + Send + Sync>;

// View handlers (immutable state)
pub(crate) type ErasedContributeAnyHandler = Box<
    dyn Fn(&dyn PluginState, &SlotId, &AppView<'_>, &ContributeContext) -> Option<Contribution>
        + Send
        + Sync,
>;
pub(crate) type ErasedContributeHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>, &ContributeContext) -> Option<Contribution>
        + Send
        + Sync,
>;
pub(crate) type ErasedTransformHandler = Box<
    dyn Fn(&dyn PluginState, &TransformTarget, &AppView<'_>, &TransformContext) -> ElementPatch
        + Send
        + Sync,
>;
pub(crate) type ErasedAnnotateGutterHandler = Box<
    dyn Fn(&dyn PluginState, usize, &AppView<'_>, &AnnotateContext) -> Option<Element>
        + Send
        + Sync,
>;
pub(crate) type ErasedAnnotateBackgroundHandler = Box<
    dyn Fn(&dyn PluginState, usize, &AppView<'_>, &AnnotateContext) -> Option<BackgroundLayer>
        + Send
        + Sync,
>;
pub(crate) type ErasedAnnotateLineHandler = Box<
    dyn Fn(&dyn PluginState, usize, &AppView<'_>, &AnnotateContext) -> Option<super::LineAnnotation>
        + Send
        + Sync,
>;
pub(crate) type ErasedAnnotateInlineHandler = Box<
    dyn Fn(&dyn PluginState, usize, &AppView<'_>, &AnnotateContext) -> Option<InlineDecoration>
        + Send
        + Sync,
>;
pub(crate) type ErasedVirtualTextHandler = Box<
    dyn Fn(&dyn PluginState, usize, &AppView<'_>, &AnnotateContext) -> Vec<VirtualTextItem>
        + Send
        + Sync,
>;
pub(crate) type ErasedOverlayHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>, &OverlayContext) -> Option<OverlayContribution>
        + Send
        + Sync,
>;
pub(crate) type ErasedDisplayHandler =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>) -> Vec<DisplayDirective> + Send + Sync>;
pub(crate) type ErasedContentAnnotationHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>, &AnnotateContext) -> Vec<ContentAnnotation>
        + Send
        + Sync,
>;
pub(crate) type ErasedRenderOrnamentHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>, &RenderOrnamentContext) -> OrnamentBatch + Send + Sync,
>;
pub(crate) type ErasedUnifiedDisplayHandler =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>) -> Vec<DisplayDirective> + Send + Sync>;
pub(crate) type ErasedMenuTransformHandler = Box<
    dyn Fn(&dyn PluginState, &[Atom], usize, bool, &AppView<'_>) -> Option<Vec<Atom>> + Send + Sync,
>;

// Scroll offset handler
pub(crate) type ErasedDisplayScrollOffsetHandler =
    Box<dyn Fn(&dyn PluginState, usize, usize, usize, &AppView<'_>) -> Option<usize> + Send + Sync>;

// Renderer extension point handlers
pub(crate) type ErasedMenuRendererHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>, &super::PluginView<'_>) -> Option<Overlay> + Send + Sync,
>;
pub(crate) type ErasedInfoRendererHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &AppView<'_>,
            &[crate::layout::Rect],
            &super::PluginView<'_>,
        ) -> Option<Vec<Overlay>>
        + Send
        + Sync,
>;

// Navigation handlers (DU-4)
pub(crate) type ErasedNavigationPolicyHandler =
    Box<dyn Fn(&dyn PluginState, &DisplayUnit) -> NavigationPolicy + Send + Sync>;
pub(crate) type ErasedNavigationActionHandler = Box<
    dyn Fn(&dyn PluginState, &DisplayUnit, NavigationAction) -> (Box<dyn PluginState>, ActionResult)
        + Send
        + Sync,
>;

// Virtual edit handler (BDT)
pub(crate) type ErasedVirtualEditHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &super::handler_registry::VirtualEditContext,
            &AppView<'_>,
        ) -> (Box<dyn PluginState>, Vec<Command>)
        + Send
        + Sync,
>;

/// Buffer-edit intercept handler (ADR-035 ShadowCursor follow-up).
///
/// Invoked by the dispatch loop when `BuiltinShadowCursorPlugin`
/// surfaces a `pending_buffer_edit` from a Mirror-projection commit.
/// Plugins return a `BufferEditVerdict` (PassThrough / Replace / Veto)
/// to observe, transform, or veto the commit before it's serialized
/// to Kakoune `exec -draft` commands.
pub(crate) type ErasedBufferEditInterceptHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &crate::state::shadow_cursor::BufferEdit,
            &AppView<'_>,
        ) -> (
            Box<dyn PluginState>,
            crate::state::shadow_cursor::BufferEditVerdict,
        ) + Send
        + Sync,
>;

/// Inline-box paint handler (ADR-031 Phase 10 Step 2-native).
///
/// Returns `Some(element)` to paint inside the inline-box slot at
/// `box_id`, or `None` to leave the slot empty (the renderer falls
/// back to the placeholder reservation behaviour). The element is
/// laid out at the slot's geometric position determined by Parley's
/// `push_inline_box`.
pub(crate) type ErasedInlineBoxPaintHandler =
    Box<dyn Fn(&dyn PluginState, u64, &AppView<'_>) -> Option<Element> + Send + Sync>;

// =============================================================================
// Handler entry types (handler + metadata)
// =============================================================================

/// A contribute handler bound to a specific slot.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct ContributeEntry {
    pub(crate) slot: SlotId,
    pub(crate) handler: ErasedContributeHandler,
}

/// A slot-agnostic contribute handler.
///
/// Used by adapters whose underlying contract dispatches contribution
/// requests for arbitrary slots (e.g. WASM plugins via the
/// `contribute-to(region, …)` WIT export). Looked up after slot-bound
/// handlers in the dispatch order, so an `on_contribute` registration
/// for a specific slot wins over the fallback.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct ContributeAnyEntry {
    pub(crate) handler: ErasedContributeAnyHandler,
}

/// A transform handler with priority metadata.
///
/// `handler` is the declarative patch handler (the modern path). The
/// optional `full_handler` is the legacy full-rewrite path used by
/// adapters whose underlying contract returns a transformed
/// [`TransformSubject`] directly (e.g. WASM plugins via the
/// `transform` WIT export when the plugin doesn't implement
/// `transform-patch`). The bridge consults the patch first; if it
/// resolves to [`ElementPatch::Identity`], the full handler runs as a
/// fallback.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct TransformEntry {
    pub(crate) priority: i16,
    pub(crate) targets: Vec<TransformTarget>,
    pub(crate) handler: ErasedTransformHandler,
    pub(crate) full_handler: Option<ErasedFullTransformHandler>,
}

/// Imperative full-rewrite transform handler. See
/// [`TransformEntry::full_handler`].
pub(crate) type ErasedFullTransformHandler = Box<
    dyn Fn(
            &dyn PluginState,
            &TransformTarget,
            super::TransformSubject,
            &AppView<'_>,
            &TransformContext,
        ) -> super::TransformSubject
        + Send
        + Sync,
>;

/// A gutter annotation handler with side and priority metadata.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct GutterHandlerEntry {
    pub(crate) side: GutterSide,
    pub(crate) priority: i16,
    pub(crate) handler: ErasedAnnotateGutterHandler,
}

/// A projection mode handler with descriptor and recovery metadata.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct ProjectionEntry {
    pub(crate) descriptor: ProjectionDescriptor,
    pub(crate) handler: ErasedDisplayHandler,
    pub(crate) recovery: DisplayRecoveryStatus,
}

// =============================================================================
// Transparency flags (ADR-030 Level 3 + Level 5)
// =============================================================================

/// Tracks which handler slots were registered via their `_transparent`
/// variant. When all registered handlers are transparent, the plugin
/// satisfies T10 (Plugin Transparency) by construction.
///
/// Level 3 covered input handlers. Level 5 extends to lifecycle handlers.
#[derive(Debug, Default)]
pub(crate) struct TransparencyFlags {
    // --- Input handlers (Level 3) ---
    pub(crate) key_handler: bool,
    pub(crate) key_middleware: bool,
    pub(crate) text_input: bool,
    pub(crate) mouse_handler: bool,
    pub(crate) drop_handler: bool,
    // --- Lifecycle handlers (Level 5) ---
    pub(crate) init_handler: bool,
    pub(crate) session_ready_handler: bool,
    pub(crate) state_changed_handler: bool,
    pub(crate) io_event_handler: bool,
    pub(crate) update_handler: bool,
    // --- Command-error / Pub-sub effects ---
    pub(crate) command_error_handler: bool,
    pub(crate) subscription_handler: bool,
}

impl TransparencyFlags {
    /// Returns true if every registered input handler used a transparent variant.
    ///
    /// For each handler slot, either (a) no handler is registered (the slot is
    /// `None` in `HandlerTable`), or (b) the handler was registered via a
    /// `_transparent` method.
    pub(crate) fn is_all_input_transparent(&self, table: &HandlerTable) -> bool {
        let key_ok = table.key_handler.is_none() || self.key_handler;
        let middleware_ok = table.key_middleware_handler.is_none() || self.key_middleware;
        let text_ok = table.text_input_handler.is_none() || self.text_input;
        let mouse_ok = table.handle_mouse_handler.is_none() || self.mouse_handler;
        let drop_ok = table.handle_drop_handler.is_none() || self.drop_handler;
        key_ok && middleware_ok && text_ok && mouse_ok && drop_ok
    }

    /// Returns true if every registered lifecycle handler used a transparent variant.
    ///
    /// Lifecycle handlers that produce `Effects` are: init, session_ready,
    /// state_changed, io_event, update, and process tasks.
    /// `on_workspace_changed` and `on_shutdown` are inherently transparent
    /// (they don't return `Effects`).
    pub(crate) fn is_all_lifecycle_transparent(&self, table: &HandlerTable) -> bool {
        let init_ok = table.init_handler.is_none() || self.init_handler;
        let session_ok = table.session_ready_handler.is_none() || self.session_ready_handler;
        let state_ok = table.state_changed_handler.is_none() || self.state_changed_handler;
        let io_ok = table.io_event_handler.is_none() || self.io_event_handler;
        let update_ok = table.update_handler.is_none() || self.update_handler;
        let tasks_ok = table.process_tasks.iter().all(|t| t.transparent);
        init_ok && session_ok && state_ok && io_ok && update_ok && tasks_ok
    }

    /// Returns true if ALL handler slots (input + lifecycle) are transparent.
    pub(crate) fn is_fully_transparent(&self, table: &HandlerTable) -> bool {
        self.is_all_input_transparent(table) && self.is_all_lifecycle_transparent(table)
    }
}

// =============================================================================
// Recovery flags (ADR-030 Level 4)
// =============================================================================

/// Tracks whether a plugin's display directives satisfy Visual Faithfulness (§10.2a).
#[derive(Debug, Default)]
pub(crate) enum DisplayRecoveryStatus {
    /// No display handler registered.
    #[default]
    NotRegistered,
    /// Handler uses `SafeDisplayDirective` — cannot emit Hide.
    NonDestructive,
    /// Handler may emit Hide, but recovery evidence was provided.
    #[allow(dead_code)] // witness value is stored for future diagnostic / introspection use
    Witnessed(super::recovery_witness::RecoveryWitness),
    /// Handler may emit Hide, no recovery evidence.
    Unwitnessed,
}

/// Per-plugin recovery flags for display directives.
#[derive(Debug, Default)]
pub(crate) struct RecoveryFlags {
    pub(crate) display: DisplayRecoveryStatus,
}

impl RecoveryFlags {
    /// Whether this plugin's display directives satisfy Visual Faithfulness (§10.2a).
    ///
    /// Returns `false` only for `Unwitnessed` — all other states are faithful.
    pub(crate) fn is_visually_faithful(&self) -> bool {
        !matches!(self.display, DisplayRecoveryStatus::Unwitnessed)
    }
}

// =============================================================================
// HandlerTable
// =============================================================================

/// Type-erased dispatch table for a single plugin's handlers.
///
/// Produced by [`HandlerRegistry::into_table()`]. Each field corresponds to
/// an extension point category. `None` / empty means the plugin did not
/// register a handler for that category.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct HandlerTable {
    // --- Lifecycle ---
    pub(crate) init_handler: Option<ErasedInitHandler>,
    pub(crate) session_ready_handler: Option<ErasedSessionReadyHandler>,
    pub(crate) state_changed_handler: Option<ErasedStateChangedHandler>,
    pub(crate) io_event_handler: Option<ErasedIoEventHandler>,
    pub(crate) workspace_changed_handler: Option<ErasedWorkspaceChangedHandler>,
    pub(crate) workspace_save_handler: Option<ErasedWorkspaceSaveHandler>,
    pub(crate) workspace_restore_handler: Option<ErasedWorkspaceRestoreHandler>,
    pub(crate) persist_state_handler: Option<ErasedPersistStateHandler>,
    pub(crate) restore_state_handler: Option<ErasedRestoreStateHandler>,
    pub(crate) shutdown_handler: Option<ErasedShutdownHandler>,
    pub(crate) update_handler: Option<ErasedUpdateHandler>,

    // --- Command-error / Pub-sub effects ---
    pub(crate) command_error_handler: Option<ErasedCommandErrorHandler>,
    pub(crate) subscription_handler: Option<ErasedSubscriptionHandler>,

    // --- Input ---
    pub(crate) key_handler: Option<ErasedKeyHandler>,
    pub(crate) key_middleware_handler: Option<ErasedKeyMiddlewareHandler>,
    pub(crate) observe_key_handler: Option<ErasedObserveKeyHandler>,
    pub(crate) observe_text_input_handler: Option<ErasedObserveTextInputHandler>,
    pub(crate) text_input_handler: Option<ErasedTextInputHandler>,
    pub(crate) observe_mouse_handler: Option<ErasedObserveMouseHandler>,
    pub(crate) handle_mouse_handler: Option<ErasedHandleMouseHandler>,
    pub(crate) observe_drop_handler: Option<ErasedObserveDropHandler>,
    pub(crate) handle_drop_handler: Option<ErasedHandleDropHandler>,
    pub(crate) default_scroll_handler: Option<ErasedDefaultScrollHandler>,
    pub(crate) key_pre_dispatch_handler: Option<ErasedKeyPreDispatchHandler>,
    pub(crate) mouse_pre_dispatch_handler: Option<ErasedMousePreDispatchHandler>,
    pub(crate) text_input_pre_dispatch_handler: Option<ErasedTextInputPreDispatchHandler>,
    pub(crate) mouse_fallback_handler: Option<ErasedMouseFallbackHandler>,

    // --- Key Map (Phase 2) ---
    pub(crate) key_map: Option<CompiledKeyMap>,
    pub(crate) key_map_builder: Option<ErasedKeyMapBuilder>,
    pub(crate) action_handler: Option<ErasedActionHandler>,
    pub(crate) group_refresh_handler: Option<ErasedGroupRefreshHandler>,

    // --- View ---
    pub(crate) contribute_handlers: Vec<ContributeEntry>,
    pub(crate) contribute_any_handler: Option<ContributeAnyEntry>,
    pub(crate) transform_handler: Option<TransformEntry>,
    pub(crate) gutter_handlers: Vec<GutterHandlerEntry>,
    pub(crate) background_handler: Option<ErasedAnnotateBackgroundHandler>,
    pub(crate) inline_handler: Option<ErasedAnnotateInlineHandler>,
    pub(crate) virtual_text_handler: Option<ErasedVirtualTextHandler>,
    /// Monolithic line-annotation handler. When set, the bridge dispatches
    /// `annotate_line_with_ctx` through this single closure instead of the
    /// per-concern decomposed setters. Used by adapters whose underlying
    /// contract surfaces all annotation parts (gutter / background /
    /// inline / virtual text) in one call — primarily WASM plugins via
    /// the `annotate-line` WIT export.
    pub(crate) annotate_line_handler: Option<ErasedAnnotateLineHandler>,
    pub(crate) overlay_handler: Option<ErasedOverlayHandler>,
    pub(crate) display_handler: Option<ErasedDisplayHandler>,
    pub(crate) projection_entries: Vec<ProjectionEntry>,
    pub(crate) content_annotation_handler: Option<ErasedContentAnnotationHandler>,
    pub(crate) render_ornament_handler: Option<ErasedRenderOrnamentHandler>,
    pub(crate) unified_display_handler: Option<ErasedUnifiedDisplayHandler>,
    pub(crate) menu_transform_handler: Option<ErasedMenuTransformHandler>,

    // --- Scroll Offset ---
    pub(crate) display_scroll_offset_handler: Option<ErasedDisplayScrollOffsetHandler>,

    // --- Renderer Extension Points ---
    pub(crate) menu_renderer_handler: Option<ErasedMenuRendererHandler>,
    pub(crate) info_renderer_handler: Option<ErasedInfoRendererHandler>,

    // --- Navigation (DU-4) ---
    pub(crate) navigation_policy_handler: Option<ErasedNavigationPolicyHandler>,
    pub(crate) navigation_action_handler: Option<ErasedNavigationActionHandler>,

    // --- Virtual Edit (BDT) ---
    pub(crate) virtual_edit_handler: Option<ErasedVirtualEditHandler>,

    // --- Buffer-edit intercept (ADR-035 ShadowCursor follow-up) ---
    pub(crate) buffer_edit_intercept_handler: Option<ErasedBufferEditInterceptHandler>,

    // --- Inline-box paint (ADR-031 Phase 10) ---
    pub(crate) inline_box_paint_handler: Option<ErasedInlineBoxPaintHandler>,

    // --- Pub/Sub ---
    pub(crate) publishers: Vec<PublishEntry>,
    pub(crate) subscribers: Vec<SubscribeEntry>,

    // --- Process Tasks ---
    pub(crate) process_tasks: Vec<ProcessTaskEntry>,

    // --- Surface declarations ---
    pub(crate) surfaces_factory: Option<ErasedSurfacesFactory>,
    pub(crate) workspace_request: Option<crate::workspace::Placement>,

    // --- Process-spawn policy ---
    /// `true` (default) lets `PluginRuntime::plugin_allows_process_spawn` return
    /// allow; `false` opts the plugin out of process spawning entirely.
    pub(crate) allows_process_spawn: bool,

    /// Optional override for the auto-inferred capabilities. Used by
    /// adapters whose capability set is sourced from an external manifest
    /// or WIT export — primarily WASM plugins via `register-capabilities`.
    /// When `Some`, takes precedence over `Self::capabilities()`.
    pub(crate) capabilities_override: Option<PluginCapabilities>,

    /// Optional override for the auto-derived capability descriptor.
    /// Used by adapters whose capability descriptor is sourced from an
    /// external manifest — primarily WASM plugins.
    pub(crate) capability_descriptor_override: Option<super::CapabilityDescriptor>,

    /// Optional override for `PluginBridge::state_hash()`. Used by
    /// adapters whose authoritative change-detection signal is external
    /// to the framework's `PluginState` — primarily WASM plugins, which
    /// run their own state inside the wasmtime store and surface a
    /// per-call hash via the `state-hash` WIT export. When set, takes
    /// precedence over the bridge's per-mutation generation counter.
    pub(crate) state_hash_handler: Option<Box<dyn Fn() -> u64 + Send + Sync>>,

    // --- Host-resolved authorities ---
    /// Bitflags requested by the plugin at registration. Defaults to empty.
    pub(crate) authorities: super::PluginAuthorities,

    // --- Display directive priority ---
    /// Priority used when this plugin's `on_display*` handler emits a
    /// `DirectiveSet`. Default 0.
    pub(crate) display_priority: i16,

    // --- Lens declarations ---
    /// Factory invoked from `PluginBridge::register_lenses` during
    /// `PluginRuntime::sync_lenses`. Default: no lenses declared.
    pub(crate) lenses_factory: Option<ErasedLensFactory>,

    // --- Config ---
    pub(crate) interests: DirtyFlags,

    // --- Transparency (ADR-030 Level 3) ---
    pub(crate) transparency: TransparencyFlags,

    // --- Recovery (ADR-030 Level 4) ---
    pub(crate) recovery: RecoveryFlags,

    // --- Builtin suppression ---
    pub(crate) suppressed_builtins: std::collections::HashSet<super::BuiltinTarget>,
}

#[allow(dead_code)] // consumed by PluginBridge
impl HandlerTable {
    /// Create an empty handler table with no handlers registered.
    pub(crate) fn empty() -> Self {
        Self {
            init_handler: None,
            session_ready_handler: None,
            state_changed_handler: None,
            io_event_handler: None,
            workspace_changed_handler: None,
            workspace_save_handler: None,
            workspace_restore_handler: None,
            persist_state_handler: None,
            restore_state_handler: None,
            shutdown_handler: None,
            update_handler: None,
            command_error_handler: None,
            subscription_handler: None,
            key_handler: None,
            key_middleware_handler: None,
            observe_key_handler: None,
            observe_text_input_handler: None,
            text_input_handler: None,
            observe_mouse_handler: None,
            handle_mouse_handler: None,
            observe_drop_handler: None,
            handle_drop_handler: None,
            default_scroll_handler: None,
            key_pre_dispatch_handler: None,
            mouse_pre_dispatch_handler: None,
            text_input_pre_dispatch_handler: None,
            mouse_fallback_handler: None,
            key_map: None,
            key_map_builder: None,
            action_handler: None,
            group_refresh_handler: None,
            contribute_handlers: Vec::new(),
            contribute_any_handler: None,
            transform_handler: None,
            gutter_handlers: Vec::new(),
            background_handler: None,
            inline_handler: None,
            annotate_line_handler: None,
            virtual_text_handler: None,
            overlay_handler: None,
            display_handler: None,
            projection_entries: Vec::new(),
            content_annotation_handler: None,
            render_ornament_handler: None,
            unified_display_handler: None,
            menu_transform_handler: None,
            display_scroll_offset_handler: None,
            menu_renderer_handler: None,
            info_renderer_handler: None,
            navigation_policy_handler: None,
            navigation_action_handler: None,
            virtual_edit_handler: None,
            buffer_edit_intercept_handler: None,
            inline_box_paint_handler: None,
            publishers: Vec::new(),
            subscribers: Vec::new(),
            process_tasks: Vec::new(),
            surfaces_factory: None,
            workspace_request: None,
            allows_process_spawn: true,
            capabilities_override: None,
            capability_descriptor_override: None,
            state_hash_handler: None,
            authorities: super::PluginAuthorities::empty(),
            display_priority: 0,
            lenses_factory: None,
            interests: DirtyFlags::ALL,
            transparency: TransparencyFlags::default(),
            recovery: RecoveryFlags::default(),
            suppressed_builtins: std::collections::HashSet::new(),
        }
    }

    /// Auto-inferred capabilities derived from which handlers are registered.
    ///
    /// NOTE: SURFACE_PROVIDER is not inferred here — it is declarative metadata
    /// only and is not used for dispatch gating.
    pub(crate) fn capabilities(&self) -> PluginCapabilities {
        let mut caps = PluginCapabilities::empty();
        if self.io_event_handler.is_some() || !self.process_tasks.is_empty() {
            caps |= PluginCapabilities::IO_HANDLER;
        }
        if self.workspace_changed_handler.is_some() {
            caps |= PluginCapabilities::WORKSPACE_OBSERVER;
        }
        if self.key_handler.is_some()
            || self.key_middleware_handler.is_some()
            || self.text_input_handler.is_some()
            || self.observe_key_handler.is_some()
            || self.observe_text_input_handler.is_some()
            || self.observe_mouse_handler.is_some()
            || self.handle_mouse_handler.is_some()
            || self.key_map.is_some()
        {
            caps |= PluginCapabilities::INPUT_HANDLER;
        }
        if self.handle_drop_handler.is_some() {
            caps |= PluginCapabilities::DROP_HANDLER;
        }
        if self.default_scroll_handler.is_some() {
            caps |= PluginCapabilities::SCROLL_POLICY;
        }
        if self.display_scroll_offset_handler.is_some() {
            caps |= PluginCapabilities::SCROLL_OFFSET;
        }
        if self.menu_renderer_handler.is_some() {
            caps |= PluginCapabilities::MENU_RENDERER;
        }
        if self.info_renderer_handler.is_some() {
            caps |= PluginCapabilities::INFO_RENDERER;
        }
        if !self.contribute_handlers.is_empty() || self.contribute_any_handler.is_some() {
            caps |= PluginCapabilities::CONTRIBUTOR;
        }
        if self.transform_handler.is_some() {
            caps |= PluginCapabilities::TRANSFORMER;
        }
        if self.has_annotation_handlers() || self.unified_display_handler.is_some() {
            caps |= PluginCapabilities::ANNOTATOR;
        }
        if self.overlay_handler.is_some() {
            caps |= PluginCapabilities::OVERLAY;
        }
        if self.display_handler.is_some()
            || self.unified_display_handler.is_some()
            || !self.projection_entries.is_empty()
        {
            caps |= PluginCapabilities::DISPLAY_TRANSFORM;
        }
        if self.content_annotation_handler.is_some() || self.unified_display_handler.is_some() {
            caps |= PluginCapabilities::CONTENT_ANNOTATOR;
        }
        if self.render_ornament_handler.is_some() {
            caps |= PluginCapabilities::RENDER_ORNAMENT;
        }
        if self.menu_transform_handler.is_some() {
            caps |= PluginCapabilities::MENU_TRANSFORM;
        }
        if self.navigation_policy_handler.is_some() {
            caps |= PluginCapabilities::NAVIGATION_POLICY;
        }
        if self.navigation_action_handler.is_some() {
            caps |= PluginCapabilities::NAVIGATION_ACTION;
        }
        if self.inline_box_paint_handler.is_some() {
            caps |= PluginCapabilities::INLINE_BOX_PAINTER;
        }
        if self.key_pre_dispatch_handler.is_some() {
            caps |= PluginCapabilities::KEY_PRE_DISPATCH;
        }
        if self.mouse_pre_dispatch_handler.is_some() {
            caps |= PluginCapabilities::MOUSE_PRE_DISPATCH;
        }
        if self.mouse_fallback_handler.is_some() {
            caps |= PluginCapabilities::MOUSE_FALLBACK;
        }
        caps
    }

    /// Declared dirty flag interests.
    pub(crate) fn interests(&self) -> DirtyFlags {
        self.interests
    }

    /// Returns true if any annotation handler (gutter, background, inline, virtual text)
    /// is registered.
    pub(crate) fn has_annotation_handlers(&self) -> bool {
        !self.gutter_handlers.is_empty()
            || self.background_handler.is_some()
            || self.inline_handler.is_some()
            || self.virtual_text_handler.is_some()
            || self.annotate_line_handler.is_some()
    }

    /// Infer a [`CapabilityDescriptor`] from registered handlers.
    pub(crate) fn capability_descriptor(&self) -> super::CapabilityDescriptor {
        use super::{AnnotationScope, CapabilityDescriptor};

        let contribution_slots: Vec<super::SlotId> = self
            .contribute_handlers
            .iter()
            .map(|e| e.slot.clone())
            .collect();

        let mut annotation_scopes = Vec::new();
        for gh in &self.gutter_handlers {
            match gh.side {
                GutterSide::Left => {
                    if !annotation_scopes.contains(&AnnotationScope::LeftGutter) {
                        annotation_scopes.push(AnnotationScope::LeftGutter);
                    }
                }
                GutterSide::Right => {
                    if !annotation_scopes.contains(&AnnotationScope::RightGutter) {
                        annotation_scopes.push(AnnotationScope::RightGutter);
                    }
                }
            }
        }
        if self.background_handler.is_some() {
            annotation_scopes.push(AnnotationScope::Background);
        }
        if self.inline_handler.is_some() {
            annotation_scopes.push(AnnotationScope::Inline);
        }
        if self.virtual_text_handler.is_some() {
            annotation_scopes.push(AnnotationScope::VirtualText);
        }

        let publish_topics: Vec<super::pubsub::TopicId> =
            self.publishers.iter().map(|e| e.topic.clone()).collect();
        let subscribe_topics: Vec<super::pubsub::TopicId> =
            self.subscribers.iter().map(|e| e.topic.clone()).collect();

        CapabilityDescriptor {
            transform_targets: self
                .transform_handler
                .as_ref()
                .map(|e| e.targets.clone())
                .unwrap_or_default(),
            contribution_slots,
            annotation_scopes,
            publish_topics,
            subscribe_topics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_has_no_capabilities() {
        let table = HandlerTable::empty();
        assert_eq!(table.capabilities(), PluginCapabilities::empty());
    }

    #[test]
    fn empty_table_has_all_interests() {
        let table = HandlerTable::empty();
        assert_eq!(table.interests(), DirtyFlags::ALL);
    }

    #[test]
    fn empty_table_has_no_annotation_handlers() {
        let table = HandlerTable::empty();
        assert!(!table.has_annotation_handlers());
    }

    #[test]
    fn drop_handler_sets_capability() {
        let mut table = HandlerTable::empty();
        table.handle_drop_handler = Some(Box::new(|_state, _event, _id, _app| None));
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::DROP_HANDLER)
        );
    }

    #[test]
    fn navigation_policy_handler_sets_capability() {
        let mut table = HandlerTable::empty();
        table.navigation_policy_handler = Some(Box::new(|_state, _unit| NavigationPolicy::Normal));
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::NAVIGATION_POLICY)
        );
    }

    #[test]
    fn navigation_action_handler_sets_capability() {
        let mut table = HandlerTable::empty();
        table.navigation_action_handler = Some(Box::new(|state, _unit, _action| {
            (dyn_clone::clone_box(state), ActionResult::Pass)
        }));
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::NAVIGATION_ACTION)
        );
    }

    #[test]
    fn render_ornament_handler_sets_capability() {
        let mut table = HandlerTable::empty();
        table.render_ornament_handler =
            Some(Box::new(|_state, _app, _ctx| OrnamentBatch::default()));
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::RENDER_ORNAMENT)
        );
    }
}
