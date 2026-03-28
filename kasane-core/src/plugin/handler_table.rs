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
use crate::input::{CompiledKeyMap, KeyEvent, KeyResponse, MouseEvent};
use crate::protocol::Atom;
use crate::render::{CursorStyleHint, InlineDecoration};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::element_patch::ElementPatch;
use super::extension_point::{ExtensionContribution, ExtensionDefinition};
use super::pubsub::{PublishEntry, SubscribeEntry};
use super::traits::KeyHandleResult;
use super::{
    AnnotateContext, AppView, BackgroundLayer, BootstrapEffects, CellDecoration, Command,
    ContributeContext, Contribution, DisplayDirective, IoEvent, OverlayContext,
    OverlayContribution, PluginCapabilities, PluginState, RuntimeEffects, SessionReadyEffects,
    SlotId, TransformContext, TransformTarget, VirtualTextItem,
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
pub(crate) type ErasedInitHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>) -> (Box<dyn PluginState>, BootstrapEffects)
        + Send
        + Sync,
>;
pub(crate) type ErasedSessionReadyHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>) -> (Box<dyn PluginState>, SessionReadyEffects)
        + Send
        + Sync,
>;
pub(crate) type ErasedStateChangedHandler = Box<
    dyn Fn(&dyn PluginState, &AppView<'_>, DirtyFlags) -> (Box<dyn PluginState>, RuntimeEffects)
        + Send
        + Sync,
>;
pub(crate) type ErasedIoEventHandler = Box<
    dyn Fn(&dyn PluginState, &IoEvent, &AppView<'_>) -> (Box<dyn PluginState>, RuntimeEffects)
        + Send
        + Sync,
>;
pub(crate) type ErasedWorkspaceChangedHandler =
    Box<dyn Fn(&dyn PluginState, &WorkspaceQuery<'_>) -> Box<dyn PluginState> + Send + Sync>;
pub(crate) type ErasedShutdownHandler = Box<dyn Fn(&dyn PluginState) + Send + Sync>;
pub(crate) type ErasedUpdateHandler = Box<
    dyn Fn(&dyn PluginState, &mut dyn Any, &AppView<'_>) -> (Box<dyn PluginState>, RuntimeEffects)
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
pub(crate) type ErasedObserveMouseHandler =
    Box<dyn Fn(&dyn PluginState, &MouseEvent, &AppView<'_>) -> Box<dyn PluginState> + Send + Sync>;
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
pub(crate) type ErasedCellDecorationHandler =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>) -> Vec<CellDecoration> + Send + Sync>;
pub(crate) type ErasedCursorStyleHandler =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>) -> Option<CursorStyleHint> + Send + Sync>;
pub(crate) type ErasedMenuTransformHandler = Box<
    dyn Fn(&dyn PluginState, &[Atom], usize, bool, &AppView<'_>) -> Option<Vec<Atom>> + Send + Sync,
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
    pub(crate) handler: ErasedTransformHandler,
}

/// A gutter annotation handler with side and priority metadata.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct GutterHandlerEntry {
    pub(crate) side: GutterSide,
    pub(crate) priority: i16,
    pub(crate) handler: ErasedAnnotateGutterHandler,
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
    pub(crate) observe_mouse_handler: Option<ErasedObserveMouseHandler>,
    pub(crate) handle_mouse_handler: Option<ErasedHandleMouseHandler>,
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
    pub(crate) cell_decoration_handler: Option<ErasedCellDecorationHandler>,
    pub(crate) cursor_style_handler: Option<ErasedCursorStyleHandler>,
    pub(crate) menu_transform_handler: Option<ErasedMenuTransformHandler>,

    // --- Pub/Sub ---
    pub(crate) publishers: Vec<PublishEntry>,
    pub(crate) subscribers: Vec<SubscribeEntry>,

    // --- Extension Points ---
    pub(crate) extension_definitions: Vec<ExtensionDefinition>,
    pub(crate) extension_contributions: Vec<ExtensionContribution>,

    // --- Config ---
    pub(crate) interests: DirtyFlags,
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
            observe_mouse_handler: None,
            handle_mouse_handler: None,
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
            cell_decoration_handler: None,
            cursor_style_handler: None,
            menu_transform_handler: None,
            publishers: Vec::new(),
            subscribers: Vec::new(),
            extension_definitions: Vec::new(),
            extension_contributions: Vec::new(),
            interests: DirtyFlags::ALL,
        }
    }

    /// Auto-inferred capabilities derived from which handlers are registered.
    pub(crate) fn capabilities(&self) -> PluginCapabilities {
        let mut caps = PluginCapabilities::empty();
        if self.io_event_handler.is_some() {
            caps |= PluginCapabilities::IO_HANDLER;
        }
        if self.workspace_changed_handler.is_some() {
            caps |= PluginCapabilities::WORKSPACE_OBSERVER;
        }
        if self.key_handler.is_some()
            || self.key_middleware_handler.is_some()
            || self.handle_mouse_handler.is_some()
            || self.key_map.is_some()
        {
            caps |= PluginCapabilities::INPUT_HANDLER;
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
        if self.display_handler.is_some() {
            caps |= PluginCapabilities::DISPLAY_TRANSFORM;
        }
        if self.cell_decoration_handler.is_some() {
            caps |= PluginCapabilities::CELL_DECORATION;
        }
        if self.cursor_style_handler.is_some() {
            caps |= PluginCapabilities::CURSOR_STYLE;
        }
        if self.menu_transform_handler.is_some() {
            caps |= PluginCapabilities::MENU_TRANSFORM;
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
}
