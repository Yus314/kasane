//! Input handlers: key, mouse, text input, drop, default scroll.

use crate::element::InteractiveId;
use crate::input::{DropEvent, KeyEvent, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};

use super::super::traits::{
    KakouneSideKeyPreDispatchResult, KakouneSideMousePreDispatchResult,
    KakouneSideTextInputPreDispatchResult, KeyHandleResult, KeyPreDispatchResult,
    MousePreDispatchResult, TextInputPreDispatchResult,
};
use super::super::{AppView, Command, KakouneSideCommand, PluginState};

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

    /// Register a tier-1 key handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Plugin authors opt into a **no-spawn** contract for key handlers by
    /// returning `Vec<KakouneSideCommand>`. The bound is stricter than
    /// ADR-044's default mapping for input handlers (Tier 2) — it is an
    /// opt-in tightening for handlers that only need Kakoune-side effects.
    ///
    /// The bound rejects `Command` returns at compile time because there is
    /// intentionally no `From<Command> for KakouneSideCommand` impl (a
    /// generic `Command` may be a `SpawnProcess` variant).
    pub fn on_key_tier1<C: Into<KakouneSideCommand> + 'static>(
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
                    cmds.into_iter()
                        .map(|c| {
                            let side: KakouneSideCommand = c.into();
                            side.into()
                        })
                        .collect(),
                )
            })
        }));
    }

    /// Register a key middleware handler.
    ///
    /// Accepts closures returning `(S, KeyHandleResult)` or
    /// `(S, KakouneTransparentKeyResult)` for compile-time transparency.
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
    ///
    /// **For new code, prefer [`Self::on_key_pre_dispatch_tier1`]** — it pins
    /// the Tier-1 contract from
    /// [ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)
    /// at compile time. Key pre-dispatch fires per keystroke and is the
    /// same re-entrance class as state-changed; process spawn from
    /// pre-dispatch is an anti-pattern in practice.
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

    /// Register a tier-1 key pre-dispatch handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Returns a [`KakouneSideKeyPreDispatchResult`] whose `commands` is
    /// `Vec<KakouneSideCommand>`. `pending_buffer_edit` passes through
    /// unchanged — the algebraic shadow-cursor commit path is
    /// orthogonal to the tier hierarchy (the dispatch loop later
    /// serializes the resolved edit into Kakoune-side commands).
    pub fn on_key_pre_dispatch_tier1<R: Into<KakouneSideKeyPreDispatchResult> + 'static>(
        &mut self,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> (S, R) + Send + Sync + 'static,
    ) {
        self.table.key_pre_dispatch_handler = Some(Box::new(move |state, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, tier1) = handler(s, key, app);
            let tier1: KakouneSideKeyPreDispatchResult = tier1.into();
            (Box::new(new_state) as Box<dyn PluginState>, tier1.into())
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
    /// `Option<(S, Vec<KakouneTransparentCommand>)>` for compile-time transparency.
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

    /// Register a tier-1 committed text input handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// `Vec<KakouneSideCommand>` opt-in for input handlers that only need
    /// Kakoune-side effects. Stricter than ADR-044's Tier 2 default for
    /// input handlers.
    pub fn on_text_input_tier1<C: Into<KakouneSideCommand> + 'static>(
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
                    cmds.into_iter()
                        .map(|c| {
                            let side: KakouneSideCommand = c.into();
                            side.into()
                        })
                        .collect(),
                )
            })
        }));
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
    ///
    /// **For new code, prefer [`Self::on_text_input_pre_dispatch_tier1`]**
    /// — it pins the Tier-1 contract from
    /// [ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)
    /// at compile time.
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

    /// Register a tier-1 text-input pre-dispatch handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Returns a [`KakouneSideTextInputPreDispatchResult`] whose
    /// `Consumed` carries `Vec<KakouneSideCommand>`. The `Pass` variant
    /// is identical between tiers (no commands).
    pub fn on_text_input_pre_dispatch_tier1<
        R: Into<KakouneSideTextInputPreDispatchResult> + 'static,
    >(
        &mut self,
        handler: impl Fn(&S, &str, &AppView<'_>) -> (S, R) + Send + Sync + 'static,
    ) {
        self.table.text_input_pre_dispatch_handler = Some(Box::new(move |state, text, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, tier1) = handler(s, text, app);
            let tier1: KakouneSideTextInputPreDispatchResult = tier1.into();
            (Box::new(new_state) as Box<dyn PluginState>, tier1.into())
        }));
    }

    /// Register a mouse pre-dispatch handler — runs before observers and hit-test dispatch.
    ///
    /// Pre-dispatch handlers can `Consume` the event (terminating dispatch) or
    /// `Pass` it through with optional state updates and commands. Used by
    /// drag-tracking and shadow-cursor-class plugins.
    ///
    /// **For new code, prefer [`Self::on_mouse_pre_dispatch_tier1`]** — it
    /// pins the Tier-1 contract from
    /// [ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)
    /// at compile time. Mouse pre-dispatch fires per mouse tick (move
    /// included), so process spawn from the broad `MousePreDispatchResult`
    /// is the same re-entrance class of bug that motivated the ADR.
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

    /// Register a tier-1 mouse pre-dispatch handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Returns a [`KakouneSideMousePreDispatchResult`] whose `commands`
    /// field is `Vec<KakouneSideCommand>` — the bound rejects raw
    /// [`MousePreDispatchResult`] returns at compile time, because there
    /// is intentionally no `From<MousePreDispatchResult>` impl on the
    /// tier-1 type. Migrate by replacing `Vec<Command>` with
    /// `Vec<KakouneSideCommand>` and the result variant with the
    /// `KakouneSideMousePreDispatchResult::*` parallel.
    pub fn on_mouse_pre_dispatch_tier1<R: Into<KakouneSideMousePreDispatchResult> + 'static>(
        &mut self,
        handler: impl Fn(&S, &MouseEvent, &AppView<'_>) -> (S, R) + Send + Sync + 'static,
    ) {
        self.table.mouse_pre_dispatch_handler = Some(Box::new(move |state, event, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, tier1) = handler(s, event, app);
            let tier1: KakouneSideMousePreDispatchResult = tier1.into();
            (Box::new(new_state) as Box<dyn PluginState>, tier1.into())
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

    /// Register a tier-1 mouse fallback handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// `Option<Vec<KakouneSideCommand>>` opt-in for mouse-fallback handlers
    /// that only forward Kakoune-side effects (the common case — the
    /// builtin fallback emits `SendToKakoune` for unhandled mouse events).
    /// The bound rejects raw `Command` returns at compile time.
    pub fn on_mouse_fallback_tier1<C: Into<KakouneSideCommand> + 'static>(
        &mut self,
        handler: impl Fn(&S, &MouseEvent, i32, &AppView<'_>) -> (S, Option<Vec<C>>)
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
            let commands = commands.map(|v| {
                v.into_iter()
                    .map(|c| {
                        let side: KakouneSideCommand = c.into();
                        side.into()
                    })
                    .collect()
            });
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
    /// `Option<(S, Vec<KakouneTransparentCommand>)>` for compile-time transparency.
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

    /// Register a tier-1 mouse handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// `Option<Vec<KakouneSideCommand>>` opt-in for click handlers that
    /// only emit Kakoune-side effects. The bound rejects raw `Command`
    /// returns at compile time — interactive-element click handlers
    /// rarely need to spawn processes, so the tier-1 narrowing matches
    /// the common case.
    pub fn on_handle_mouse_tier1<C: Into<KakouneSideCommand> + 'static>(
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
                    cmds.into_iter()
                        .map(|c| {
                            let side: KakouneSideCommand = c.into();
                            side.into()
                        })
                        .collect(),
                )
            })
        }));
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
    /// `Option<(S, Vec<KakouneTransparentCommand>)>` for compile-time transparency.
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

    /// Register a tier-1 drop handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// `Vec<KakouneSideCommand>` opt-in for drop handlers that only need
    /// Kakoune-side effects.
    pub fn on_drop_tier1<C: Into<KakouneSideCommand> + 'static>(
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
                    cmds.into_iter()
                        .map(|c| {
                            let side: KakouneSideCommand = c.into();
                            side.into()
                        })
                        .collect(),
                )
            })
        }));
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

