//! Built-in plugin for drag state tracking.
//!
//! Converts Press/Release mouse events into `UpdateDragState` commands,
//! allowing the framework to update `RuntimeState.drag` from plugin effects
//! rather than hardcoded logic in `update.rs`.

use crate::input::{MouseEvent, MouseEventKind};
use crate::plugin::{
    AppView, Command, MousePreDispatchResult, PluginBackend, PluginCapabilities, PluginId,
};
use crate::state::DragState;

/// Built-in plugin that tracks mouse drag state.
///
/// Converts Press→Active and Release→None into `UpdateDragState` commands,
/// allowing the framework to update `RuntimeState.drag` from plugin effects
/// rather than hardcoded logic.
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
                commands: vec![Command::UpdateDragState(DragState::Active {
                    button,
                    start_line: event.line,
                    start_column: event.column,
                })],
            },
            MouseEventKind::Release(_) => MousePreDispatchResult::Pass {
                commands: vec![Command::UpdateDragState(DragState::None)],
            },
            _ => MousePreDispatchResult::Pass { commands: vec![] },
        }
    }
}
