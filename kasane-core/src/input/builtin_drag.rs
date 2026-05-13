//! Built-in plugin for drag state tracking.
//!
//! Converts Press/Release mouse events into `Effects::state_updates.drag`
//! mutations (R4 typed channel), allowing the framework to update
//! `RuntimeState.drag` from plugin effects rather than hardcoded logic in
//! `update.rs`.

use crate::input::MouseEventKind;
use crate::plugin::{
    HandlerRegistry, MousePreDispatchResult, PluginId, StateUpdates, StatelessPlugin,
};
use crate::state::DragState;

/// Built-in plugin that tracks mouse drag state.
///
/// Converts Press→Active and Release→None into `state_updates.drag`
/// mutations on the pre-dispatch result, letting the framework apply the
/// new drag state without an out-of-band side channel.
pub struct BuiltinDragPlugin;

impl StatelessPlugin for BuiltinDragPlugin {
    fn id(&self) -> PluginId {
        PluginId::from("kasane.builtin.drag")
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_mouse_pre_dispatch(|_state, event, _app| {
            let result = match event.kind {
                MouseEventKind::Press(button) => MousePreDispatchResult::Pass {
                    commands: vec![],
                    state_updates: StateUpdates {
                        drag: Some(DragState::Active {
                            button,
                            start_line: event.line,
                            start_column: event.column,
                        }),
                        ..Default::default()
                    },
                },
                MouseEventKind::Release(_) => MousePreDispatchResult::Pass {
                    commands: vec![],
                    state_updates: StateUpdates {
                        drag: Some(DragState::None),
                        ..Default::default()
                    },
                },
                _ => MousePreDispatchResult::Pass {
                    commands: vec![],
                    state_updates: StateUpdates::default(),
                },
            };
            ((), result)
        });
    }
}
