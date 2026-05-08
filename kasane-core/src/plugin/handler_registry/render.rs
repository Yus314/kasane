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
}
