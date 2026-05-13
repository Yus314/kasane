//! Carve-outs on the decoration / display axis.
//!
//! γ-3.3c-5b: the redundant manual `on_gutter` / `on_background` /
//! `on_inline` / `on_virtual_text` / `on_annotate_line` / `on_overlay` /
//! `on_display{,_safe,_witnessed}` / `on_unified_display{,_safe}` /
//! `on_content_annotation` / `on_render_ornament` setters were retired —
//! plugin code now invokes the macro-generated counterparts via `Deref`
//! from `HandlerRegistry` to `gen::HandlerRegistry`. The carve-out
//! `define_projection` / `define_additive_projection` setters (spec §9.1)
//! stay manual: they take both a [`ProjectionDescriptor`](crate::display::ProjectionDescriptor)
//! and a handler closure, and derive [`DisplayRecoveryStatus`] from the
//! descriptor's `Structural` / `Additive` category — the bespoke
//! recovery-inference is the carve-out point. The
//! [`is_display_recoverable`](HandlerRegistry::is_display_recoverable)
//! query method also stays here.

use super::super::{AppView, DisplayDirective, PluginState};

use super::HandlerRegistry;

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    /// Whether this plugin's display directives satisfy Visual Faithfulness (§10.2a).
    ///
    /// Both the singleton `display` and `unified_display` slots must be
    /// recoverable (or unregistered).
    pub fn is_display_recoverable(&self) -> bool {
        super::super::handler_table::is_visually_faithful(&self.inner.table.display_recovery)
            && super::super::handler_table::is_visually_faithful(
                &self.inner.table.unified_display_recovery,
            )
    }

    /// Define a named projection mode.
    ///
    /// - **Structural** projections auto-create a `RecoveryWitness::Declared` since
    ///   switching structural projections is the built-in recovery mechanism.
    /// - **Additive** projections are marked `NonDestructive`.
    pub fn define_projection(
        &mut self,
        descriptor: crate::display::ProjectionDescriptor,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<DisplayDirective> + Send + Sync + 'static,
    ) {
        use super::super::handler_table::DisplayRecoveryStatus;
        use crate::display::ProjectionCategory;

        let recovery = match descriptor.category {
            ProjectionCategory::Structural => {
                DisplayRecoveryStatus::Witnessed(super::super::RecoveryWitness {
                    mechanism: super::super::RecoveryMechanism::Declared {
                        description: "projection mode switch",
                    },
                })
            }
            ProjectionCategory::Additive => DisplayRecoveryStatus::NonDestructive,
        };

        self.on_projection(descriptor, recovery, handler);
    }

    /// Define a named additive projection with compile-time non-destructive guarantee.
    ///
    /// The handler returns `Vec<SafeDisplayDirective>` (no Hide), making
    /// non-destructiveness a compile-time property.
    pub fn define_additive_projection(
        &mut self,
        descriptor: crate::display::ProjectionDescriptor,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<super::super::SafeDisplayDirective>
        + Send
        + Sync
        + 'static,
    ) {
        use super::super::handler_table::DisplayRecoveryStatus;
        use crate::display::ProjectionCategory;

        assert!(
            descriptor.category == ProjectionCategory::Additive,
            "define_additive_projection requires Additive category, got {:?}",
            descriptor.category,
        );

        self.on_projection(
            descriptor,
            DisplayRecoveryStatus::NonDestructive,
            move |state, app| handler(state, app).into_iter().map(Into::into).collect(),
        );
    }
}
