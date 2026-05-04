//! Input dispatch methods for [`PluginRuntime`].
//!
//! Handles key, mouse, text input, and drop event dispatch across registered plugins.

use crate::element::{InteractiveId, PluginTag};
use crate::input::{DropEvent, KeyEvent, KeyResponse, MouseEvent};
use crate::state::DirtyFlags;

use crate::plugin::effects::{MouseHandleResult, StateUpdates, TextInputHandleResult};
use crate::plugin::traits::MousePreDispatchResult;
use crate::plugin::{
    AppView, Command, KeyHandleResult, KeyPreDispatchResult, PluginCapabilities, PluginId,
    TextInputPreDispatchResult,
};

use super::{KeyDispatchResult, PluginRuntime, PluginSlot};

impl PluginRuntime {
    pub fn dispatch_key_middleware(
        &mut self,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyDispatchResult {
        let mut current_key = key.clone();
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            // --- Key map dispatch path (Phase 2+) ---
            if slot.backend.compiled_key_map().is_some() {
                // Refresh group active flags if state changed.
                let current_hash = slot.backend.state_hash();
                if current_hash != slot.last_group_refresh_hash {
                    slot.backend.refresh_key_groups(app);
                    slot.last_group_refresh_hash = current_hash;
                }

                if let Some(result) = Self::dispatch_key_map(slot, &current_key, app) {
                    return result;
                }
                // No match in this plugin's key map — fall through to next plugin.
                continue;
            }

            // --- Legacy dispatch path ---
            match slot.backend.handle_key_middleware(&current_key, app) {
                KeyHandleResult::Consumed(commands) => {
                    return KeyDispatchResult::Consumed {
                        source_plugin: slot.backend.id(),
                        commands,
                    };
                }
                KeyHandleResult::Transformed(next_key) => current_key = next_key,
                KeyHandleResult::Passthrough => {}
            }
        }

        KeyDispatchResult::Passthrough(current_key)
    }

    pub fn dispatch_text_input_handler(
        &mut self,
        text: &str,
        app: &AppView<'_>,
    ) -> TextInputHandleResult {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            if let Some(commands) = slot.backend.handle_text_input(text, app) {
                return TextInputHandleResult::Handled {
                    source_plugin: slot.backend.id(),
                    commands,
                };
            }
        }

