//! Built-in universal reveal toggle plugin (RFC-107a).
//!
//! Binds `<a-r>` to `Command::ToggleUniversalReveal`, which flips
//! `state.config.universal_reveal_state`. When enabled, all destructive
//! display directives (`Hide`, `HideInline`) are filtered out pre-algebra
//! in `collect_tagged_display_directives`, providing §10.2a-faithful
//! recovery for every plugin's destructive directives via a single
//! host-owned key — analogous to `FoldToggleState` for `Fold`.
//!
//! Registered as a low-priority builtin so user plugins can override
//! `<a-r>` if they want a different reveal binding.

use crate::input::{Key, Modifiers};
use crate::plugin::{Command, HandlerRegistry, PluginId, StatelessPlugin};

/// Built-in plugin: `<a-r>` → toggle universal reveal of destructive
/// display directives.
pub struct BuiltinRevealPlugin;

impl StatelessPlugin for BuiltinRevealPlugin {
    fn id(&self) -> PluginId {
        PluginId::from("kasane.builtin.reveal")
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_key(|_state, key, _app| {
            if key.key == Key::Char('r') && key.modifiers == Modifiers::ALT {
                Some(((), vec![Command::ToggleUniversalReveal]))
            } else {
                None
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::KeyEvent;
    use crate::plugin::{AppView, PluginBridge};
    use crate::state::AppState;

    #[test]
    fn alt_r_emits_toggle_universal_reveal() {
        let mut plugin = PluginBridge::new(BuiltinRevealPlugin);
        let state = AppState::default();
        let view = AppView::new(&state);
        let key = KeyEvent {
            key: Key::Char('r'),
            modifiers: Modifiers::ALT,
        };
        let result = plugin.handle_key(&key, &view);
        assert!(matches!(
            result.as_deref(),
            Some([Command::ToggleUniversalReveal])
        ));
    }

    #[test]
    fn plain_r_is_ignored() {
        let mut plugin = PluginBridge::new(BuiltinRevealPlugin);
        let state = AppState::default();
        let view = AppView::new(&state);
        let key = KeyEvent {
            key: Key::Char('r'),
            modifiers: Modifiers::empty(),
        };
        assert!(plugin.handle_key(&key, &view).is_none());
    }

    #[test]
    fn ctrl_r_is_ignored() {
        let mut plugin = PluginBridge::new(BuiltinRevealPlugin);
        let state = AppState::default();
        let view = AppView::new(&state);
        let key = KeyEvent {
            key: Key::Char('r'),
            modifiers: Modifiers::CTRL,
        };
        assert!(plugin.handle_key(&key, &view).is_none());
    }
}
