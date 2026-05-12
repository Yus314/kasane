//! Render-overlay handlers (menu / info) and the declarative key-map builder.

use crate::element::Overlay;
use crate::input::{CompiledKeyMap, KeyEvent, KeyResponse};

use super::super::{AppView, PluginState};

use super::{HandlerRegistry, KeyMapBuilder};

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    pub fn on_render_menu_overlay(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, &super::super::PluginView<'_>) -> Option<Overlay>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.menu_renderer_handler = Some(Box::new(move |state, app, view| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app, view)
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
        handler: impl Fn(
            &S,
            &AppView<'_>,
            &[crate::layout::Rect],
            &super::super::PluginView<'_>,
        ) -> Option<Vec<Overlay>>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.info_renderer_handler = Some(Box::new(move |state, app, avoid, view| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app, avoid, view)
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

    /// Install a pre-built [`CompiledKeyMap`].
    ///
    /// Counterpart to [`Self::on_key_map`] for plugins whose key-map
    /// origin is *not* the in-process [`KeyMapBuilder`] DSL — primarily
    /// WASM plugins, which compile groups + bindings out-of-process via
    /// the `declare-key-map` WIT export and hand the result to the host
    /// already-built. Pair with [`Self::on_refresh_key_groups`] when the
    /// map carries gated groups, and [`Self::on_invoke_action`] when its
    /// bindings reference named actions.
    pub fn declare_key_map(&mut self, map: CompiledKeyMap) {
        self.table.key_map = Some(map);
    }

    /// Register a refresh handler invoked before each key dispatch to
    /// recompute per-group `active` flags on the installed
    /// [`CompiledKeyMap`].
    ///
    /// The native [`Self::on_key_map`] path wires this automatically
    /// from the builder's group predicates; this lower-level setter is
    /// for callers that installed the map via [`Self::declare_key_map`]
    /// and need to drive group activation from arbitrary state.
    pub fn on_refresh_key_groups(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, &mut CompiledKeyMap) + Send + Sync + 'static,
    ) {
        self.table.group_refresh_handler = Some(Box::new(move |state, app, map| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, app, map);
        }));
    }

    /// Register an action handler invoked when a [`CompiledKeyMap`]
    /// binding fires a named action.
    ///
    /// The native [`Self::on_key_map`] path wires per-action handlers
    /// internally from the builder; this lower-level setter takes a
    /// single dispatcher that receives the action id as a string and
    /// chooses the response.
    pub fn on_invoke_action(
        &mut self,
        handler: impl Fn(&S, &str, &KeyEvent, &AppView<'_>) -> (S, KeyResponse) + Send + Sync + 'static,
    ) {
        self.table.action_handler = Some(Box::new(move |state, action_id, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, response) = handler(s, action_id, key, app);
            (Box::new(new_state) as Box<dyn PluginState>, response)
        }));
    }
}