        TextInputHandleResult::NotHandled
    }

    /// Key map dispatch for a single plugin slot.
    ///
    /// Returns `Some(result)` if the key was consumed or a chord was started,
    /// `None` if this plugin doesn't handle the key.
    fn dispatch_key_map(
        slot: &mut PluginSlot,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> Option<KeyDispatchResult> {
        use crate::input::key_map::DEFAULT_CHORD_TIMEOUT_MS;

        let plugin_id = slot.backend.id();

        // 1. If a chord is pending, try to resolve it.
        if slot.chord_state.is_pending() {
            let timeout = slot
                .backend
                .compiled_key_map()
                .map_or(DEFAULT_CHORD_TIMEOUT_MS, |m| m.chord_timeout_ms);

            if slot.chord_state.is_timed_out(timeout) {
                // Timeout: cancel chord, re-dispatch this key from scratch.
                slot.chord_state.cancel();
                return Self::dispatch_key_map(slot, key, app);
            }

            let leader = slot.chord_state.pending_leader.clone().unwrap();
            if let Some(action_id) = slot
                .backend
                .compiled_key_map()
                .and_then(|m| m.match_chord_follower(&leader, key))
            {
                // Chord matched — invoke action.
                slot.chord_state.cancel();
                let response = slot.backend.invoke_action(action_id, key, app);
                return Some(Self::key_response_to_dispatch(response, plugin_id));
            }

            // No chord match — cancel and pass through (don't consume).
            slot.chord_state.cancel();
            return None;
        }

        // 2. Not pending — check for chord leader.
        if slot
            .backend
            .compiled_key_map()
            .is_some_and(|m| m.match_chord_leader(key))
        {
            slot.chord_state.set_pending(key.clone());
            return Some(KeyDispatchResult::Consumed {
                source_plugin: plugin_id,
                commands: vec![],
            });
        }

        // 3. Try single-key binding.
        if let Some(action_id) = slot
            .backend
            .compiled_key_map()
            .and_then(|m| m.match_key(key))
        {
            let response = slot.backend.invoke_action(action_id, key, app);
            return Some(Self::key_response_to_dispatch(response, plugin_id));
        }

        // 4. No match at all — passthrough.
        None
    }

    fn key_response_to_dispatch(response: KeyResponse, plugin_id: PluginId) -> KeyDispatchResult {
        match response {
            KeyResponse::Pass => KeyDispatchResult::Passthrough(KeyEvent {
                key: crate::input::Key::Escape,
                modifiers: crate::input::Modifiers::empty(),
            }),
            KeyResponse::Consume => KeyDispatchResult::Consumed {
                source_plugin: plugin_id,
                commands: vec![],
            },
            KeyResponse::ConsumeRedraw => KeyDispatchResult::Consumed {
                source_plugin: plugin_id,
                commands: vec![Command::RequestRedraw(DirtyFlags::ALL)],
            },
            KeyResponse::ConsumeWith(commands) => KeyDispatchResult::Consumed {
                source_plugin: plugin_id,
                commands,
            },
        }
    }

    /// Dispatch key pre-dispatch to plugins with KEY_PRE_DISPATCH capability.
    /// First plugin returning `Consumed` wins.
    ///
    /// ADR-035 ShadowCursor follow-up: when the consuming plugin
    /// surfaces a `pending_buffer_edit`, this method runs the
    /// `intercept_buffer_edit` chain across all registered plugins
    /// (in slot order), folds verdicts (`Replace` substitutes the
    /// running edit; `Veto` short-circuits and drops the commit),
    /// and serializes the final edit into Kakoune commands via
    /// `state::shadow_cursor::edit_to_commands` before returning.
    pub fn dispatch_key_pre_dispatch(
        &mut self,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyPreDispatchResult {
        let mut consuming: Option<KeyPreDispatchResult> = None;
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::KEY_PRE_DISPATCH)
            {
                continue;
            }
            let result = slot.backend.handle_key_pre_dispatch(key, app);
            match result {
                KeyPreDispatchResult::Consumed { .. } => {
                    consuming = Some(result);
                    break;
                }
                KeyPreDispatchResult::Pass {
                    ref commands,
                    ref state_updates,
                } if !commands.is_empty() || !state_updates.is_empty() => {
                    return result;
                }
                KeyPreDispatchResult::Pass { .. } => continue,
            }
        }

        let Some(mut result) = consuming else {
            return KeyPreDispatchResult::Pass {
                commands: vec![],
                state_updates: StateUpdates::default(),
            };
        };

        // Intercept dispatch (ADR-035 ShadowCursor follow-up): if
        // the consumer surfaced a pending BufferEdit, fold the
        // intercept chain over all plugins and serialize the
        // final edit (if any) into commands.
        if let KeyPreDispatchResult::Consumed {
            commands,
            pending_buffer_edit,
            ..
        } = &mut result
            && let Some(edit) = pending_buffer_edit.take()
        {
            use crate::state::shadow_cursor::edit_to_commands;
            let mut backends: Vec<&mut dyn crate::plugin::PluginBackend> =
                self.slots.iter_mut().map(|s| s.backend.as_mut()).collect();
            if let Some(final_edit) = fold_intercept_chain(edit, &mut backends, app)
                && !final_edit.is_hippocratic_noop()
            {
                commands.extend(edit_to_commands(&final_edit));
            }
        }

        result
    }

    /// Dispatch text input pre-dispatch to plugins with KEY_PRE_DISPATCH capability.
    pub fn dispatch_text_input_pre_dispatch(
        &mut self,
        text: &str,
        app: &AppView<'_>,
    ) -> TextInputPreDispatchResult {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::KEY_PRE_DISPATCH)
            {
                continue;
            }
            let result = slot.backend.handle_text_input_pre_dispatch(text, app);
            match result {
                TextInputPreDispatchResult::Consumed { .. } => return result,
                TextInputPreDispatchResult::Pass => continue,
            }
        }
        TextInputPreDispatchResult::Pass
    }

    /// Dispatch mouse pre-dispatch to plugins with MOUSE_PRE_DISPATCH capability.
    /// First plugin returning `Consumed` wins. Pass commands are collected.
    pub fn dispatch_mouse_pre_dispatch(
        &mut self,
        event: &MouseEvent,
        app: &AppView<'_>,
    ) -> MousePreDispatchResult {
        let mut pass_commands = Vec::new();
        let mut pass_state_updates = StateUpdates::default();
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::MOUSE_PRE_DISPATCH)
            {
                continue;
            }
            let result = slot.backend.handle_mouse_pre_dispatch(event, app);
            match result {
                MousePreDispatchResult::Consumed {
                    flags,
                    mut commands,
                    state_updates,
                } => {
                    commands.splice(0..0, pass_commands);
                    let mut merged = pass_state_updates;
                    merged.merge(state_updates);
                    return MousePreDispatchResult::Consumed {
                        flags,
                        commands,
                        state_updates: merged,
                    };
                }
                MousePreDispatchResult::Pass {
                    commands,
                    state_updates,
                } => {
                    pass_commands.extend(commands);
                    pass_state_updates.merge(state_updates);
                }
            }
        }
        MousePreDispatchResult::Pass {
            commands: pass_commands,
            state_updates: pass_state_updates,
        }
    }

    /// Dispatch mouse fallback to plugins with MOUSE_FALLBACK capability.
    /// First plugin returning `Some` wins.
    pub fn dispatch_mouse_fallback(
        &mut self,
        event: &MouseEvent,
        scroll_amount: i32,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::MOUSE_FALLBACK)
            {
                continue;
            }
            if let Some(commands) = slot
                .backend
                .handle_mouse_fallback(event, scroll_amount, app)
            {
                return Some(commands);
            }
        }
        None
    }

    /// Broadcast key observation to all plugins with INPUT_HANDLER capability.
    pub fn observe_key_all(&mut self, key: &KeyEvent, app: &AppView<'_>) {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            slot.backend.observe_key(key, app);
        }
    }

    /// Broadcast committed text input observation to all plugins with INPUT_HANDLER capability.
    pub fn observe_text_input_all(&mut self, text: &str, app: &AppView<'_>) {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            slot.backend.observe_text_input(text, app);
        }
    }

    /// Broadcast mouse observation to all plugins with INPUT_HANDLER capability.
    pub fn observe_mouse_all(&mut self, event: &MouseEvent, app: &AppView<'_>) {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            slot.backend.observe_mouse(event, app);
        }
    }

    /// Owner-based mouse handler dispatch.
    ///
    /// If the `InteractiveId` has a plugin owner tag, dispatches directly to the
    /// owning plugin (O(1) lookup). Falls back to first-wins iteration for
    /// framework-owned or unassigned IDs.
    pub fn dispatch_mouse_handler(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult {
        if id.owner != PluginTag::FRAMEWORK && id.owner != PluginTag::UNASSIGNED {
            // Direct dispatch to owning plugin
            if let Some(slot) = self.slots.iter_mut().find(|s| s.plugin_tag == id.owner)
                && let Some(commands) = slot.backend.handle_mouse(event, id, app)
            {
                return MouseHandleResult::Handled {
                    source_plugin: slot.backend.id(),
                    commands,
                };
            }
            return MouseHandleResult::NotHandled;
        }
        // Legacy fallback for framework/unassigned IDs
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            if let Some(commands) = slot.backend.handle_mouse(event, id, app) {
                let source = slot.backend.id();
                return MouseHandleResult::Handled {
                    source_plugin: source,
                    commands,
                };
            }
        }
        MouseHandleResult::NotHandled
    }

    /// Broadcast drop observation to all plugins with DROP_HANDLER capability.
    pub fn observe_drop_all(&mut self, event: &DropEvent, app: &AppView<'_>) {
        for slot in &mut self.slots {
            if !slot.capabilities.contains(PluginCapabilities::DROP_HANDLER) {
                continue;
            }
            slot.backend.observe_drop(event, app);
        }
    }

    /// Owner-based drop handler dispatch.
    pub fn dispatch_drop_handler(
        &mut self,
        event: &DropEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult {
        if id.owner != PluginTag::FRAMEWORK && id.owner != PluginTag::UNASSIGNED {
            if let Some(slot) = self.slots.iter_mut().find(|s| s.plugin_tag == id.owner)
                && let Some(commands) = slot.backend.handle_drop(event, id, app)
            {
                return MouseHandleResult::Handled {
                    source_plugin: slot.backend.id(),
                    commands,
                };
            }
            return MouseHandleResult::NotHandled;
        }
        for slot in &mut self.slots {
            if !slot.capabilities.contains(PluginCapabilities::DROP_HANDLER) {
                continue;
            }
            if let Some(commands) = slot.backend.handle_drop(event, id, app) {
                return MouseHandleResult::Handled {
                    source_plugin: slot.backend.id(),
                    commands,
                };
            }
        }
        MouseHandleResult::NotHandled
    }
}