#[cfg(test)]
mod tier1_mouse_tests {
    //! Positive integration tests for the tier-1 pre-dispatch /
    //! handle-mouse setters across mouse, key, and text-input handlers
    //! (ADR-044). The compile-fail aspect (raw `MousePreDispatchResult` /
    //! `KeyPreDispatchResult` / `TextInputPreDispatchResult` rejected by
    //! their `_tier1` setters, and raw `Command` rejected by
    //! `on_handle_mouse_tier1`) is enforced
    //! structurally by the `R: Into<KakouneSide*>` / `C: Into<KakouneSideCommand>`
    //! bound and the absence of reverse `From` impls — there is no way to
    //! construct a closure that satisfies the bound while returning the
    //! broad type.

    use crate::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
    use crate::plugin::{
        AppView, HandlerRegistry, KakouneSideCommand, KakouneSideKeyPreDispatchResult,
        KakouneSideMousePreDispatchResult, KakouneSideTextInputPreDispatchResult, Plugin,
        PluginBackend, PluginBridge, PluginId, StateUpdates,
    };
    use crate::state::{AppState, DirtyFlags};

    #[derive(Clone, Default, PartialEq, Hash, Debug)]
    struct TestState {
        last_event: u64,
    }

    fn probe_event() -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 0,
            column: 0,
            modifiers: Modifiers::empty(),
        }
    }

    #[test]
    fn tier1_mouse_pre_dispatch_pass_lifts_to_broad_result() {
        struct Plug;
        impl Plugin for Plug {
            type State = TestState;
            fn id(&self) -> PluginId {
                PluginId("test.tier1-mpd-pass".into())
            }
            fn register(&self, r: &mut HandlerRegistry<TestState>) {
                r.on_mouse_pre_dispatch_tier1(|state, _event, _app| {
                    (
                        TestState {
                            last_event: state.last_event + 1,
                        },
                        KakouneSideMousePreDispatchResult::Pass {
                            commands: vec![KakouneSideCommand::request_redraw(DirtyFlags::BUFFER)],
                            state_updates: StateUpdates::default(),
                        },
                    )
                });
            }
        }

        let mut bridge = PluginBridge::new(Plug);
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let result = bridge.handle_mouse_pre_dispatch(&probe_event(), &app);
        match result {
            crate::plugin::MousePreDispatchResult::Pass { commands, .. } => {
                assert_eq!(commands.len(), 1, "tier-1 command should lift through");
            }
            _ => panic!("expected Pass variant"),
        }
    }

    #[test]
    fn tier1_mouse_pre_dispatch_consumed_lifts_to_broad_result() {
        struct Plug;
        impl Plugin for Plug {
            type State = TestState;
            fn id(&self) -> PluginId {
                PluginId("test.tier1-mpd-consumed".into())
            }
            fn register(&self, r: &mut HandlerRegistry<TestState>) {
                r.on_mouse_pre_dispatch_tier1(|s, _event, _app| {
                    (
                        s.clone(),
                        KakouneSideMousePreDispatchResult::Consumed {
                            flags: DirtyFlags::BUFFER,
                            commands: vec![],
                            state_updates: StateUpdates::default(),
                        },
                    )
                });
            }
        }

        let mut bridge = PluginBridge::new(Plug);
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let result = bridge.handle_mouse_pre_dispatch(&probe_event(), &app);
        match result {
            crate::plugin::MousePreDispatchResult::Consumed { flags, .. } => {
                assert!(flags.contains(DirtyFlags::BUFFER));
            }
            _ => panic!("expected Consumed variant"),
        }
    }

    #[test]
    fn tier1_handle_mouse_lifts_to_broad_commands() {
        struct Plug;
        impl Plugin for Plug {
            type State = TestState;
            fn id(&self) -> PluginId {
                PluginId("test.tier1-handle-mouse".into())
            }
            fn register(&self, r: &mut HandlerRegistry<TestState>) {
                r.on_handle_mouse_tier1(|s, _event, _id, _app| {
                    Some((
                        s.clone(),
                        vec![KakouneSideCommand::request_redraw(DirtyFlags::BUFFER)],
                    ))
                });
            }
        }

        let mut bridge = PluginBridge::new(Plug);
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let id = crate::element::InteractiveId::framework(0);
        let result = bridge.handle_mouse(&probe_event(), id, &app);
        match result {
            Some(commands) => assert_eq!(commands.len(), 1),
            None => panic!("tier-1 handle-mouse should consume the event"),
        }
    }

    fn probe_key() -> KeyEvent {
        KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        }
    }

    #[test]
    fn tier1_key_pre_dispatch_pass_lifts_to_broad_result() {
        struct Plug;
        impl Plugin for Plug {
            type State = TestState;
            fn id(&self) -> PluginId {
                PluginId("test.tier1-kpd-pass".into())
            }
            fn register(&self, r: &mut HandlerRegistry<TestState>) {
                r.on_key_pre_dispatch_tier1(|s, _key, _app| {
                    (
                        s.clone(),
                        KakouneSideKeyPreDispatchResult::Pass {
                            commands: vec![KakouneSideCommand::request_redraw(DirtyFlags::BUFFER)],
                            state_updates: StateUpdates::default(),
                        },
                    )
                });
            }
        }

        let mut bridge = PluginBridge::new(Plug);
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let result = bridge.handle_key_pre_dispatch(&probe_key(), &app);
        match result {
            crate::plugin::KeyPreDispatchResult::Pass { commands, .. } => {
                assert_eq!(commands.len(), 1, "tier-1 command should lift through");
            }
            _ => panic!("expected Pass variant"),
        }
    }

    #[test]
    fn tier1_key_pre_dispatch_consumed_preserves_pending_buffer_edit() {
        // The shadow-cursor commit channel is orthogonal to the tier
        // hierarchy; the lift must preserve the optional buffer edit.
        struct Plug;
        impl Plugin for Plug {
            type State = TestState;
            fn id(&self) -> PluginId {
                PluginId("test.tier1-kpd-consumed".into())
            }
            fn register(&self, r: &mut HandlerRegistry<TestState>) {
                r.on_key_pre_dispatch_tier1(|s, _key, _app| {
                    (
                        s.clone(),
                        KakouneSideKeyPreDispatchResult::Consumed {
                            flags: DirtyFlags::STATUS,
                            commands: vec![],
                            state_updates: StateUpdates::default(),
                            pending_buffer_edit: None,
                        },
                    )
                });
            }
        }

        let mut bridge = PluginBridge::new(Plug);
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let result = bridge.handle_key_pre_dispatch(&probe_key(), &app);
        match result {
            crate::plugin::KeyPreDispatchResult::Consumed {
                flags,
                pending_buffer_edit,
                ..
            } => {
                assert!(flags.contains(DirtyFlags::STATUS));
                assert!(pending_buffer_edit.is_none());
            }
            _ => panic!("expected Consumed variant"),
        }
    }

    #[test]
    fn tier1_text_input_pre_dispatch_consumed_lifts_to_broad_result() {
        struct Plug;
        impl Plugin for Plug {
            type State = TestState;
            fn id(&self) -> PluginId {
                PluginId("test.tier1-tipd-consumed".into())
            }
            fn register(&self, r: &mut HandlerRegistry<TestState>) {
                r.on_text_input_pre_dispatch_tier1(|s, _text, _app| {
                    (
                        s.clone(),
                        KakouneSideTextInputPreDispatchResult::Consumed {
                            flags: DirtyFlags::BUFFER,
                            commands: vec![KakouneSideCommand::request_redraw(DirtyFlags::BUFFER)],
                            state_updates: StateUpdates::default(),
                        },
                    )
                });
            }
        }

        let mut bridge = PluginBridge::new(Plug);
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let result = bridge.handle_text_input_pre_dispatch("hi", &app);
        match result {
            crate::plugin::TextInputPreDispatchResult::Consumed {
                flags, commands, ..
            } => {
                assert!(flags.contains(DirtyFlags::BUFFER));
                assert_eq!(commands.len(), 1, "tier-1 command should lift through");
            }
            _ => panic!("expected Consumed variant"),
        }
    }

    #[test]
    fn tier1_text_input_pre_dispatch_pass_round_trips() {
        struct Plug;
        impl Plugin for Plug {
            type State = TestState;
            fn id(&self) -> PluginId {
                PluginId("test.tier1-tipd-pass".into())
            }
            fn register(&self, r: &mut HandlerRegistry<TestState>) {
                r.on_text_input_pre_dispatch_tier1(|s, _text, _app| {
                    (s.clone(), KakouneSideTextInputPreDispatchResult::Pass)
                });
            }
        }

        let mut bridge = PluginBridge::new(Plug);
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let result = bridge.handle_text_input_pre_dispatch("hi", &app);
        assert!(matches!(
            result,
            crate::plugin::TextInputPreDispatchResult::Pass
        ));
    }
}
