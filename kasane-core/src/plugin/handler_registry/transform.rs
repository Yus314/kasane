//! Carve-outs on the contribute / transform axis.
//!
//! γ-3.3c-5b: the redundant manual `on_contribute` / `on_contribute_any` /
//! `on_menu_transform` setters were retired — plugin code now invokes
//! the macro-generated counterparts via `Deref` from `HandlerRegistry`
//! to `gen::HandlerRegistry`. The two manual setters retained are:
//!
//! - **`on_transform(priority, handler)`** — convenience over generated
//!   `on_transform(priority, targets, handler)` for the common case
//!   where the plugin doesn't need to declare specific transform
//!   targets (defaults to `Vec::new()`).
//! - **`on_transform_for(priority, targets, handler)`** — thin
//!   `&[TransformTarget]` → `Vec<TransformTarget>` adapter over the
//!   generated setter.
//! - **`on_transform_full`** — full-rewrite carve-out (spec §9.4).

use super::super::algebra::element_patch::ElementPatch;
use super::super::{AppView, PluginState, TransformContext, TransformTarget};

use super::HandlerRegistry;

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    /// Register a transform handler with priority (no specific targets).
    ///
    /// Convenience wrapper over the generated
    /// `on_transform(priority, targets, handler)` for plugins that
    /// don't declare specific transform targets. The handler returns an
    /// [`ElementPatch`] describing the declarative transform; higher
    /// priority = applied earlier (inner position in the chain).
    pub fn on_transform(
        &mut self,
        priority: i16,
        handler: impl Fn(&S, &TransformTarget, &AppView<'_>, &TransformContext) -> ElementPatch
        + Send
        + Sync
        + 'static,
    ) {
        // Shadows generated `on_transform` (which has 3 args). Disambiguate
        // by calling through `self.inner` so this method's body doesn't
        // recurse into itself.
        self.inner.on_transform(priority, Vec::new(), handler);
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
        self.inner.on_transform(priority, targets.to_vec(), handler);
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
        // The full handler is stored separately from the patch entry
        // (carve-out, spec §9.4) so registering the full handler does
        // not implicitly create a patch entry — the bridge falls back
        // to the full handler only when the patch path returns
        // `ElementPatch::Identity` (or no patch entry exists).
        self.transform_full_handler = Some(erased);
    }
}
