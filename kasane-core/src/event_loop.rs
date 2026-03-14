//! Backend-agnostic event loop helpers.
//!
//! Extracts the deferred command handling logic that is shared between
//! TUI and GUI backends.

use std::any::Any;
use std::io::Write;
use std::time::Duration;

use crate::plugin::{
    CommandResult, DeferredCommand, PluginId, PluginRegistry, execute_commands,
    extract_deferred_commands,
};
use crate::state::{AppState, DirtyFlags};
use crate::surface::SurfaceRegistry;

/// Backend-agnostic timer scheduling.
///
/// Implementations spawn a background thread that sleeps for `delay` and then
/// delivers the timer event through the backend's event system.
pub trait TimerScheduler {
    fn schedule_timer(&self, delay: Duration, target: PluginId, payload: Box<dyn Any + Send>);
}

/// Handle deferred commands (timers, inter-plugin messages, config overrides).
///
/// Returns `true` if a `Quit` command was encountered.
#[allow(clippy::too_many_arguments)]
pub fn handle_deferred_commands(
    deferred: Vec<DeferredCommand>,
    state: &mut AppState,
    registry: &mut PluginRegistry,
    surface_registry: &mut SurfaceRegistry,
    kak_writer: &mut dyn Write,
    clipboard_get: &mut dyn FnMut() -> Option<String>,
    dirty: &mut DirtyFlags,
    timer: &dyn TimerScheduler,
) -> bool {
    for cmd in deferred {
        match cmd {
            DeferredCommand::PluginMessage { target, payload } => {
                let (flags, commands) = registry.deliver_message(&target, payload, state);
                *dirty |= flags;
                let (normal, nested_deferred) = extract_deferred_commands(commands);
                if matches!(
                    execute_commands(normal, kak_writer, clipboard_get),
                    CommandResult::Quit
                ) {
                    return true;
                }
                if handle_deferred_commands(
                    nested_deferred,
                    state,
                    registry,
                    surface_registry,
                    kak_writer,
                    clipboard_get,
                    dirty,
                    timer,
                ) {
                    return true;
                }
            }
            DeferredCommand::ScheduleTimer {
                delay,
                target,
                payload,
            } => {
                timer.schedule_timer(delay, target, payload);
            }
            DeferredCommand::SetConfig { key, value } => {
                crate::state::apply_set_config(state, dirty, &key, &value);
            }
            DeferredCommand::Pane(_) => {
                // Pane commands will be handled in Phase 5a-1
            }
            DeferredCommand::Workspace(ws_cmd) => {
                crate::workspace::dispatch_workspace_command(surface_registry, ws_cmd, dirty);
            }
            DeferredCommand::RegisterThemeTokens(_tokens) => {
                // Theme token registration will be handled when Theme is
                // accessible from the event loop (Phase 1 completion).
            }
        }
    }
    false
}
