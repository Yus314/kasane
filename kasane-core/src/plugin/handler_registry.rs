//! Type-safe handler registration for the `Plugin` trait architecture.
//!
//! [`HandlerRegistry`] provides typed registration methods that accept closures
//! parameterized over the plugin's concrete state type `S`. Calling
//! [`into_table()`](HandlerRegistry::into_table) performs type erasure and
//! produces a [`HandlerTable`] for framework-internal dispatch.
//!
//! # Example (Phase 2+)
//!
//! ```ignore
//! fn register(&self, r: &mut HandlerRegistry<MyState>) {
//!     r.declare_interests(DirtyFlags::BUFFER);
//!     r.on_state_changed(|state, app, dirty| {
//!         // ...
//!         (new_state, Effects::default())
//!     });
//!     r.on_decorate_background(|state, line, app, ctx| {
//!         // ...
//!         Some(BackgroundLayer { ... })
//!     });
//! }
//! ```

use std::any::Any;
use std::marker::PhantomData;

use serde::{Serialize, de::DeserializeOwned};

use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use crate::display::unit::DisplayUnit;
use crate::element::{Element, InteractiveId, Overlay};
use crate::input::{
    ChordBinding, CompiledKeyMap, DropEvent, KeyBinding, KeyEvent, KeyGroup, KeyPattern,
    KeyResponse, MouseEvent,
};
use crate::protocol::Atom;
use crate::render::InlineDecoration;
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::channel::ChannelValue;
use super::element_patch::ElementPatch;
use super::extension_point::{
    CompositionRule, ExtensionContribution, ExtensionDefinition, ExtensionPointId,
};
use super::handler_table::{
    ContributeEntry, GutterHandlerEntry, GutterSide, HandlerTable, TransformEntry,
};
use super::kakoune_safe_effects::KakouneSafeEffects;
use super::process_task::{ProcessTaskEntry, ProcessTaskResult, ProcessTaskSpec};
use super::pubsub::{PublishEntry, SubscribeEntry, Topic, TopicId};
use super::traits::KeyHandleResult;
use super::{
    AnnotateContext, AppView, BackgroundLayer, Command, ContributeContext, Contribution,
    DisplayDirective, Effects, IoEvent, KakouneSafeCommand, OrnamentBatch, OverlayContext,
    OverlayContribution, PluginState, RenderOrnamentContext, SlotId, TransformContext,
    TransformTarget, VirtualTextItem,
};

/// Marker trait for handler return types that carry transparency metadata.
///
/// When `IS_TRANSPARENT` is true, the framework records that the handler was
/// registered with a transparent type, enabling compile-time guarantees about
/// the absence of Kakoune writes (ADR-030).
pub trait Transparency {
    /// Whether this type represents a transparent handler return.
    const IS_TRANSPARENT: bool;
}

impl Transparency for Effects {
    const IS_TRANSPARENT: bool = false;
}

impl Transparency for KakouneSafeEffects {
    const IS_TRANSPARENT: bool = true;
}

impl Transparency for Command {
    const IS_TRANSPARENT: bool = false;
}

impl Transparency for KakouneSafeCommand {
    const IS_TRANSPARENT: bool = true;
}

impl Transparency for KeyHandleResult {
    const IS_TRANSPARENT: bool = false;
}

impl Transparency for super::KakouneSafeKeyResult {
    const IS_TRANSPARENT: bool = true;
}

/// Context passed to `on_virtual_edit` handlers when a shadow cursor edit is committed.
#[derive(Debug, Clone)]
pub struct VirtualEditContext {
    /// Buffer line anchoring the editable span (0-indexed).
    pub anchor_line: usize,
    /// Index of the span within the editable virtual text.
    pub span_index: usize,
    /// Original text content at activation time.
    pub original_text: String,
    /// Current edited text content.
    pub working_text: String,
    /// Byte range within the anchor buffer line (for Mirror reference).
    pub buffer_byte_range: std::ops::Range<usize>,
}

/// Downcast state, call handler, box the new state and return `(BoxedState, second.into())`.
macro_rules! register_state_effect {
    ($self:ident, $field:ident, $handler:ident $(, $arg:ident)*) => {
        $self.table.$field = Some(Box::new(move |state, $($arg),*| {
            let s = state.as_any().downcast_ref::<S>().expect("state type mismatch");
            let (new_state, effects) = $handler(s, $($arg),*);
            (Box::new(new_state) as Box<dyn PluginState>, effects.into())
        }));
    };
}

/// Downcast state, call handler, forward the return value directly.
macro_rules! register_view {
    ($self:ident, $field:ident, $handler:ident $(, $arg:ident)*) => {
        $self.table.$field = Some(Box::new(move |state, $($arg),*| {
            let s = state.as_any().downcast_ref::<S>().expect("state type mismatch");
            $handler(s, $($arg),*)
        }));
    };
}

/// Downcast state, call handler, box only the returned state.
macro_rules! register_state_only {
    ($self:ident, $field:ident, $handler:ident $(, $arg:ident)*) => {
        $self.table.$field = Some(Box::new(move |state, $($arg),*| {
            let s = state.as_any().downcast_ref::<S>().expect("state type mismatch");
            Box::new($handler(s, $($arg),*)) as Box<dyn PluginState>
        }));
    };
}

/// Downcast state, call handler (no return value).
macro_rules! register_void {
    ($self:ident, $field:ident, $handler:ident) => {
        $self.table.$field = Some(Box::new(move |state| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            $handler(s);
        }));
    };
}

