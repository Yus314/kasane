//! Type-erased handler dispatch table.
//!
//! γ-3.3d: this module is a thin wrapper around the macro-generated
//! `crate::plugin::handler_table_spec::generated::HandlerTable`. It
//! re-exports the spec module's entry-struct shapes, transparency
//! flags, and recovery status enum, and adds the two carve-out
//! storage fields (`process_tasks`, `transform_full_handler`) that
//! cannot live in the generated table because their shapes don't
//! generalize. All accesses to non-carve-out fields auto-deref
//! through to the inner generated table — setters and dispatch sites
//! see a uniform `HandlerTable` API.
//!
//! Plugin authors interact with
//! [`HandlerRegistry`](super::handler_registry::HandlerRegistry); the
//! `HandlerTable` produced by [`HandlerRegistry::into_table()`] is
//! consumed by `PluginBridge`.

use std::ops::{Deref, DerefMut};

use crate::state::DirtyFlags;

use super::process_task::ProcessTaskEntry;
use super::{AppView, PluginCapabilities, PluginState, SlotId, TransformContext, TransformTarget};

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
// Erased handler type aliases — only the carve-out alias remains
// =============================================================================
//
// γ-3.3d: ~50 manual `Erased<Name>Handler` aliases were retired. After
// the entry-struct re-exports in γ-3.3c-4b made
// `gen::Erased<Name>Handler` the canonical owners, the manual aliases
// duplicated the same boxed-fn signatures and had no remaining
// in-tree consumers. The `ErasedFullTransformHandler` alias below
// stays because it backs the `full_fallback` carve-out (spec §9.4) —
// the macro deliberately rejects the modifier (its `TransformSubject`
// signature does not generalize) and the manual side hand-writes the
// setter in `handler_registry/transform.rs::on_transform_full`.

/// Imperative full-rewrite transform handler (`full_fallback`
/// carve-out, spec §9.4). See [`HandlerTable::transform_full_handler`].
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

// =============================================================================
// Re-exports from the generated module (canonical owners)
// =============================================================================

#[allow(unused_imports)]
pub(crate) use crate::plugin::handler_table_spec::generated::{
    ContributeEntry, DisplayRecoveryStatus, GutterEntry, ProjectionEntry, TransformEntry,
    TransparencyFlags,
};

// =============================================================================
// HandlerTable
// =============================================================================

/// Type-erased dispatch table for a single plugin's handlers.
///
/// γ-3.3c-4c: this struct now wraps the macro-generated
/// `gen::HandlerTable` plus the two carve-out fields whose storage
/// shape doesn't generalize through the spec module
/// (`process_tasks`, `transform_full_handler`). All non-carve-out
/// field accesses (`self.table.<field>`) auto-resolve through
/// `Deref`/`DerefMut` to the underlying generated table — setters and
/// dispatch sites are unaware of the indirection.
#[allow(dead_code)] // consumed by PluginBridge
pub(crate) struct HandlerTable {
    /// Generated dispatch table — the canonical source of truth for
    /// every spec entry's handler storage, transparency tracking, and
    /// recovery status.
    pub(crate) generated: crate::plugin::handler_table_spec::generated::HandlerTable,

    // --- Carve-outs (§9 of `docs/handler-table-dsl.md`) ---
    /// Vec-of-metadata Lifecycle storage for the `process_task`
    /// carve-out (§9.5). Keyed by `&'static str` with a per-task
    /// `ProcessTaskSpec` payload + `streaming`/`transparent` flags.
    pub(crate) process_tasks: Vec<ProcessTaskEntry>,
    /// Imperative full-rewrite companion to `transform_handler`
    /// (`full_fallback` carve-out, spec §9.4). The dispatch path
    /// consults the declarative patch handler first, then this
    /// full handler if the patch is `ElementPatch::Identity`.
    pub(crate) transform_full_handler: Option<ErasedFullTransformHandler>,
}

impl Deref for HandlerTable {
    type Target = crate::plugin::handler_table_spec::generated::HandlerTable;
    fn deref(&self) -> &Self::Target {
        &self.generated
    }
}

impl DerefMut for HandlerTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.generated
    }
}

#[allow(dead_code)] // consumed by PluginBridge
impl HandlerTable {
    /// Create an empty handler table with no handlers registered.
    pub(crate) fn empty() -> Self {
        Self {
            generated: crate::plugin::handler_table_spec::generated::HandlerTable::empty(),
            process_tasks: Vec::new(),
            transform_full_handler: None,
        }
    }

    /// Returns true if every registered input handler used a transparent variant.
    ///
    /// Wraps the generated `is_all_input_transparent` predicate; no carve-out
    /// extension because the input axis has no carve-outs.
    pub(crate) fn is_input_transparent(&self) -> bool {
        self.generated
            .transparency
            .is_all_input_transparent(&self.generated)
    }

    /// Returns true if every registered lifecycle handler used a transparent variant.
    ///
    /// Combines the generated `is_all_lifecycle_transparent` predicate
    /// with the `process_tasks` carve-out — every registered task must
    /// have been registered via a `_transparent` setter.
    pub(crate) fn is_lifecycle_transparent(&self) -> bool {
        self.generated
            .transparency
            .is_all_lifecycle_transparent(&self.generated)
            && self.process_tasks.iter().all(|t| t.transparent)
    }

    /// Returns true if ALL handler slots (input + lifecycle) are transparent.
    pub(crate) fn is_fully_transparent(&self) -> bool {
        self.is_input_transparent() && self.is_lifecycle_transparent()
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
            || !self.projection_handlers.is_empty()
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
        if self.paint_inline_box_handler.is_some() {
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

        let contribution_slots: Vec<SlotId> = self
            .contribute_handlers
            .iter()
            .map(|e| e.key.clone())
            .collect();

        let mut annotation_scopes = Vec::new();
        for gh in &self.gutter_handlers {
            match gh.key {
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

        let publish_topics: Vec<super::pubsub::TopicId> = self
            .publish_handlers
            .iter()
            .map(|e| e.key.clone())
            .collect();
        let subscribe_topics: Vec<super::pubsub::TopicId> = self
            .subscribe_handlers
            .iter()
            .map(|e| e.key.clone())
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::navigation::{ActionResult, NavigationPolicy};
    use crate::plugin::OrnamentBatch;

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
        table.navigation_action_handler = Some(Box::new(|_state, _unit, _action| {
            (Box::new(()) as Box<dyn PluginState>, ActionResult::Pass)
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