/// Fold the buffer-edit intercept chain over a slice of plugin
/// backends in slot order (ADR-035 ShadowCursor follow-up).
///
/// Each plugin's `intercept_buffer_edit` returns a verdict:
/// - `PassThrough` keeps the running edit unchanged.
/// - `Replace(new)` substitutes the running edit with `new`.
/// - `Veto` short-circuits and returns `None` from the fold.
///
/// Returns the final edit (or `None` on veto). The caller is
/// responsible for the Hippocratic noop check and command
/// serialization; this function is purely the verdict fold.
pub fn fold_intercept_chain(
    initial: crate::state::shadow_cursor::BufferEdit,
    backends: &mut [&mut dyn crate::plugin::PluginBackend],
    app: &AppView<'_>,
) -> Option<crate::state::shadow_cursor::BufferEdit> {
    use crate::state::shadow_cursor::BufferEditVerdict;
    let mut current = Some(initial);
    for backend in backends {
        let Some(running) = current.as_ref() else {
            break;
        };
        match backend.intercept_buffer_edit(running, app) {
            BufferEditVerdict::PassThrough => {}
            BufferEditVerdict::Replace(new_edit) => current = Some(new_edit),
            BufferEditVerdict::Veto => current = None,
        }
    }
    current
}

#[cfg(test)]
mod intercept_tests {
    use super::*;
    use crate::history::VersionId;
    use crate::plugin::{Effects, PluginBackend, PluginId};
    use crate::state::AppState;
    use crate::state::selection::{BufferPos, Selection};
    use crate::state::shadow_cursor::{BufferEdit, BufferEditVerdict};

