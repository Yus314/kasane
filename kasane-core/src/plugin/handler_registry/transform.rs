//! Contribution and transform-chain handlers.

use crate::protocol::Atom;

use super::super::element_patch::ElementPatch;
use super::super::handler_table::{ContributeEntry, TransformEntry};
use super::super::{
    AppView, ContributeContext, Contribution, PluginState, SlotId, TransformContext,
    TransformTarget,
};

use super::HandlerRegistry;

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    pub fn on_contribute(
        &mut self,
        slot: SlotId,
        handler: impl Fn(&S, &AppView<'_>, &ContributeContext) -> Option<Contribution>
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  app: &AppView<'_>,
                  ctx: &ContributeContext|
                  -> Option<Contribution> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app, ctx)
            },
        );
        self.table.contribute_handlers.push(ContributeEntry {
            slot,
            handler: erased,
        });
    }

    /// Register a transform handler with priority.
    ///
    /// The handler returns an [`ElementPatch`] describing the declarative transform.
    /// Higher priority = applied earlier (inner position in the chain).
    pub fn on_transform(
        &mut self,
        priority: i16,
        handler: impl Fn(&S, &TransformTarget, &AppView<'_>, &TransformContext) -> ElementPatch
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  target: &TransformTarget,
                  app: &AppView<'_>,
                  ctx: &TransformContext|
                  -> ElementPatch {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, target, app, ctx)
            },
        );
        self.table.transform_handler = Some(TransformEntry {
            priority,
            targets: Vec::new(),
            handler: erased,
        });
    }

    /// Register a transform handler for specific targets.
    ///
    /// Unlike [`on_transform()`], this specifies which targets the transform applies to.
    /// The `targets` list is exposed via [`CapabilityDescriptor::transform_targets`],
    /// enabling `may_interfere()` to detect transform target overlap.
    pub fn on_transform_for(
        &mut self,
        priority: i16,
        targets: &[TransformTarget],
        handler: impl Fn(&S, &TransformTarget, &AppView<'_>, &TransformContext) -> ElementPatch
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  target: &TransformTarget,
                  app: &AppView<'_>,
                  ctx: &TransformContext|
                  -> ElementPatch {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, target, app, ctx)
            },
        );
        self.table.transform_handler = Some(TransformEntry {
            priority,
            targets: targets.to_vec(),
            handler: erased,
        });
    }

    /// Register a gutter annotation handler.
    ///
    /// `side` determines left or right gutter placement. `priority` controls
    /// sort ordering (lower = further left within the same side).
    pub fn on_menu_transform(
        &mut self,
        handler: impl Fn(&S, &[Atom], usize, bool, &AppView<'_>) -> Option<Vec<Atom>>
        + Send
        + Sync
        + 'static,
    ) {
        register_view!(
            self,
            menu_transform_handler,
            handler,
            item,
            index,
            selected,
            app
        );
    }
}
