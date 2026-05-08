//! Input handlers: key, mouse, text input, drop, default scroll.

use crate::element::InteractiveId;
use crate::input::{DropEvent, KeyEvent, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};

use super::super::traits::{
    KeyHandleResult, KeyPreDispatchResult, MousePreDispatchResult, TextInputPreDispatchResult,
};
use super::super::{AppView, Command, PluginState};

use super::{HandlerRegistry, Transparency};

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
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

    /// Register a key pre-dispatch handler — runs before observers and middleware.
    ///
    /// Pre-dispatch handlers can `Consume` the key (terminating dispatch) or
    /// `Pass` it through with optional state updates and commands. Used by
    /// shadow-cursor-class plugins that need first-look at every keystroke.
    pub fn on_key_pre_dispatch(
        &mut self,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> (S, KeyPreDispatchResult)
        + Send
        + Sync
        + 'static,
    ) {
        self.table.key_pre_dispatch_handler = Some(Box::new(move |state, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, result) = handler(s, key, app);
            (Box::new(new_state) as Box<dyn PluginState>, result)
        }));
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

    /// Register a text-input pre-dispatch handler — runs before the input handler chain.
    ///
    /// Pre-dispatch handlers can `Consume` the input (terminating dispatch) or
    /// `Pass` it through with optional state updates and commands.
    pub fn on_text_input_pre_dispatch(
        &mut self,
        handler: impl Fn(&S, &str, &AppView<'_>) -> (S, TextInputPreDispatchResult)
        + Send
        + Sync
        + 'static,
    ) {
        self.table.text_input_pre_dispatch_handler = Some(Box::new(move |state, text, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, result) = handler(s, text, app);
            (Box::new(new_state) as Box<dyn PluginState>, result)
        }));
    }

    /// Register a mouse pre-dispatch handler — runs before observers and hit-test dispatch.
    ///
    /// Pre-dispatch handlers can `Consume` the event (terminating dispatch) or
    /// `Pass` it through with optional state updates and commands. Used by
    /// drag-tracking and shadow-cursor-class plugins.
    pub fn on_mouse_pre_dispatch(
        &mut self,
        handler: impl Fn(&S, &MouseEvent, &AppView<'_>) -> (S, MousePreDispatchResult)
        + Send
        + Sync
        + 'static,
    ) {
        self.table.mouse_pre_dispatch_handler = Some(Box::new(move |state, event, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, result) = handler(s, event, app);
            (Box::new(new_state) as Box<dyn PluginState>, result)
        }));
    }

    /// Register a mouse fallback handler — invoked when no plugin consumes a mouse event.
    ///
    /// Used by `BuiltinMouseFallbackPlugin` to forward unhandled mouse events
    /// to Kakoune. User plugins can override default mouse-to-Kakoune behaviour
    /// by registering a higher-priority `on_mouse_fallback`.
    pub fn on_mouse_fallback(
        &mut self,
        handler: impl Fn(&S, &MouseEvent, i32, &AppView<'_>) -> (S, Option<Vec<Command>>)
        + Send
        + Sync
        + 'static,
    ) {
        self.table.mouse_fallback_handler = Some(Box::new(move |state, event, scroll, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, commands) = handler(s, event, scroll, app);
            (Box::new(new_state) as Box<dyn PluginState>, commands)
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
}
