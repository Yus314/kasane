//! Built-in input plugin that handles PageUp/PageDown and default buffer scroll policy.
//!
//! Registered as the lowest-priority plugin so that any user plugin
//! can override these keys via `handle_key()`.

use crate::input::Key;
use crate::plugin::{HandlerRegistry, KakouneSideCommand, Plugin, PluginId};
use crate::protocol::KasaneRequest;
use crate::scroll::ScrollPolicyResult;

/// Built-in plugin for default key bindings and the production scroll policy fallback.
///
/// Registered last in the plugin chain so all other plugins get
/// first-wins priority on these keys and on default scroll policy decisions.
pub struct BuiltinInputPlugin;

impl Plugin for BuiltinInputPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.input".into())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        // Tier 1 (ADR-044): PageUp/PageDown forward to Kakoune as Scroll
        // requests; no process spawn. `on_key_tier1` rejects any future
        // change that would emit a `ProcessCommand` here.
        r.on_key_tier1(|_state, key, app| {
            if !key.modifiers.is_empty() {
                return None;
            }
            let cmd = match key.key {
                Key::PageUp => KakouneSideCommand::send_to_kakoune(KasaneRequest::Scroll {
                    amount: -(app.available_height() as i32),
                    line: app.cursor_line() as u32,
                    column: app.cursor_col() as u32,
                }),
                Key::PageDown => KakouneSideCommand::send_to_kakoune(KasaneRequest::Scroll {
                    amount: app.available_height() as i32,
                    line: app.cursor_line() as u32,
                    column: app.cursor_col() as u32,
                }),
                _ => return None,
            };
            Some(((), vec![cmd]))
        });

        r.on_default_scroll(|_state, candidate, _app| {
            Some(((), ScrollPolicyResult::Immediate(candidate.resolved)))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{KeyEvent, Modifiers};
    use crate::plugin::{AppView, Command, PluginBackend, PluginBridge};
    use crate::scroll::{DefaultScrollCandidate, resolve_default_scroll_policy};
    use crate::state::AppState;

    fn bridge() -> PluginBridge {
        PluginBridge::new(BuiltinInputPlugin)
    }

    #[test]
    fn test_builtin_handles_pageup() {
        let mut plugin = bridge();
        let state = AppState::default();
        let view = AppView::new(&state);
        let key = KeyEvent {
            key: Key::PageUp,
            modifiers: Modifiers::empty(),
        };
        let result = plugin.handle_key(&key, &view);
        assert!(result.is_some());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(
            matches!(cmds[0], Command::SendToKakoune(KasaneRequest::Scroll { amount, .. }) if amount < 0)
        );
    }

    #[test]
    fn test_builtin_handles_pagedown() {
        let mut plugin = bridge();
        let state = AppState::default();
        let view = AppView::new(&state);
        let key = KeyEvent {
            key: Key::PageDown,
            modifiers: Modifiers::empty(),
        };
        let result = plugin.handle_key(&key, &view);
        assert!(result.is_some());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(
            matches!(cmds[0], Command::SendToKakoune(KasaneRequest::Scroll { amount, .. }) if amount > 0)
        );
    }

    #[test]
    fn test_builtin_ignores_modified_pageup() {
        let mut plugin = bridge();
        let state = AppState::default();
        let view = AppView::new(&state);
        let key = KeyEvent {
            key: Key::PageUp,
            modifiers: Modifiers::CTRL,
        };
        assert!(plugin.handle_key(&key, &view).is_none());
    }

    #[test]
    fn test_builtin_ignores_other_keys() {
        let mut plugin = bridge();
        let state = AppState::default();
        let view = AppView::new(&state);
        let key = KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        };
        assert!(plugin.handle_key(&key, &view).is_none());
    }

    #[test]
    fn test_user_plugin_overrides_builtin() {
        use crate::input::Modifiers;
        use crate::plugin::PluginRuntime;
        use crate::state::{Msg, update_in_place};

        struct CustomPageUpPlugin;
        impl crate::plugin::Plugin for CustomPageUpPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("custom_pageup".into())
            }
            fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
                r.on_key(|_state, key, _app| {
                    if key.key == Key::PageUp {
                        Some((
                            (),
                            vec![Command::SendToKakoune(KasaneRequest::Keys(vec![
                                "custom".to_string(),
                            ]))],
                        ))
                    } else {
                        None
                    }
                });
            }
        }

        let mut state = Box::new(AppState::default());
        let mut registry = PluginRuntime::new();
        // Custom plugin registered BEFORE builtin → gets priority
        registry.register(CustomPageUpPlugin);
        registry.register(BuiltinInputPlugin);
        let key = KeyEvent {
            key: Key::PageUp,
            modifiers: Modifiers::empty(),
        };
        let commands = update_in_place(&mut state, Msg::Key(key), &mut registry, 3).commands;
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
        use crate::plugin::PluginRuntime;
        use crate::state::{Msg, update_in_place};

        // Plugin that doesn't handle PageUp
        struct NoOpPlugin;
        impl crate::plugin::Plugin for NoOpPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("noop".into())
            }
            fn register(&self, _r: &mut crate::plugin::HandlerRegistry<()>) {}
        }

        let mut state = Box::new(AppState::default());
        let mut registry = PluginRuntime::new();
        registry.register(NoOpPlugin);
        registry.register(BuiltinInputPlugin);
        let key = KeyEvent {
            key: Key::PageUp,
            modifiers: Modifiers::empty(),
        };
        let commands = update_in_place(&mut state, Msg::Key(key), &mut registry, 3).commands;
        assert_eq!(commands.len(), 1);
        // BuiltinInputPlugin should handle it as a Scroll command
        assert!(matches!(
            commands[0],
            Command::SendToKakoune(KasaneRequest::Scroll { .. })
        ));
    }

    #[test]
    fn test_builtin_scroll_policy_immediate_when_smooth_disabled() {
        let mut plugin = bridge();
        let state = AppState::default();
        let view = AppView::new(&state);
        let candidate = DefaultScrollCandidate::new(
            10,
            5,
            Modifiers::empty(),
            crate::scroll::ScrollGranularity::Line,
            3,
            crate::scroll::ResolvedScroll::new(3, 10, 5),
        );

        let result = plugin.handle_default_scroll(candidate, &view);

        assert_eq!(
            result,
            Some(ScrollPolicyResult::Immediate(
                crate::scroll::ResolvedScroll::new(3, 10, 5)
            ))
        );
    }

    #[test]
    fn test_builtin_scroll_returns_immediate() {
        let mut plugin = bridge();
        let state = AppState::default();
        let view = AppView::new(&state);
        let candidate = DefaultScrollCandidate::new(
            10,
            5,
            Modifiers::empty(),
            crate::scroll::ScrollGranularity::Line,
            3,
            crate::scroll::ResolvedScroll::new(3, 10, 5),
        );

        let result = plugin.handle_default_scroll(candidate, &view);

        assert_eq!(
            result,
            Some(ScrollPolicyResult::Immediate(candidate.resolved))
        );
    }

    #[test]
    fn test_user_scroll_policy_overrides_builtin_production_default() {
        struct OverrideScrollPlugin;

        impl crate::plugin::Plugin for OverrideScrollPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("override.scroll".into())
            }
            fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
                r.on_default_scroll(|_state, _candidate, _app| {
                    Some(((), ScrollPolicyResult::Suppress))
                });
            }
        }

        let state = AppState::default();
        let candidate = DefaultScrollCandidate::new(
            10,
            5,
            Modifiers::empty(),
            crate::scroll::ScrollGranularity::Line,
            3,
            crate::scroll::ResolvedScroll::new(3, 10, 5),
        );
        let mut registry = crate::plugin::PluginRuntime::new();
        registry.register(OverrideScrollPlugin);
        registry.register(BuiltinInputPlugin);

        assert_eq!(
            resolve_default_scroll_policy(&mut registry, &state, candidate),
            ScrollPolicyResult::Suppress
        );
    }
}
