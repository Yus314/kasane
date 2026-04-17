//! Type-erased handler dispatch table.
//!
//! `HandlerTable` stores handler closures produced by [`HandlerRegistry::into_table()`]
//! after type erasure. Each field holds an optional handler (or vec of handlers) for
//! a specific extension point category.
//!
//! This module is framework-internal. Plugin authors interact with
//! [`HandlerRegistry`](super::handler_registry::HandlerRegistry) instead.

use std::any::Any;

use crate::element::{Element, InteractiveId};
use crate::input::{CompiledKeyMap, DropEvent, KeyEvent, KeyResponse, MouseEvent};
use crate::protocol::Atom;
use crate::render::InlineDecoration;
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use crate::display::unit::DisplayUnit;

use crate::display::projection::ProjectionDescriptor;

use super::element_patch::ElementPatch;
use super::extension_point::{ExtensionContribution, ExtensionDefinition};
use super::process_task::ProcessTaskEntry;
use super::pubsub::{PublishEntry, SubscribeEntry};
use super::traits::KeyHandleResult;
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
pub(crate) type ErasedShutdownHandler = Box<dyn Fn(&dyn PluginState) + Send + Sync>;
pub(crate) type ErasedUpdateHandler = Box<
    dyn Fn(&dyn PluginState, &mut dyn Any, &AppView<'_>) -> (Box<dyn PluginState>, Effects)
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
pub(crate) type ErasedRenderOrnamentHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>, &RenderOrnamentContext) -> OrnamentBatch + Send + Sync,
>;
pub(crate) type ErasedMenuTransformHandler = Box<
    dyn Fn(&dyn PluginState, &[Atom], usize, bool, &AppView<'_>) -> Option<Vec<Atom>> + Send + Sync,
>;

// Navigation handlers (DU-4)
pub(crate) type ErasedNavigationPolicyHandler =
    Box<dyn Fn(&dyn PluginState, &DisplayUnit) -> NavigationPolicy + Send + Sync>;
pub(crate) type ErasedNavigationActionHandler = Box<
    dyn Fn(&dyn PluginState, &DisplayUnit, NavigationAction) -> (Box<dyn PluginState>, ActionResult)
        + Send
        + Sync,
>;

// =============================================================================
// Handler entry types (handler + metadata)
// =============================================================================

/// A contribute handler bound to a specific slot.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct ContributeEntry {
    pub(crate) slot: SlotId,
    pub(crate) handler: ErasedContributeHandler,
}

/// A transform handler with priority metadata.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct TransformEntry {
    pub(crate) priority: i16,
    pub(crate) targets: Vec<TransformTarget>,
    pub(crate) handler: ErasedTransformHandler,
}

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
    pub(crate) shutdown_handler: Option<ErasedShutdownHandler>,
    pub(crate) update_handler: Option<ErasedUpdateHandler>,

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

    // --- Key Map (Phase 2) ---
    pub(crate) key_map: Option<CompiledKeyMap>,
    pub(crate) key_map_builder: Option<ErasedKeyMapBuilder>,
    pub(crate) action_handler: Option<ErasedActionHandler>,
    pub(crate) group_refresh_handler: Option<ErasedGroupRefreshHandler>,

    // --- View ---
    pub(crate) contribute_handlers: Vec<ContributeEntry>,
    pub(crate) transform_handler: Option<TransformEntry>,
    pub(crate) gutter_handlers: Vec<GutterHandlerEntry>,
    pub(crate) background_handler: Option<ErasedAnnotateBackgroundHandler>,
    pub(crate) inline_handler: Option<ErasedAnnotateInlineHandler>,
    pub(crate) virtual_text_handler: Option<ErasedVirtualTextHandler>,
    pub(crate) overlay_handler: Option<ErasedOverlayHandler>,
    pub(crate) display_handler: Option<ErasedDisplayHandler>,
    pub(crate) projection_entries: Vec<ProjectionEntry>,
    pub(crate) render_ornament_handler: Option<ErasedRenderOrnamentHandler>,
    pub(crate) menu_transform_handler: Option<ErasedMenuTransformHandler>,

    // --- Navigation (DU-4) ---
    pub(crate) navigation_policy_handler: Option<ErasedNavigationPolicyHandler>,
    pub(crate) navigation_action_handler: Option<ErasedNavigationActionHandler>,

    // --- Pub/Sub ---
    pub(crate) publishers: Vec<PublishEntry>,
    pub(crate) subscribers: Vec<SubscribeEntry>,

    // --- Extension Points ---
    pub(crate) extension_definitions: Vec<ExtensionDefinition>,
    pub(crate) extension_contributions: Vec<ExtensionContribution>,

    // --- Process Tasks ---
    pub(crate) process_tasks: Vec<ProcessTaskEntry>,

    // --- Config ---
    pub(crate) interests: DirtyFlags,

    // --- Transparency (ADR-030 Level 3) ---
    pub(crate) transparency: TransparencyFlags,

    // --- Recovery (ADR-030 Level 4) ---
    pub(crate) recovery: RecoveryFlags,
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
            shutdown_handler: None,
            update_handler: None,
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
            key_map: None,
            key_map_builder: None,
            action_handler: None,
            group_refresh_handler: None,
            contribute_handlers: Vec::new(),
            transform_handler: None,
            gutter_handlers: Vec::new(),
            background_handler: None,
            inline_handler: None,
            virtual_text_handler: None,
            overlay_handler: None,
            display_handler: None,
            projection_entries: Vec::new(),
            render_ornament_handler: None,
            menu_transform_handler: None,
            navigation_policy_handler: None,
            navigation_action_handler: None,
            publishers: Vec::new(),
            subscribers: Vec::new(),
            extension_definitions: Vec::new(),
            extension_contributions: Vec::new(),
            process_tasks: Vec::new(),
            interests: DirtyFlags::ALL,
            transparency: TransparencyFlags::default(),
            recovery: RecoveryFlags::default(),
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
            || self.observe_text_input_handler.is_some()
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
        if !self.contribute_handlers.is_empty() {
            caps |= PluginCapabilities::CONTRIBUTOR;
        }
        if self.transform_handler.is_some() {
            caps |= PluginCapabilities::TRANSFORMER;
        }
        if self.has_annotation_handlers() {
            caps |= PluginCapabilities::ANNOTATOR;
        }
        if self.overlay_handler.is_some() {
            caps |= PluginCapabilities::OVERLAY;
        }
        if self.display_handler.is_some() || !self.projection_entries.is_empty() {
            caps |= PluginCapabilities::DISPLAY_TRANSFORM;
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

        let extensions_defined: Vec<super::extension_point::ExtensionPointId> = self
            .extension_definitions
            .iter()
            .map(|e| e.id.clone())
            .collect();
        let extensions_consumed: Vec<super::extension_point::ExtensionPointId> = self
            .extension_contributions
            .iter()
            .map(|e| e.id.clone())
            .collect();

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
            extensions_defined,
            extensions_consumed,
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