    fn mk_edit(replacement: &str) -> BufferEdit {
        BufferEdit {
            target: Selection::new(BufferPos::new(0, 0), BufferPos::new(0, 5)),
            original: "hello".into(),
            replacement: replacement.into(),
            base_version: VersionId::INITIAL,
        }
    }

    /// A minimal PluginBackend that returns a fixed verdict from
    /// `intercept_buffer_edit` and default impls for everything
    /// else.
    struct FixedVerdictBackend {
        id: PluginId,
        verdict: BufferEditVerdict,
    }
    crate::impl_migrated_caps_default!(FixedVerdictBackend);

    impl PluginBackend for FixedVerdictBackend {
        fn id(&self) -> PluginId {
            self.id.clone()
        }

        fn on_init_effects(&mut self, _state: &AppView<'_>) -> Effects {
            Effects::default()
        }

        fn intercept_buffer_edit(
            &mut self,
            _edit: &BufferEdit,
            _state: &AppView<'_>,
        ) -> BufferEditVerdict {
            self.verdict.clone()
        }
    }

    fn backend(id: &str, verdict: BufferEditVerdict) -> FixedVerdictBackend {
        FixedVerdictBackend {
            id: PluginId(id.into()),
            verdict,
        }
    }

    #[test]
    fn empty_chain_returns_initial_edit() {
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let initial = mk_edit("world");
        let mut backends: Vec<&mut dyn PluginBackend> = vec![];
        let out = fold_intercept_chain(initial.clone(), &mut backends, &app);
        assert_eq!(out, Some(initial));
    }

