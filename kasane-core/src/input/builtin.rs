//! Built-in input plugin that handles PageUp/PageDown and default buffer scroll policy.
//!
//! Registered as the lowest-priority plugin so that any user plugin
//! can override these keys via `handle_key()`.

use crate::input::{Key, KeyEvent};
use crate::plugin::{Command, PluginBackend, PluginCapabilities, PluginId};
use crate::protocol::KasaneRequest;
use crate::scroll::{
    DefaultScrollCandidate, ScrollPolicyResult, legacy_smooth_scroll_plan, smooth_scroll_enabled,
};
use crate::state::AppState;

/// Built-in plugin for default key bindings and bundled scroll policy.
///
/// Registered last in the plugin chain so all other plugins get
/// first-wins priority on these keys and on default scroll policy decisions.
pub struct BuiltinInputPlugin;

impl PluginBackend for BuiltinInputPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.input".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::INPUT_HANDLER | PluginCapabilities::SCROLL_POLICY
    }

    fn handle_key(&mut self, key: &KeyEvent, state: &AppState) -> Option<Vec<Command>> {
        if !key.modifiers.is_empty() {
            return None;
        }
        match key.key {
            Key::PageUp => {
                let cmd = Command::SendToKakoune(KasaneRequest::Scroll {
                    amount: -(state.available_height() as i32),
                    line: state.cursor_pos.line as u32,
                    column: state.cursor_pos.column as u32,
                });
                Some(vec![cmd])
            }
            Key::PageDown => {
                let cmd = Command::SendToKakoune(KasaneRequest::Scroll {
                    amount: state.available_height() as i32,
                    line: state.cursor_pos.line as u32,
                    column: state.cursor_pos.column as u32,
                });
                Some(vec![cmd])
            }
            _ => None,
        }
    }

    fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        state: &AppState,
    ) -> Option<ScrollPolicyResult> {
        Some(if smooth_scroll_enabled(state) {
            ScrollPolicyResult::Plan(legacy_smooth_scroll_plan(candidate))
        } else {
            ScrollPolicyResult::Immediate(candidate.resolved)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;

    #[test]
    fn test_builtin_handles_pageup() {
        let mut plugin = BuiltinInputPlugin;
        let state = AppState::default();
        let key = KeyEvent {
            key: Key::PageUp,
            modifiers: Modifiers::empty(),
        };
        let result = plugin.handle_key(&key, &state);
        assert!(result.is_some());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(
            matches!(cmds[0], Command::SendToKakoune(KasaneRequest::Scroll { amount, .. }) if amount < 0)
        );
    }

    #[test]
    fn test_builtin_handles_pagedown() {
        let mut plugin = BuiltinInputPlugin;
        let state = AppState::default();
        let key = KeyEvent {
            key: Key::PageDown,
            modifiers: Modifiers::empty(),
        };
        let result = plugin.handle_key(&key, &state);
        assert!(result.is_some());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(
            matches!(cmds[0], Command::SendToKakoune(KasaneRequest::Scroll { amount, .. }) if amount > 0)
        );
    }

    #[test]
    fn test_builtin_ignores_modified_pageup() {
        let mut plugin = BuiltinInputPlugin;
        let state = AppState::default();
        let key = KeyEvent {
            key: Key::PageUp,
            modifiers: Modifiers::CTRL,
        };
        assert!(plugin.handle_key(&key, &state).is_none());
    }

    #[test]
    fn test_builtin_ignores_other_keys() {
        let mut plugin = BuiltinInputPlugin;
        let state = AppState::default();
        let key = KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        };
        assert!(plugin.handle_key(&key, &state).is_none());
    }

    #[test]
    fn test_user_plugin_overrides_builtin() {
        use crate::input::Modifiers;
        use crate::plugin::PluginRegistry;
        use crate::state::{Msg, update};

        struct CustomPageUpPlugin;
        impl PluginBackend for CustomPageUpPlugin {
            fn id(&self) -> PluginId {
                PluginId("custom_pageup".into())
            }
            fn handle_key(&mut self, key: &KeyEvent, _state: &AppState) -> Option<Vec<Command>> {
                if key.key == Key::PageUp {
                    Some(vec![Command::SendToKakoune(KasaneRequest::Keys(vec![
                        "custom".to_string(),
                    ]))])
                } else {
                    None
                }
            }
        }

        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        // Custom plugin registered BEFORE builtin → gets priority
        registry.register_backend(Box::new(CustomPageUpPlugin));
        registry.register_backend(Box::new(BuiltinInputPlugin));
        let key = KeyEvent {
            key: Key::PageUp,
            modifiers: Modifiers::empty(),
        };
        let commands = update(&mut state, Msg::Key(key), &mut registry, 3).commands;
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::SendToKakoune(KasaneRequest::Keys(keys)) => {
                assert_eq!(keys[0], "custom");
            }
            _ => panic!("expected custom handler to win"),
        }
    }

    #[test]
    fn test_builtin_fallback_when_no_override() {
        use crate::input::Modifiers;
        use crate::plugin::PluginRegistry;
        use crate::state::{Msg, update};

        // Plugin that doesn't handle PageUp
        struct NoOpPlugin;
        impl PluginBackend for NoOpPlugin {
            fn id(&self) -> PluginId {
                PluginId("noop".into())
            }
        }

        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        registry.register_backend(Box::new(NoOpPlugin));
        registry.register_backend(Box::new(BuiltinInputPlugin));
        let key = KeyEvent {
            key: Key::PageUp,
            modifiers: Modifiers::empty(),
        };
        let commands = update(&mut state, Msg::Key(key), &mut registry, 3).commands;
        assert_eq!(commands.len(), 1);
        // BuiltinInputPlugin should handle it as a Scroll command
        assert!(matches!(
            commands[0],
            Command::SendToKakoune(KasaneRequest::Scroll { .. })
        ));
    }

    #[test]
    fn test_builtin_scroll_policy_immediate_when_smooth_disabled() {
        let mut plugin = BuiltinInputPlugin;
        let state = AppState::default();
        let candidate = DefaultScrollCandidate::new(
            10,
            5,
            Modifiers::empty(),
            crate::scroll::ScrollGranularity::Line,
            3,
            crate::scroll::ResolvedScroll::new(3, 10, 5),
        );

        let result = plugin.handle_default_scroll(candidate, &state);

        assert_eq!(
            result,
            Some(ScrollPolicyResult::Immediate(
                crate::scroll::ResolvedScroll::new(3, 10, 5)
            ))
        );
    }

    #[test]
    fn test_builtin_scroll_policy_plans_when_smooth_enabled() {
        let mut plugin = BuiltinInputPlugin;
        let mut state = AppState::default();
        crate::scroll::set_smooth_scroll_enabled(&mut state.plugin_config, true);
        let candidate = DefaultScrollCandidate::new(
            10,
            5,
            Modifiers::empty(),
            crate::scroll::ScrollGranularity::Line,
            3,
            crate::scroll::ResolvedScroll::new(3, 10, 5),
        );

        let result = plugin.handle_default_scroll(candidate, &state);

        assert_eq!(
            result,
            Some(ScrollPolicyResult::Plan(legacy_smooth_scroll_plan(
                candidate
            )))
        );
    }
}
