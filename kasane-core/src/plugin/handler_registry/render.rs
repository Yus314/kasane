//! Carve-outs on the render-overlay / key-map axis.
//!
//! γ-3.3c-5b: the redundant manual `on_menu_renderer` / `on_info_renderer` /
//! `on_group_refresh` / `on_action` setters were retired — plugin code
//! now invokes the macro-generated counterparts via `Deref` from
//! `HandlerRegistry` to `gen::HandlerRegistry`. The two manual setters
//! retained are:
//!
//! - **`on_key_map(builder)`** — orchestrator carve-out (spec §9.2)
//!   that composes three internal table fields (`key_map`,
//!   `group_refresh_handler`, `action_handler`) from a single
//!   [`KeyMapBuilder`] closure.
//! - **`declare_key_map(map)`** — config-write counterpart for callers
//!   that have a pre-built [`CompiledKeyMap`] (e.g. WASM plugins
//!   compiling out-of-process via the `declare-key-map` WIT export).

use crate::input::{CompiledKeyMap, KeyEvent, KeyResponse};

use super::super::{AppView, PluginState};

use super::{HandlerRegistry, KeyMapBuilder};

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
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
        self.inner.table.key_map = Some(initial_map);

        // Store group refresh handler: evaluates `when()` predicates against state.
        let group_predicates = km.group_predicates;
        self.inner.table.group_refresh_handler = Some(Box::new(
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
        self.inner.table.action_handler = Some(Box::new(
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
    /// already-built. Pair with the generated `on_group_refresh` when
    /// the map carries gated groups, and the generated `on_action` when
    /// its bindings reference named actions.
    pub fn declare_key_map(&mut self, map: CompiledKeyMap) {
        self.inner.table.key_map = Some(map);
    }
}