/// Type-safe handler registration builder.
///
/// `S` is the plugin's concrete state type. Registration methods accept closures
/// over `&S` and automatically infer [`PluginCapabilities`] from which handlers
/// are registered.
pub struct HandlerRegistry<S: PluginState> {
    table: HandlerTable,
    _phantom: PhantomData<S>,
}

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    /// Create a new empty registry.
    pub(crate) fn new() -> Self {
        Self {
            table: HandlerTable::empty(),
            _phantom: PhantomData,
        }
    }

    /// Consume the registry and produce a type-erased [`HandlerTable`].
    pub(crate) fn into_table(self) -> HandlerTable {
        self.table
    }

    // =========================================================================
    // Configuration
    // =========================================================================

    /// Declare which [`DirtyFlags`] this plugin's view methods depend on.
    ///
    /// When no declared flags are dirty and the plugin's state hasn't changed,
    /// the framework can skip re-collecting this plugin's contributions.
    /// Default: `DirtyFlags::ALL` (always re-collect).
    pub fn declare_interests(&mut self, flags: DirtyFlags) {
        self.table.interests = flags;
    }

    /// Suppress a built-in plugin feature.
    ///
    /// When called, the corresponding built-in plugin will skip its default
    /// behavior, allowing this plugin to provide a full replacement.
    pub fn suppress_builtin(&mut self, target: super::BuiltinTarget) {
        self.table.suppressed_builtins.insert(target);
    }

    // =========================================================================
    // Lifecycle handlers
    // =========================================================================

    /// Register an initialization handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    /// Using `KakouneSafeEffects` provides a compile-time guarantee of no
    /// Kakoune writes (ADR-030 Level 5).
    pub fn on_init<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, init_handler, handler, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.init_handler = true;
        }
    }

    /// Register a session-ready handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_session_ready<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, session_ready_handler, handler, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.session_ready_handler = true;
        }
    }

    /// Register a state-changed handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_state_changed<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, DirtyFlags) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, state_changed_handler, handler, app, dirty);
        if E::IS_TRANSPARENT {
            self.table.transparency.state_changed_handler = true;
        }
    }

    /// Register an I/O event handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_io_event<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &IoEvent, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, io_event_handler, handler, event, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.io_event_handler = true;
        }
    }

    /// Register a declarative process task.
    ///
    /// The framework manages job ID allocation, stdout buffering, fallback on
    /// spawn failure, and state machine transitions. The handler receives a
    /// [`ProcessTaskResult`] when the task completes, fails, or (in streaming
    /// mode) produces output.
    ///
    /// The task is started by calling [`start_process_task`] on the plugin bridge.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    ///
    /// ```ignore
    /// r.on_process_task(
    ///     "file_list",
    ///     ProcessTaskSpec::new("fd", &["--type", "f"])
    ///         .fallback(ProcessTaskSpec::new("find", &[".", "-type", "f"])),
    ///     |state, result, _app| match result {
    ///         ProcessTaskResult::Completed { stdout, .. } => { /* ... */ }
    ///         ProcessTaskResult::Failed(msg) => { /* ... */ }
    ///         _ => (state.clone(), Effects::none()),
    ///     },
    /// );
    /// ```
    pub fn on_process_task<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        name: &'static str,
        spec: ProcessTaskSpec,
        handler: impl Fn(&S, &ProcessTaskResult, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.process_tasks.push(ProcessTaskEntry {
            name,
            spec,
            handler: Box::new(move |state, result, app| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let (new_state, effects) = handler(s, result, app);
                (Box::new(new_state) as Box<dyn PluginState>, effects.into())
            }),
            streaming: false,
            transparent: E::IS_TRANSPARENT,
        });
    }

    /// Register a streaming process task.
    ///
    /// Like [`on_process_task`](Self::on_process_task), but delivers stdout
    /// chunks incrementally via [`ProcessTaskResult::Stdout`] instead of
    /// accumulating them until process exit.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_process_task_streaming<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        name: &'static str,
        spec: ProcessTaskSpec,
        handler: impl Fn(&S, &ProcessTaskResult, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.process_tasks.push(ProcessTaskEntry {
            name,
            spec,
            handler: Box::new(move |state, result, app| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let (new_state, effects) = handler(s, result, app);
                (Box::new(new_state) as Box<dyn PluginState>, effects.into())
            }),
            streaming: true,
            transparent: E::IS_TRANSPARENT,
        });
    }

    /// Register a workspace-changed handler.
    pub fn on_workspace_changed(
        &mut self,
        handler: impl Fn(&S, &WorkspaceQuery<'_>) -> S + Send + Sync + 'static,
    ) {
        register_state_only!(self, workspace_changed_handler, handler, query);
    }

    /// Register a workspace save handler.
    ///
    /// Called during workspace layout save. Return `Some(value)` to persist
    /// plugin-specific data alongside the layout. The data will be passed
    /// back to the restore handler when the layout is restored.
    pub fn on_workspace_save(
        &mut self,
        handler: impl Fn(&S) -> Option<serde_json::Value> + Send + Sync + 'static,
    ) {
        self.table.workspace_save_handler = Some(Box::new(move |state| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s)
        }));
    }

    /// Register a workspace restore handler.
    ///
    /// Called during workspace layout restore with data previously returned
    /// by the save handler.
    pub fn on_workspace_restore(
        &mut self,
        handler: impl Fn(&S, &serde_json::Value) -> S + Send + Sync + 'static,
    ) {
        self.table.workspace_restore_handler = Some(Box::new(move |state, data| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, data)) as Box<dyn PluginState>
        }));
    }

    /// Register a shutdown handler.
    pub fn on_shutdown(&mut self, handler: impl Fn(&S) + Send + Sync + 'static) {
        register_void!(self, shutdown_handler, handler);
    }

    /// Register an update (message) handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_update<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &mut dyn Any, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, update_handler, handler, msg, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.update_handler = true;
        }
    }

    // =========================================================================
    // Input handlers
    // =========================================================================

    /// Register a key handler (consumes keys, returns commands).
    ///
    /// Accepts closures returning `Option<(S, Vec<Command>)>` or
    /// `Option<(S, Vec<KakouneSafeCommand>)>` for compile-time transparency.
    pub fn on_key<C: Into<Command> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> Option<(S, Vec<C>)> + Send + Sync + 'static,
    ) {
        self.table.key_handler = Some(Box::new(move |state, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, key, app).map(|(new_state, cmds)| {
                (
                    Box::new(new_state) as Box<dyn PluginState>,
                    cmds.into_iter().map(Into::into).collect(),
                )
            })
        }));
        if C::IS_TRANSPARENT {
            self.table.transparency.key_handler = true;
        }
    }

    /// Register a key middleware handler.
    ///
    /// Accepts closures returning `(S, KeyHandleResult)` or
    /// `(S, KakouneSafeKeyResult)` for compile-time transparency.
    pub fn on_key_middleware<R: Into<KeyHandleResult> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> (S, R) + Send + Sync + 'static,
    ) {
        self.table.key_middleware_handler = Some(Box::new(move |state, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, result) = handler(s, key, app);
            (Box::new(new_state) as Box<dyn PluginState>, result.into())
        }));
        if R::IS_TRANSPARENT {
            self.table.transparency.key_middleware = true;
        }
    }

    /// Register a key observer (notification only, cannot consume).
    pub fn on_observe_key(
        &mut self,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> S + Send + Sync + 'static,
    ) {
        self.table.observe_key_handler = Some(Box::new(move |state, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, key, app)) as Box<dyn PluginState>
        }));
    }

    /// Register a committed text input handler (consumes text, returns commands).
    ///
    /// Accepts closures returning `Option<(S, Vec<Command>)>` or
    /// `Option<(S, Vec<KakouneSafeCommand>)>` for compile-time transparency.
    pub fn on_text_input<C: Into<Command> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &str, &AppView<'_>) -> Option<(S, Vec<C>)> + Send + Sync + 'static,
    ) {
        self.table.text_input_handler = Some(Box::new(move |state, text, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, text, app).map(|(new_state, cmds)| {
                (
                    Box::new(new_state) as Box<dyn PluginState>,
                    cmds.into_iter().map(Into::into).collect(),
                )
            })
        }));
        if C::IS_TRANSPARENT {
            self.table.transparency.text_input = true;
        }
    }

    /// Register a committed text input observer (notification only, cannot consume).
    pub fn on_observe_text_input(
        &mut self,
        handler: impl Fn(&S, &str, &AppView<'_>) -> S + Send + Sync + 'static,
    ) {
        self.table.observe_text_input_handler = Some(Box::new(move |state, text, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, text, app)) as Box<dyn PluginState>
        }));
    }

    /// Register a mouse observer (notification only, cannot consume).
    pub fn on_observe_mouse(
        &mut self,
        handler: impl Fn(&S, &MouseEvent, &AppView<'_>) -> S + Send + Sync + 'static,
    ) {
        self.table.observe_mouse_handler = Some(Box::new(move |state, event, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, event, app)) as Box<dyn PluginState>
        }));
    }

    /// Register a mouse handler (interactive element click).
    ///
    /// Accepts closures returning `Option<(S, Vec<Command>)>` or
    /// `Option<(S, Vec<KakouneSafeCommand>)>` for compile-time transparency.
    pub fn on_handle_mouse<C: Into<Command> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &MouseEvent, InteractiveId, &AppView<'_>) -> Option<(S, Vec<C>)>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.handle_mouse_handler = Some(Box::new(move |state, event, id, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, event, id, app).map(|(new_state, cmds)| {
                (
                    Box::new(new_state) as Box<dyn PluginState>,
                    cmds.into_iter().map(Into::into).collect(),
                )
            })
        }));
        if C::IS_TRANSPARENT {
            self.table.transparency.mouse_handler = true;
        }
    }

    /// Register a drop observer (notification only, cannot consume).
    pub fn on_observe_drop(
        &mut self,
        handler: impl Fn(&S, &DropEvent, &AppView<'_>) -> S + Send + Sync + 'static,
    ) {
        self.table.observe_drop_handler = Some(Box::new(move |state, event, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, event, app)) as Box<dyn PluginState>
        }));
    }

    /// Register a drop handler (interactive element drop target).
    ///
    /// Accepts closures returning `Option<(S, Vec<Command>)>` or
    /// `Option<(S, Vec<KakouneSafeCommand>)>` for compile-time transparency.
    pub fn on_drop<C: Into<Command> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &DropEvent, InteractiveId, &AppView<'_>) -> Option<(S, Vec<C>)>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.handle_drop_handler = Some(Box::new(move |state, event, id, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, event, id, app).map(|(new_state, cmds)| {
                (
                    Box::new(new_state) as Box<dyn PluginState>,
                    cmds.into_iter().map(Into::into).collect(),
                )
            })
        }));
        if C::IS_TRANSPARENT {
            self.table.transparency.drop_handler = true;
        }
    }

    // =========================================================================
    // Transparency query
    // =========================================================================

    /// Returns true if all registered input handlers use their transparent variants.
    ///
    /// When true, the plugin satisfies T10 (Plugin Transparency) by construction
    /// for all input handler extension points. View handlers (contribute, transform,
    /// annotate, overlay, display, render_ornaments) are transparent by construction
    /// since they never return Commands.
    pub fn is_input_transparent(&self) -> bool {
        self.table
            .transparency
            .is_all_input_transparent(&self.table)
    }

    /// Returns true if all registered lifecycle handlers use their transparent variants.
    ///
    /// Lifecycle handlers that produce `Effects` are: init, session_ready,
    /// state_changed, io_event, update, and process tasks.
    pub fn is_lifecycle_transparent(&self) -> bool {
        self.table
            .transparency
            .is_all_lifecycle_transparent(&self.table)
    }

    /// Returns true if ALL registered handlers (input + lifecycle) use transparent variants.
    ///
    /// When true, the plugin satisfies T10 (Plugin Transparency) by construction
    /// for all extension points that can produce `Command` values.
    pub fn is_fully_transparent(&self) -> bool {
        self.table.transparency.is_fully_transparent(&self.table)
    }

    // =========================================================================
    // Other input handlers
    // =========================================================================

    /// Register a default scroll policy handler.
    pub fn on_default_scroll(
        &mut self,
        handler: impl Fn(&S, DefaultScrollCandidate, &AppView<'_>) -> Option<(S, ScrollPolicyResult)>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.default_scroll_handler = Some(Box::new(move |state, candidate, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, candidate, app)
                .map(|(new_state, result)| (Box::new(new_state) as Box<dyn PluginState>, result))
        }));
    }

    /// Register a display scroll offset handler.
    ///
    /// Called during rendering when a non-identity DisplayMap is active.
    /// The handler receives the cursor's display Y coordinate, viewport height,
    /// the default offset computed by the core algorithm, and the current AppView.
    /// Return `Some(offset)` to override, or `None` to defer.
    pub fn on_display_scroll_offset(
        &mut self,
        handler: impl Fn(&S, usize, usize, usize, &AppView<'_>) -> Option<usize> + Send + Sync + 'static,
    ) {
        self.table.display_scroll_offset_handler = Some(Box::new(
            move |state, cursor_y, viewport_h, default_off, app| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, cursor_y, viewport_h, default_off, app)
            },
        ));
    }

    // =========================================================================
    // Renderer extension point handlers
    // =========================================================================

    /// Register a custom menu overlay renderer.
    ///
    /// When registered, this handler is called instead of the built-in menu renderer.
    /// Return `Some(overlay)` to provide the menu overlay, or `None` to defer
    /// to the next plugin or the built-in renderer.
    ///
    /// The overlay-level transform chain is still applied by the pipeline after
    /// this handler returns.
    pub fn on_render_menu_overlay(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Option<Overlay> + Send + Sync + 'static,
    ) {
        self.table.menu_renderer_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app)
        }));
    }

    /// Register a custom info overlay renderer.
    ///
    /// When registered, this handler is called instead of the built-in info renderer.
    /// Return `Some(overlays)` to provide the info overlays, or `None` to defer
    /// to the next plugin or the built-in renderer.
    ///
    /// The overlay-level transform chain is still applied by the pipeline after
    /// this handler returns.
    pub fn on_render_info_overlays(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, &[crate::layout::Rect]) -> Option<Vec<Overlay>>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.info_renderer_handler = Some(Box::new(move |state, app, avoid| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app, avoid)
        }));
    }

    // =========================================================================
    // Key map handlers (Phase 2 — declarative key bindings)
    // =========================================================================

    /// Register a declarative key map with groups, bindings, chords, and actions.
    ///
    /// The builder callback configures the key map structure. Groups are evaluated
    /// in registration order; first matching binding wins.
    ///
    /// ```ignore
    /// r.on_key_map(|km| {
    ///     km.group("active", |s: &MyState| s.active, |g| {
    ///         g.bind(KeyPattern::Exact(KeyEvent::ctrl('p')), "activate");
    ///         g.bind(KeyPattern::AnyCharPlain, "append_char");
    ///     });
    ///     km.chord(KeyEvent::ctrl('w'), |c| {
    ///         c.bind(KeyPattern::Exact(KeyEvent::char_plain('v')), "split_v");
    ///     });
    ///     km.action("activate", |state, _key, _app| {
    ///         let new = MyState { active: true, ..state.clone() };
    ///         (new, KeyResponse::ConsumeRedraw)
    ///     });
    /// });
    /// ```
    pub fn on_key_map(&mut self, builder: impl FnOnce(&mut KeyMapBuilder<S>)) {
        let mut km = KeyMapBuilder::<S>::new();
        builder(&mut km);

        // Build the initial compiled key map.
        let initial_map = km.build_compiled_map();
        self.table.key_map = Some(initial_map);

        // Store group refresh handler: evaluates `when()` predicates against state.
        let group_predicates = km.group_predicates;
        self.table.group_refresh_handler = Some(Box::new(
            move |state: &dyn PluginState, _app: &AppView<'_>, map: &mut CompiledKeyMap| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                for (i, predicate) in group_predicates.iter().enumerate() {
                    if let Some(group) = map.groups.get_mut(i) {
                        group.active = predicate(s);
                    }
                }
            },
        ));

        // Store action handler.
        let actions = km.actions;
        self.table.action_handler = Some(Box::new(
            move |state: &dyn PluginState,
                  action_id: &str,
                  key: &KeyEvent,
                  app: &AppView<'_>|
                  -> (Box<dyn PluginState>, KeyResponse) {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                for (id, handler) in &actions {
                    if *id == action_id {
                        let (new_state, response) = handler(s, key, app);
                        return (Box::new(new_state) as Box<dyn PluginState>, response);
                    }
                }
                (
                    Box::new(s.clone()) as Box<dyn PluginState>,
                    KeyResponse::Pass,
                )
            },
        ));
    }

    // =========================================================================
    // View handlers
    // =========================================================================

    /// Register a contribution handler for a specific slot.
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
        self.table.recovery.display = super::handler_table::DisplayRecoveryStatus::Unwitnessed;
    }

    /// Display handler that cannot emit Hide directives (compile-time safe).
    ///
    /// The handler returns `Vec<SafeDisplayDirective>`, which has no `Hide`
    /// constructor, making non-destructiveness a compile-time property.
    pub fn on_display_safe(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<super::SafeDisplayDirective> + Send + Sync + 'static,
    ) {
        self.table.display_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app).into_iter().map(Into::into).collect()
        }));
        self.table.recovery.display = super::handler_table::DisplayRecoveryStatus::NonDestructive;
    }

    /// Display handler that may emit Hide, with recovery evidence.
    ///
    /// The caller provides a [`RecoveryWitness`](super::RecoveryWitness)
    /// documenting how the user can recover hidden content, satisfying
    /// Visual Faithfulness (§10.2a).
    pub fn on_display_witnessed(
        &mut self,
        witness: super::RecoveryWitness,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<DisplayDirective> + Send + Sync + 'static,
    ) {
        register_view!(self, display_handler, handler, app);
        self.table.recovery.display =
            super::handler_table::DisplayRecoveryStatus::Witnessed(witness);
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
        self.table.recovery.display = super::handler_table::DisplayRecoveryStatus::Unwitnessed;
    }

    /// Unified display handler that cannot emit destructive directives (compile-time safe).
    pub fn on_display_unified_safe(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<super::SafeDisplayDirective> + Send + Sync + 'static,
    ) {
        self.table.unified_display_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app).into_iter().map(Into::into).collect()
        }));
        self.table.recovery.display = super::handler_table::DisplayRecoveryStatus::NonDestructive;
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
        use super::handler_table::{DisplayRecoveryStatus, ProjectionEntry};
        use crate::display::ProjectionCategory;

        let recovery = match descriptor.category {
            ProjectionCategory::Structural => {
                DisplayRecoveryStatus::Witnessed(super::RecoveryWitness {
                    mechanism: super::RecoveryMechanism::Declared {
                        description: "projection mode switch",
                    },
                })
            }
            ProjectionCategory::Additive => DisplayRecoveryStatus::NonDestructive,
        };

        let erased: super::handler_table::ErasedDisplayHandler = Box::new(move |state, app| {
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
        handler: impl Fn(&S, &AppView<'_>) -> Vec<super::SafeDisplayDirective> + Send + Sync + 'static,
    ) {
        use super::handler_table::{DisplayRecoveryStatus, ProjectionEntry};
        use crate::display::ProjectionCategory;

        assert!(
            descriptor.category == ProjectionCategory::Additive,
            "define_additive_projection requires Additive category, got {:?}",
            descriptor.category,
        );

        let erased: super::handler_table::ErasedDisplayHandler = Box::new(move |state, app| {
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

    /// Register a menu item transform handler.
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

    // =========================================================================
    // Navigation handlers (DU-4)
    // =========================================================================

    /// Register a navigation policy handler for display units.
    ///
    /// The handler returns a `NavigationPolicy` for a given display unit,
    /// allowing plugins to override the default navigation behavior.
    /// FirstWins composition: the first plugin returning a policy wins.
    pub fn on_navigation_policy(
        &mut self,
        handler: impl Fn(&S, &DisplayUnit) -> NavigationPolicy + Send + Sync + 'static,
    ) {
        register_view!(self, navigation_policy_handler, handler, unit);
    }

    /// Register a navigation action handler for display units.
    ///
    /// Called when a `Boundary` unit is activated (click or keyboard).
    /// Returns `(new_state, ActionResult)` following the functional-update model.
    /// FirstWins composition: the first non-Pass result wins.
    pub fn on_navigation_action(
        &mut self,
        handler: impl Fn(&S, &DisplayUnit, NavigationAction) -> (S, ActionResult)
        + Send
        + Sync
        + 'static,
    ) {
        register_state_effect!(self, navigation_action_handler, handler, unit, action);
    }

    // =========================================================================
    // Virtual edit handlers (BDT)
    // =========================================================================

    /// Register a virtual edit handler for editable virtual text.
    ///
    /// Called when a shadow cursor edit is committed on a `PluginDefined`
    /// projection. The handler receives the edit context and returns
    /// `(new_state, Vec<Command>)` with the commands to apply.
    pub fn on_virtual_edit(
        &mut self,
        handler: impl Fn(&S, &VirtualEditContext, &AppView<'_>) -> (S, Vec<Command>)
        + Send
        + Sync
        + 'static,
    ) {
        register_state_effect!(self, virtual_edit_handler, handler, ctx, app);
    }

    // =========================================================================
    // Inline-box paint handler (ADR-031 Phase 10 Step 2-native)
    // =========================================================================

    /// Register an inline-box paint handler.
    ///
    /// Called by the renderer when a `DisplayDirective::InlineBox` slot
    /// owned by this plugin needs paint content. The handler receives the
    /// `box_id` declared in the directive and returns either an Element
    /// to paint inside the slot, or `None` to leave the slot empty (the
    /// renderer falls back to the placeholder reservation).
    ///
    /// Auto-registers `PluginCapabilities::INLINE_BOX_PAINTER`. Step 2-host
    /// will wire the renderer to invoke this; until then registration is
    /// inert.
    pub fn on_paint_inline_box(
        &mut self,
        handler: impl Fn(&S, u64, &AppView<'_>) -> Option<Element> + Send + Sync + 'static,
    ) {
        register_view!(self, inline_box_paint_handler, handler, box_id, app);
    }

    // =========================================================================
    // Pub/Sub handlers
    // =========================================================================

    /// Publish a typed value on a topic each frame.
    ///
    /// The handler is called during the publication collection phase. Its return
    /// value is delivered to all subscribers of the same topic.
    ///
    /// ```ignore
    /// r.publish::<u32>(TopicId::new("cursor.line"), |state, _app| state.cursor_line);
    /// ```
    pub fn publish<T: Serialize + Send + 'static>(
        &mut self,
        topic: TopicId,
        handler: impl Fn(&S, &AppView<'_>) -> T + Send + Sync + 'static,
    ) {
        self.table.publishers.push(PublishEntry {
            topic,
            handler: Box::new(move |state: &dyn PluginState, app: &AppView<'_>| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let value = handler(s, app);
                ChannelValue::new(&value).expect("publish serialization failed")
            }),
        });
    }

    /// Subscribe to a typed topic published by another plugin.
    ///
    /// The handler receives each published value and returns the updated state.
    /// Called during the delivery phase, after all publications are collected.
    ///
    /// ```ignore
    /// r.subscribe::<u32>(TopicId::new("cursor.line"), |state, value| {
    ///     MyState { tracked_line: *value, ..state.clone() }
    /// });
    /// ```
    pub fn subscribe<T: DeserializeOwned + 'static>(
        &mut self,
        topic: TopicId,
        handler: impl Fn(&S, &T) -> S + Send + Sync + 'static,
    ) {
        self.table.subscribers.push(SubscribeEntry {
            topic,
            handler: Box::new(
                move |state: &dyn PluginState, value: &ChannelValue| -> Box<dyn PluginState> {
                    let s = state
                        .as_any()
                        .downcast_ref::<S>()
                        .expect("state type mismatch");
                    let v: T = value
                        .deserialize()
                        .expect("subscribe deserialization failed");
                    Box::new(handler(s, &v))
                },
            ),
        });
    }

    /// Publish a typed value on a topic, returning a [`Topic<T>`] handle.
    ///
    /// The returned handle carries the type parameter `T` at compile time,
    /// ensuring that [`subscribe_typed`](Self::subscribe_typed) callers use
    /// the correct type. Untyped [`publish`](Self::publish) / [`subscribe`](Self::subscribe)
    /// remain for WASM cross-boundary interop.
    ///
    /// ```ignore
    /// let topic: Topic<u32> = r.publish_typed("cursor.line", |s, _| s.line);
    /// r.subscribe_typed(&topic, |state, value: &u32| { ... });
    /// ```
    pub fn publish_typed<T: Serialize + Send + 'static>(
        &mut self,
        name: impl Into<compact_str::CompactString>,
        handler: impl Fn(&S, &AppView<'_>) -> T + Send + Sync + 'static,
    ) -> Topic<T> {
        let topic = Topic::<T>::new(name);
        self.table.publishers.push(PublishEntry {
            topic: topic.id().clone(),
            handler: Box::new(move |state: &dyn PluginState, app: &AppView<'_>| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let value = handler(s, app);
                ChannelValue::new(&value).expect("publish serialization failed")
            }),
        });
        topic
    }

    /// Subscribe to a [`Topic<T>`] handle with compile-time type safety.
    ///
    /// The `T` parameter is enforced by the `Topic<T>` handle, so no
    /// turbofish is needed and type mismatches are caught at compile time.
    pub fn subscribe_typed<T: DeserializeOwned + 'static>(
        &mut self,
        topic: &Topic<T>,
        handler: impl Fn(&S, &T) -> S + Send + Sync + 'static,
    ) {
        self.subscribe::<T>(topic.id().clone(), handler);
    }

    // =========================================================================
    // Extension Point handlers
    // =========================================================================

    /// Define a custom extension point that other plugins can contribute to.
    ///
    /// The `rule` determines how multiple contributions are composed.
    ///
    /// ```ignore
    /// r.define_extension::<(), Vec<StatusItem>>(
    ///     ExtensionPointId::new("myplugin.status-items"),
    ///     CompositionRule::Merge,
    /// );
    /// ```
    pub fn define_extension<I: Send + 'static, O: Send + 'static>(
        &mut self,
        id: ExtensionPointId,
        rule: CompositionRule,
    ) {
        self.table.extension_definitions.push(ExtensionDefinition {
            id,
            rule,
            handler: None,
        });
    }

    /// Define a custom extension point and also contribute a handler for it.
    pub fn define_extension_with_handler<
        I: DeserializeOwned + Send + 'static,
        O: Serialize + Send + 'static,
    >(
        &mut self,
        id: ExtensionPointId,
        rule: CompositionRule,
        handler: impl Fn(&S, &I, &AppView<'_>) -> O + Send + Sync + 'static,
    ) {
        self.table.extension_definitions.push(ExtensionDefinition {
            id,
            rule,
            handler: Some(Box::new(
                move |state: &dyn PluginState,
                      input: &ChannelValue,
                      app: &AppView<'_>|
                      -> ChannelValue {
                    let s = state
                        .as_any()
                        .downcast_ref::<S>()
                        .expect("state type mismatch");
                    let i: I = input
                        .deserialize()
                        .expect("extension input deserialization failed");
                    let output = handler(s, &i, app);
                    ChannelValue::new(&output).expect("extension output serialization failed")
                },
            )),
        });
    }

    /// Contribute to an extension point defined by another plugin.
    ///
    /// ```ignore
    /// r.on_extension::<(), Vec<StatusItem>>(
    ///     ExtensionPointId::new("myplugin.status-items"),
    ///     |_state, _input, _app| vec![StatusItem { text: "hello" }],
    /// );
    /// ```
    pub fn on_extension<I: DeserializeOwned + Send + 'static, O: Serialize + Send + 'static>(
        &mut self,
        id: ExtensionPointId,
        handler: impl Fn(&S, &I, &AppView<'_>) -> O + Send + Sync + 'static,
    ) {
        self.table
            .extension_contributions
            .push(ExtensionContribution {
                id,
                handler: Box::new(
                    move |state: &dyn PluginState,
                          input: &ChannelValue,
                          app: &AppView<'_>|
                          -> ChannelValue {
                        let s = state
                            .as_any()
                            .downcast_ref::<S>()
                            .expect("state type mismatch");
                        let i: I = input
                            .deserialize()
                            .expect("extension input deserialization failed");
                        let output = handler(s, &i, app);
                        ChannelValue::new(&output).expect("extension output serialization failed")
                    },
                ),
            });
    }
}

// =============================================================================
// KeyMapBuilder — fluent API for declaring key maps
// =============================================================================

type GroupPredicate<S> = Box<dyn Fn(&S) -> bool + Send + Sync>;
type ActionHandler<S> = Box<dyn Fn(&S, &KeyEvent, &AppView<'_>) -> (S, KeyResponse) + Send + Sync>;

/// Builder for constructing a [`CompiledKeyMap`] with type-safe state access.
pub struct KeyMapBuilder<S: PluginState> {
    groups: Vec<KeyGroupDef<S>>,
    chord_groups: Vec<ChordGroupDef>,
    pub(crate) group_predicates: Vec<GroupPredicate<S>>,
    pub(crate) actions: Vec<(&'static str, ActionHandler<S>)>,
}

struct KeyGroupDef<S> {
    name: &'static str,
    predicate: GroupPredicate<S>,
    bindings: Vec<KeyBinding>,
    chords: Vec<ChordBinding>,
}

struct ChordGroupDef {
    bindings: Vec<ChordBinding>,
}

impl<S: PluginState + Clone + 'static> KeyMapBuilder<S> {
    fn new() -> Self {
        Self {
            groups: Vec::new(),
            chord_groups: Vec::new(),
            group_predicates: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// Define a key group that is active when the predicate returns true.
    ///
    /// Groups are evaluated in declaration order — first matching binding wins.
    pub fn group(
        &mut self,
        name: &'static str,
        when: impl Fn(&S) -> bool + Send + Sync + 'static,
        build: impl FnOnce(&mut KeyGroupConfig),
    ) {
        let mut cfg = KeyGroupConfig {
            bindings: Vec::new(),
            chords: Vec::new(),
        };
        build(&mut cfg);
        self.groups.push(KeyGroupDef {
            name,
            predicate: Box::new(when),
            bindings: cfg.bindings,
            chords: cfg.chords,
        });
    }

    /// Define chord bindings under a leader key.
    ///
    /// The chord group is always active (create it inside a `group()` for
    /// conditional activation).
    pub fn chord(&mut self, leader: KeyEvent, build: impl FnOnce(&mut ChordConfig)) {
        let mut cfg = ChordConfig {
            leader: leader.clone(),
            bindings: Vec::new(),
        };
        build(&mut cfg);
        self.chord_groups.push(ChordGroupDef {
            bindings: cfg.bindings,
        });
    }

    /// Register an action handler by ID.
    ///
    /// Action handlers receive the current state and the triggering key event,
    /// and return the updated state plus a [`KeyResponse`].
    pub fn action(
        &mut self,
        id: &'static str,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> (S, KeyResponse) + Send + Sync + 'static,
    ) {
        self.actions.push((id, Box::new(handler)));
    }

    /// Build the initial [`CompiledKeyMap`] from the declared groups.
    fn build_compiled_map(&mut self) -> CompiledKeyMap {
        let mut groups = Vec::new();

        for def in &self.groups {
            let active = true; // will be refreshed on first frame
            groups.push(KeyGroup {
                name: def.name,
                active,
                bindings: def.bindings.clone(),
                chords: def.chords.clone(),
            });
        }

        // Merge standalone chord groups into their own always-active group.
        for chord_def in &self.chord_groups {
            groups.push(KeyGroup {
                name: "__chord__",
                active: true,
                bindings: Vec::new(),
                chords: chord_def.bindings.clone(),
            });
        }

        // Move predicates out for the refresh handler.
        self.group_predicates = self
            .groups
            .iter_mut()
            .map(|def| {
                // Replace with a dummy predicate; the real one is captured by the closure.
                std::mem::replace(&mut def.predicate, Box::new(|_| true))
            })
            .collect();
        // Always-active chord groups get constant `true` predicates.
        for _ in &self.chord_groups {
            self.group_predicates.push(Box::new(|_| true));
        }

        CompiledKeyMap {
            groups,
            ..Default::default()
        }
    }
}

/// Configuration for bindings within a key group.
pub struct KeyGroupConfig {
    bindings: Vec<KeyBinding>,
    chords: Vec<ChordBinding>,
}

impl KeyGroupConfig {
    /// Add a single-key binding.
    pub fn bind(&mut self, pattern: KeyPattern, action_id: &'static str) {
        self.bindings.push(KeyBinding { pattern, action_id });
    }

    /// Add a chord binding within this group.
    pub fn chord_bind(&mut self, leader: KeyEvent, follower: KeyPattern, action_id: &'static str) {
        self.chords.push(ChordBinding {
            leader,
            follower,
            action_id,
        });
    }
}

/// Configuration for chord bindings under a leader key.
pub struct ChordConfig {
    leader: KeyEvent,
    bindings: Vec<ChordBinding>,
}

impl ChordConfig {
    /// Add a follower binding under this chord's leader.
    pub fn bind(&mut self, follower: KeyPattern, action_id: &'static str) {
        self.bindings.push(ChordBinding {
            leader: self.leader.clone(),
            follower,
            action_id,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginCapabilities;
    use crate::plugin::kakoune_safe_command::KakouneSafeCommand;
    use crate::plugin::traits::PluginBackend;
    use crate::state::DirtyFlags;

    #[derive(Clone, Debug, PartialEq, Hash, Default)]
    struct TestState {
        counter: u32,
    }

    #[test]
    fn empty_registry_has_no_capabilities() {
        let registry = HandlerRegistry::<TestState>::new();
        let table = registry.into_table();
        assert_eq!(table.capabilities(), PluginCapabilities::empty());
    }

    #[test]
    fn declare_interests() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.declare_interests(DirtyFlags::BUFFER);
        let table = registry.into_table();
        assert_eq!(table.interests(), DirtyFlags::BUFFER);
    }

    #[test]
    fn on_decorate_background_sets_annotator_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_background(|_state, _line, _app, _ctx| None);
        let table = registry.into_table();
        assert!(table.capabilities().contains(PluginCapabilities::ANNOTATOR));
    }

    #[test]
    fn on_contribute_sets_contributor_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_contribute(SlotId::STATUS_LEFT, |_state, _app, _ctx| None);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::CONTRIBUTOR)
        );
        assert_eq!(table.contribute_handlers.len(), 1);
        assert_eq!(table.contribute_handlers[0].slot, SlotId::STATUS_LEFT);
    }

    #[test]
    fn on_transform_sets_transformer_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_transform(10, |_state, _target, _app, _ctx| ElementPatch::Identity);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::TRANSFORMER)
        );
        assert!(table.transform_handler.is_some());
        assert_eq!(table.transform_handler.as_ref().unwrap().priority, 10);
    }

    #[test]
    fn on_transform_has_empty_targets() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_transform(10, |_state, _target, _app, _ctx| ElementPatch::Identity);
        let table = registry.into_table();
        let desc = table.capability_descriptor();
        assert!(desc.transform_targets.is_empty());
    }

    #[test]
    fn on_transform_for_populates_targets() {
        use crate::plugin::context::TransformTarget;
        let mut registry = HandlerRegistry::<TestState>::new();
        let targets = [TransformTarget::BUFFER, TransformTarget::STATUS_BAR];
        registry.on_transform_for(5, &targets, |_state, _target, _app, _ctx| {
            ElementPatch::Identity
        });
        let table = registry.into_table();
        let desc = table.capability_descriptor();
        assert_eq!(desc.transform_targets.len(), 2);
        assert!(desc.transform_targets.contains(&TransformTarget::BUFFER));
        assert!(
            desc.transform_targets
                .contains(&TransformTarget::STATUS_BAR)
        );
    }

    #[test]
    fn on_transform_for_sets_priority() {
        use crate::plugin::context::TransformTarget;
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_transform_for(
            42,
            &[TransformTarget::MENU],
            |_state, _target, _app, _ctx| ElementPatch::Identity,
        );
        let table = registry.into_table();
        assert_eq!(table.transform_handler.as_ref().unwrap().priority, 42);
    }

    #[test]
    fn may_interfere_detects_transform_target_overlap() {
        use crate::plugin::context::TransformTarget;

        let mut r1 = HandlerRegistry::<TestState>::new();
        r1.on_transform_for(
            0,
            &[TransformTarget::BUFFER, TransformTarget::MENU],
            |_s, _t, _a, _c| ElementPatch::Identity,
        );
        let desc1 = r1.into_table().capability_descriptor();

        let mut r2 = HandlerRegistry::<TestState>::new();
        r2.on_transform_for(
            0,
            &[TransformTarget::MENU, TransformTarget::STATUS_BAR],
            |_s, _t, _a, _c| ElementPatch::Identity,
        );
        let desc2 = r2.into_table().capability_descriptor();

        // MENU overlaps
        assert!(desc1.may_interfere(&desc2));
    }

    #[test]
    fn may_interfere_no_overlap() {
        use crate::plugin::context::TransformTarget;

        let mut r1 = HandlerRegistry::<TestState>::new();
        r1.on_transform_for(0, &[TransformTarget::BUFFER], |_s, _t, _a, _c| {
            ElementPatch::Identity
        });
        let desc1 = r1.into_table().capability_descriptor();

        let mut r2 = HandlerRegistry::<TestState>::new();
        r2.on_transform_for(0, &[TransformTarget::MENU], |_s, _t, _a, _c| {
            ElementPatch::Identity
        });
        let desc2 = r2.into_table().capability_descriptor();

        assert!(!desc1.may_interfere(&desc2));
    }

    #[test]
    fn on_key_sets_input_handler_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(|_state, _key, _app| None::<(TestState, Vec<Command>)>);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::INPUT_HANDLER)
        );
    }

    #[test]
    fn on_text_input_sets_input_handler_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_text_input(|_state, _text, _app| None::<(TestState, Vec<Command>)>);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::INPUT_HANDLER)
        );
    }

    #[test]
    fn on_overlay_sets_overlay_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_overlay(|_state, _app, _ctx| None);
        let table = registry.into_table();
        assert!(table.capabilities().contains(PluginCapabilities::OVERLAY));
    }

    #[test]
    fn on_display_sets_display_transform_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display(|_state, _app| vec![]);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::DISPLAY_TRANSFORM)
        );
    }

    #[test]
    fn on_render_ornaments_sets_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_render_ornaments(|_state, _app, _ctx| OrnamentBatch::default());
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::RENDER_ORNAMENT)
        );
    }

    #[test]
    fn on_paint_inline_box_sets_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_paint_inline_box(|_state, _box_id, _app| None);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::INLINE_BOX_PAINTER)
        );
    }

    #[test]
    fn paint_inline_box_default_is_no_op() {
        // A registry with no inline-box-paint handler must not advertise
        // the capability (gating invariant — host can skip dispatch).
        let registry = HandlerRegistry::<TestState>::new();
        let table = registry.into_table();
        assert!(
            !table
                .capabilities()
                .contains(PluginCapabilities::INLINE_BOX_PAINTER)
        );
    }

    #[test]
    fn multiple_gutter_handlers() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_gutter(GutterSide::Left, 0, |_s, _l, _a, _c| None);
        registry.on_decorate_gutter(GutterSide::Right, 10, |_s, _l, _a, _c| None);
        let table = registry.into_table();
        assert_eq!(table.gutter_handlers.len(), 2);
        assert_eq!(table.gutter_handlers[0].side, GutterSide::Left);
        assert_eq!(table.gutter_handlers[0].priority, 0);
        assert_eq!(table.gutter_handlers[1].side, GutterSide::Right);
        assert_eq!(table.gutter_handlers[1].priority, 10);
    }

    #[test]
    fn multiple_contribute_handlers() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_contribute(SlotId::STATUS_LEFT, |_s, _a, _c| None);
        registry.on_contribute(SlotId::STATUS_RIGHT, |_s, _a, _c| None);
        let table = registry.into_table();
        assert_eq!(table.contribute_handlers.len(), 2);
    }

    #[test]
    fn combined_capabilities() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_background(|_s, _l, _a, _c| None);
        registry.on_overlay(|_s, _a, _c| None);
        registry.on_key(|_s, _k, _a| None::<(TestState, Vec<Command>)>);
        let table = registry.into_table();
        let caps = table.capabilities();
        assert!(caps.contains(PluginCapabilities::ANNOTATOR));
        assert!(caps.contains(PluginCapabilities::OVERLAY));
        assert!(caps.contains(PluginCapabilities::INPUT_HANDLER));
        assert!(!caps.contains(PluginCapabilities::TRANSFORMER));
    }

    #[test]
    fn has_annotation_handlers_with_background() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_background(|_s, _l, _a, _c| None);
        let table = registry.into_table();
        assert!(table.has_annotation_handlers());
    }

    #[test]
    fn has_annotation_handlers_with_gutter() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_gutter(GutterSide::Left, 0, |_s, _l, _a, _c| None);
        let table = registry.into_table();
        assert!(table.has_annotation_handlers());
    }

    #[test]
    fn handler_type_erasure_invocation() {
        // Verify that erased handlers can be invoked with the correct state type.
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_state_changed(|state, _app, _dirty| {
            let new_state = TestState {
                counter: state.counter + 1,
            };
            (new_state, Effects::default())
        });
        let table = registry.into_table();

        // Create a boxed state
        let _state: Box<dyn PluginState> = Box::new(TestState { counter: 5 });

        // We can't easily create an AppView in tests, but we can verify
        // the handler is stored and the type alias is correct.
        assert!(table.state_changed_handler.is_some());
    }

    #[test]
    fn on_navigation_policy_sets_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_navigation_policy(|_state, _unit| NavigationPolicy::Normal);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::NAVIGATION_POLICY)
        );
    }

    #[test]
    fn on_navigation_action_sets_capability_and_updates_state() {
        use crate::display;
        use crate::plugin::PluginBridge;
        use crate::plugin::state::Plugin;

        #[derive(Clone, Debug, PartialEq, Hash, Default)]
        struct NavTestState {
            counter: u32,
        }
        struct NavTestPlugin;
        impl Plugin for NavTestPlugin {
            type State = NavTestState;
            fn id(&self) -> crate::plugin::PluginId {
                crate::plugin::PluginId("nav-test".into())
            }
            fn register(&self, r: &mut HandlerRegistry<NavTestState>) {
                r.on_navigation_action(|state, _unit, _action| {
                    (
                        NavTestState {
                            counter: state.counter + 1,
                        },
                        ActionResult::Handled,
                    )
                });
            }
        }

        let mut bridge = PluginBridge::new(NavTestPlugin);
        assert!(
            bridge
                .capabilities()
                .contains(PluginCapabilities::NAVIGATION_ACTION)
        );

        let unit = display::unit::DisplayUnit {
            id: display::unit::DisplayUnitId::from_content(
                &display::unit::UnitSource::Line(0),
                &display::unit::SemanticRole::BufferContent,
            ),
            display_line: 0,
            role: display::unit::SemanticRole::BufferContent,
            source: display::unit::UnitSource::Line(0),
            interaction: display::InteractionPolicy::Normal,
        };
        let result = bridge.navigation_action(&unit, NavigationAction::None);
        assert_eq!(result, Some(ActionResult::Handled));
    }

    // =========================================================================
    // Transparent handler registration (ADR-030 Level 3)
    // =========================================================================

    #[test]
    fn on_key_transparent_sets_input_handler_and_transparency() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(
            |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneSafeCommand>)> {
                None
            },
        );
        assert!(registry.is_input_transparent());
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::INPUT_HANDLER)
        );
        assert!(table.transparency.key_handler);
    }

    #[test]
    fn on_key_non_transparent_means_not_input_transparent() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(|_state, _key, _app| None::<(TestState, Vec<Command>)>);
        assert!(!registry.is_input_transparent());
    }

    #[test]
    fn mixed_transparent_and_non_transparent_is_not_transparent() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(
            |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneSafeCommand>)> {
                None
            },
        );
        registry.on_text_input(|_state, _text, _app| None::<(TestState, Vec<Command>)>);
        assert!(!registry.is_input_transparent());
    }

    #[test]
    fn all_transparent_handlers_means_input_transparent() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(
            |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneSafeCommand>)> {
                None
            },
        );
        registry.on_text_input(
            |_state: &TestState, _text, _app| -> Option<(TestState, Vec<KakouneSafeCommand>)> {
                None
            },
        );
        assert!(registry.is_input_transparent());
    }

    #[test]
    fn no_handlers_is_input_transparent() {
        let registry = HandlerRegistry::<TestState>::new();
        assert!(registry.is_input_transparent());
    }

    // =========================================================================
    // Unified display handler tests (Phase 1B.2)
    // =========================================================================

    #[test]
    fn on_display_unified_sets_display_transform_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified(|_state, _app| vec![]);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::DISPLAY_TRANSFORM)
        );
    }

    #[test]
    fn on_display_unified_sets_annotator_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified(|_state, _app| vec![]);
        let table = registry.into_table();
        assert!(table.capabilities().contains(PluginCapabilities::ANNOTATOR));
    }

    #[test]
    fn on_display_unified_sets_content_annotator_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified(|_state, _app| vec![]);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::CONTENT_ANNOTATOR)
        );
    }

    #[test]
    fn on_display_unified_safe_is_recoverable() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified_safe(|_state, _app| vec![]);
        assert!(registry.is_display_recoverable());
    }

    #[test]
    fn on_display_unified_is_not_recoverable() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified(|_state, _app| vec![]);
        assert!(!registry.is_display_recoverable());
    }
}
