//! Decoration, display, content-annotation, and render-ornament handlers.

use crate::element::Element;
use crate::render::InlineDecoration;

use super::super::handler_table::{GutterHandlerEntry, GutterSide};
use super::super::{
    AnnotateContext, AppView, BackgroundLayer, DisplayDirective, OrnamentBatch, OverlayContext,
    OverlayContribution, PluginState, RenderOrnamentContext, VirtualTextItem,
};

use super::HandlerRegistry;

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    pub fn on_decorate_gutter(
        &mut self,
        side: GutterSide,
        priority: i16,
        handler: impl Fn(&S, usize, &AppView<'_>, &AnnotateContext) -> Option<Element>
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  line: usize,
                  app: &AppView<'_>,
                  ctx: &AnnotateContext|
                  -> Option<Element> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, line, app, ctx)
            },
        );
        self.table.gutter_handlers.push(GutterHandlerEntry {
            side,
            priority,
            handler: erased,
        });
    }

    /// Register a background annotation handler.
    pub fn on_decorate_background(
        &mut self,
        handler: impl Fn(&S, usize, &AppView<'_>, &AnnotateContext) -> Option<BackgroundLayer>
        + Send
        + Sync
        + 'static,
    ) {
        register_view!(self, background_handler, handler, line, app, ctx);
    }

    /// Register an inline decoration handler.
    pub fn on_decorate_inline(
        &mut self,
        handler: impl Fn(&S, usize, &AppView<'_>, &AnnotateContext) -> Option<InlineDecoration>
        + Send
        + Sync
        + 'static,
    ) {
        register_view!(self, inline_handler, handler, line, app, ctx);
    }

    /// Register a virtual text handler.
    pub fn on_virtual_text(
        &mut self,
        handler: impl Fn(&S, usize, &AppView<'_>, &AnnotateContext) -> Vec<VirtualTextItem>
        + Send
        + Sync
        + 'static,
    ) {
        register_view!(self, virtual_text_handler, handler, line, app, ctx);
    }

    /// Register an overlay contribution handler.
    pub fn on_overlay(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, &OverlayContext) -> Option<OverlayContribution>
        + Send
        + Sync
        + 'static,
    ) {
        register_view!(self, overlay_handler, handler, app, ctx);
    }

    /// Register a display directive handler.
    ///
    /// If the handler may emit `Hide` directives, consider using
    /// [`on_display_witnessed`](Self::on_display_witnessed) to provide recovery
    /// evidence, or [`on_display_safe`](Self::on_display_safe) if `Hide` is not
    /// needed.
    pub fn on_display(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<DisplayDirective> + Send + Sync + 'static,
    ) {
        register_view!(self, display_handler, handler, app);
        self.table.recovery.display =
            super::super::handler_table::DisplayRecoveryStatus::Unwitnessed;
    }

    /// Display handler that cannot emit Hide directives (compile-time safe).
    ///
    /// The handler returns `Vec<SafeDisplayDirective>`, which has no `Hide`
    /// constructor, making non-destructiveness a compile-time property.
    pub fn on_display_safe(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<super::super::SafeDisplayDirective>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.display_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app).into_iter().map(Into::into).collect()
        }));
        self.table.recovery.display =
            super::super::handler_table::DisplayRecoveryStatus::NonDestructive;
    }

    /// Display handler that may emit Hide, with recovery evidence.
    ///
    /// The caller provides a [`RecoveryWitness`](super::super::RecoveryWitness)
    /// documenting how the user can recover hidden content, satisfying
    /// Visual Faithfulness (§10.2a).
    pub fn on_display_witnessed(
        &mut self,
        witness: super::super::RecoveryWitness,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<DisplayDirective> + Send + Sync + 'static,
    ) {
        register_view!(self, display_handler, handler, app);
        self.table.recovery.display =
            super::super::handler_table::DisplayRecoveryStatus::Witnessed(witness);
    }

    /// Register a unified display handler that returns all directive categories.
    ///
    /// The unified handler replaces the 6 separate annotation/display handlers
    /// (gutter, background, inline, virtual text, content annotation, display).
    /// The framework partitions the returned directives by category and routes
    /// each to the correct resolution path.
    ///
    /// If the handler may emit `Hide` or `HideInline` directives, the
    /// recovery status is set to `Unwitnessed`.
    pub fn on_display_unified(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<DisplayDirective> + Send + Sync + 'static,
    ) {
        register_view!(self, unified_display_handler, handler, app);
        self.table.recovery.display =
            super::super::handler_table::DisplayRecoveryStatus::Unwitnessed;
    }

    /// Unified display handler that cannot emit destructive directives (compile-time safe).
    pub fn on_display_unified_safe(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<super::super::SafeDisplayDirective>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.unified_display_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app).into_iter().map(Into::into).collect()
        }));
        self.table.recovery.display =
            super::super::handler_table::DisplayRecoveryStatus::NonDestructive;
    }

    /// Whether this plugin's display directives satisfy Visual Faithfulness (§10.2a).
    pub fn is_display_recoverable(&self) -> bool {
        self.table.recovery.is_visually_faithful()
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
        use super::super::handler_table::{DisplayRecoveryStatus, ProjectionEntry};
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

        let erased: super::super::handler_table::ErasedDisplayHandler =
            Box::new(move |state, app| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app)
            });

        self.table.projection_entries.push(ProjectionEntry {
            descriptor,
            handler: erased,
            recovery,
        });
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
        use super::super::handler_table::{DisplayRecoveryStatus, ProjectionEntry};
        use crate::display::ProjectionCategory;

        assert!(
            descriptor.category == ProjectionCategory::Additive,
            "define_additive_projection requires Additive category, got {:?}",
            descriptor.category,
        );

        let erased: super::super::handler_table::ErasedDisplayHandler =
            Box::new(move |state, app| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app).into_iter().map(Into::into).collect()
            });

        self.table.projection_entries.push(ProjectionEntry {
            descriptor,
            handler: erased,
            recovery: DisplayRecoveryStatus::NonDestructive,
        });
    }

    /// Register a content annotation handler.
    ///
    /// Content annotations insert full `Element` trees between buffer lines
    /// (unlike display directives which only insert `Vec<Atom>` text).
    /// The handler is called once per frame and returns annotations for
    /// all relevant lines.
    ///
    /// Structurally additive — no safety tiers or RecoveryWitness needed.
    pub fn on_content_annotation(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, &AnnotateContext) -> Vec<crate::display::ContentAnnotation>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.content_annotation_handler = Some(Box::new(move |state, app, ctx| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app, ctx)
        }));
    }

    /// Register backend-independent physical ornament proposals.
    pub fn on_render_ornaments(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, &RenderOrnamentContext) -> OrnamentBatch
        + Send
        + Sync
        + 'static,
    ) {
        register_view!(self, render_ornament_handler, handler, app, ctx);
    }
}
