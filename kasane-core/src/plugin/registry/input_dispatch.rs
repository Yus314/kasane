//! Input dispatch methods for [`PluginRuntime`].
//!
//! Handles key, mouse, text input, and drop event dispatch across registered plugins.

use crate::element::{InteractiveId, PluginTag};
use crate::input::{DropEvent, KeyEvent, KeyResponse, MouseEvent};
use crate::state::DirtyFlags;

use crate::plugin::effects::{MouseHandleResult, TextInputHandleResult};
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
    pub fn dispatch_key_pre_dispatch(
        &mut self,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyPreDispatchResult {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::KEY_PRE_DISPATCH)
            {
                continue;
            }
            let result = slot.backend.handle_key_pre_dispatch(key, app);
            match result {
                KeyPreDispatchResult::Consumed { .. } => return result,
                KeyPreDispatchResult::Pass { ref commands } if !commands.is_empty() => {
                    return result;
                }
                KeyPreDispatchResult::Pass { .. } => continue,
            }
        }
        KeyPreDispatchResult::Pass { commands: vec![] }
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
                } => {
                    commands.splice(0..0, pass_commands);
                    return MousePreDispatchResult::Consumed { flags, commands };
                }
                MousePreDispatchResult::Pass { commands } => {
                    pass_commands.extend(commands);
                }
            }
        }
        MousePreDispatchResult::Pass {
            commands: pass_commands,
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
