//! Contribution and transform-chain handlers.

use crate::protocol::Atom;

use super::super::algebra::element_patch::ElementPatch;
use super::super::handler_table::{ContributeAnyEntry, ContributeEntry, TransformEntry};
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

    /// Register a slot-agnostic contribute handler.
    ///
    /// Counterpart to [`Self::on_contribute`] for adapters whose
    /// underlying contract dispatches contribution requests for arbitrary
    /// slots — primarily WASM plugins, which delegate slot routing to the
    /// `contribute-to(region, …)` WIT export. The bridge consults
    /// [`Self::on_contribute`] entries first; the any-handler is the
    /// fallback when no slot-specific handler matches.
    pub fn on_contribute_any(
        &mut self,
        handler: impl Fn(&S, &SlotId, &AppView<'_>, &ContributeContext) -> Option<Contribution>
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  slot: &SlotId,
                  app: &AppView<'_>,
                  ctx: &ContributeContext|
                  -> Option<Contribution> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, slot, app, ctx)
            },
        );
        self.table.contribute_any_handler = Some(ContributeAnyEntry { handler: erased });
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
        let existing_full = self
            .table
            .transform_handler
            .take()
            .and_then(|prev| prev.full_handler);
        self.table.transform_handler = Some(TransformEntry {
            priority,
            targets: Vec::new(),
            handler: erased,
            full_handler: existing_full,
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
        let existing_full = self
            .table
            .transform_handler
            .take()
            .and_then(|prev| prev.full_handler);
        self.table.transform_handler = Some(TransformEntry {
            priority,
            targets: targets.to_vec(),
            handler: erased,
            full_handler: existing_full,
        });
    }

    /// Register an imperative full-rewrite transform handler.
    ///
    /// Counterpart to [`Self::on_transform`] for adapters whose
    /// underlying contract returns a transformed [`TransformSubject`]
    /// directly (e.g. WASM plugins via the `transform` WIT export when
    /// the plugin doesn't implement `transform-patch`). The bridge
    /// consults [`Self::on_transform`] (the patch path) first; if it
    /// resolves to [`ElementPatch::Identity`], this full handler runs
    /// as the fallback.
    ///
    /// Caller must register [`Self::on_transform`] first to set the
    /// priority. Calling [`Self::on_transform`] *after* this clears
    /// the full handler is intentional — overwriting the patch resets
    /// the entry — so register the full handler last when both are
    /// needed.
    pub fn on_transform_full(
        &mut self,
        handler: impl Fn(
            &S,
            &TransformTarget,
            super::super::TransformSubject,
            &AppView<'_>,
            &TransformContext,
        ) -> super::super::TransformSubject
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  target: &TransformTarget,
                  subject: super::super::TransformSubject,
                  app: &AppView<'_>,
                  ctx: &TransformContext|
                  -> super::super::TransformSubject {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, target, subject, app, ctx)
            },
        );
        let entry = self
            .table
            .transform_handler
            .get_or_insert_with(|| TransformEntry {
                priority: 0,
                targets: Vec::new(),
                handler: Box::new(|_, _, _, _| ElementPatch::Identity),
                full_handler: None,
            });
        entry.full_handler = Some(erased);
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