    #[test]
    fn pass_through_chain_returns_initial_edit() {
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let mut a = backend("a", BufferEditVerdict::PassThrough);
        let mut b = backend("b", BufferEditVerdict::PassThrough);
        let mut backends: Vec<&mut dyn PluginBackend> = vec![&mut a, &mut b];
        let out = fold_intercept_chain(mk_edit("world"), &mut backends, &app);
        assert_eq!(out.as_ref().map(|e| e.replacement.as_str()), Some("world"));
    }

    #[test]
    fn replace_substitutes_running_edit() {
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let mut a = backend("a", BufferEditVerdict::Replace(mk_edit("WORLD")));
        let mut backends: Vec<&mut dyn PluginBackend> = vec![&mut a];
        let out = fold_intercept_chain(mk_edit("world"), &mut backends, &app);
        assert_eq!(out.as_ref().map(|e| e.replacement.as_str()), Some("WORLD"));
    }

    #[test]
    fn replace_then_replace_chains_in_order() {
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let mut a = backend("a", BufferEditVerdict::Replace(mk_edit("first")));
        let mut b = backend("b", BufferEditVerdict::Replace(mk_edit("second")));
        let mut backends: Vec<&mut dyn PluginBackend> = vec![&mut a, &mut b];
        let out = fold_intercept_chain(mk_edit("world"), &mut backends, &app);
        // b runs after a; b's Replace wins.
        assert_eq!(out.as_ref().map(|e| e.replacement.as_str()), Some("second"));
    }

    #[test]
    fn veto_short_circuits_returning_none() {
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let mut a = backend("a", BufferEditVerdict::Veto);
        let mut b = backend("b", BufferEditVerdict::Replace(mk_edit("WORLD")));
        let mut backends: Vec<&mut dyn PluginBackend> = vec![&mut a, &mut b];
        let out = fold_intercept_chain(mk_edit("world"), &mut backends, &app);
        assert_eq!(out, None);
    }

    #[test]
    fn replace_then_veto_returns_none() {
        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let mut a = backend("a", BufferEditVerdict::Replace(mk_edit("WORLD")));
        let mut b = backend("b", BufferEditVerdict::Veto);
        let mut backends: Vec<&mut dyn PluginBackend> = vec![&mut a, &mut b];
        let out = fold_intercept_chain(mk_edit("world"), &mut backends, &app);
        assert_eq!(out, None);
    }

    #[test]
    fn veto_does_not_invoke_subsequent_handlers() {
        // A counter-driven backend that we can inspect.
        struct CountingBackend {
            id: PluginId,
            invoked: std::sync::Arc<std::sync::atomic::AtomicUsize>,
            verdict: BufferEditVerdict,
        }
        crate::impl_migrated_caps_default!(CountingBackend);
        impl PluginBackend for CountingBackend {
            fn id(&self) -> PluginId {
                self.id.clone()
            }
            fn on_init_effects(&mut self, _state: &AppView<'_>) -> Effects {
                Effects::default()
            }
            fn intercept_buffer_edit(
                &mut self,
                _edit: &BufferEdit,
                _state: &AppView<'_>,
            ) -> BufferEditVerdict {
                self.invoked
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                self.verdict.clone()
            }
        }

        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let mut a = backend("vetoer", BufferEditVerdict::Veto);
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut b = CountingBackend {
            id: PluginId("after-veto".into()),
            invoked: counter.clone(),
            verdict: BufferEditVerdict::PassThrough,
        };
        let mut backends: Vec<&mut dyn PluginBackend> = vec![&mut a, &mut b];
        let _ = fold_intercept_chain(mk_edit("world"), &mut backends, &app);
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "downstream plugin must not be invoked after a Veto"
        );
    }
}
