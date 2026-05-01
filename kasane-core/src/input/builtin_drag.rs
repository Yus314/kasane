//! Built-in plugin for drag state tracking.
//!
//! Converts Press/Release mouse events into `Effects::state_updates.drag`
//! mutations (R4 typed channel), allowing the framework to update
//! `RuntimeState.drag` from plugin effects rather than hardcoded logic in
//! `update.rs`.

use crate::input::{MouseEvent, MouseEventKind};
use crate::plugin::{AppView, MousePreDispatchResult, PluginBackend, PluginCapabilities, PluginId};
use crate::state::DragState;

/// Built-in plugin that tracks mouse drag state.
///
/// Converts Press→Active and Release→None into `state_updates.drag`
/// mutations on the pre-dispatch result, letting the framework apply the
/// new drag state without an out-of-band side channel.
pub struct BuiltinDragPlugin;

impl PluginBackend for BuiltinDragPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.drag".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::MOUSE_PRE_DISPATCH
    }

    fn handle_mouse_pre_dispatch(
        &mut self,
        event: &MouseEvent,
        _state: &AppView<'_>,
    ) -> MousePreDispatchResult {
        match event.kind {
            MouseEventKind::Press(button) => MousePreDispatchResult::Pass {
                commands: vec![],
                state_updates: crate::plugin::StateUpdates {
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
                state_updates: crate::plugin::StateUpdates {
                    drag: Some(DragState::None),
                    ..Default::default()
                },
            },
            _ => MousePreDispatchResult::Pass {
                commands: vec![],
                state_updates: crate::plugin::StateUpdates::default(),
            },
        }
    }
}
