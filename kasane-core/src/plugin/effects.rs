use std::any::Any;

use crate::element::InteractiveId;
use crate::input::{KeyEvent, MouseEvent};
use crate::protocol::KasaneRequest;
use crate::scroll::{DefaultScrollCandidate, ScrollPlan, ScrollPolicyResult};
use crate::state::DirtyFlags;

use super::{AppView, Command, KeyDispatchResult, PluginId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapEffects {
    pub redraw: DirtyFlags,
}

impl Default for BootstrapEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
        }
    }
}

impl BootstrapEffects {
    pub fn merge(&mut self, other: Self) {
        self.redraw |= other.redraw;
    }
}

pub struct SessionReadyEffects {
    pub redraw: DirtyFlags,
    pub commands: Vec<SessionReadyCommand>,
    pub scroll_plans: Vec<ScrollPlan>,
}

impl Default for SessionReadyEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
            commands: Vec::new(),
            scroll_plans: Vec::new(),
        }
    }
}

impl SessionReadyEffects {
    pub fn merge(&mut self, mut other: Self) {
        self.redraw |= other.redraw;
        self.commands.append(&mut other.commands);
        self.scroll_plans.append(&mut other.scroll_plans);
    }
}

pub enum SessionReadyCommand {
    SendToKakoune(KasaneRequest),
    Paste,
    PluginMessage {
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
}

#[derive(Default)]
pub struct InitBatch {
    pub effects: BootstrapEffects,
}

#[derive(Default)]
pub struct ReadyBatch {
    pub effects: SessionReadyEffects,
}

pub struct RuntimeEffects {
    pub redraw: DirtyFlags,
    pub commands: Vec<Command>,
    pub scroll_plans: Vec<ScrollPlan>,
}

impl Default for RuntimeEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
            commands: Vec::new(),
            scroll_plans: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct RuntimeBatch {
    pub effects: RuntimeEffects,
}

impl RuntimeEffects {
    pub fn merge(&mut self, mut other: Self) {
        self.redraw |= other.redraw;
        self.commands.append(&mut other.commands);
        self.scroll_plans.append(&mut other.scroll_plans);
    }
}

/// Result of first-wins mouse dispatch across plugins.
pub enum MouseHandleResult {
    Handled {
        source_plugin: PluginId,
        commands: Vec<Command>,
    },
    NotHandled,
}

/// Minimal interface for plugin effects consumed by `update()`.
///
/// Parametrizes the TEA update function over the plugin system,
/// enabling isolated testing with mock implementations.
pub trait PluginEffects {
    /// Notify plugins of state changes and collect batched effects.
    fn notify_state_changed(&mut self, app: &AppView<'_>, flags: DirtyFlags) -> RuntimeBatch;

    /// Broadcast key observation to all plugins (cannot consume).
    fn observe_key_all(&mut self, key: &KeyEvent, app: &AppView<'_>);

    /// Run the key middleware chain (first-wins dispatch).
    fn dispatch_key_middleware(&mut self, key: &KeyEvent, app: &AppView<'_>) -> KeyDispatchResult;

    /// Broadcast mouse observation to all plugins (cannot consume).
    fn observe_mouse_all(&mut self, event: &MouseEvent, app: &AppView<'_>);

    /// Run first-wins mouse handler dispatch via hit-test id.
    fn dispatch_mouse_handler(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult;

    /// Resolve default scroll policy for a scroll candidate.
    fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        app: &AppView<'_>,
    ) -> Option<ScrollPolicyResult>;
}

/// No-op implementation — all observations are discarded, all dispatches pass through.
pub struct NullEffects;

impl PluginEffects for NullEffects {
    fn notify_state_changed(&mut self, _: &AppView<'_>, _: DirtyFlags) -> RuntimeBatch {
        RuntimeBatch::default()
    }
    fn observe_key_all(&mut self, _: &KeyEvent, _: &AppView<'_>) {}
    fn dispatch_key_middleware(&mut self, key: &KeyEvent, _: &AppView<'_>) -> KeyDispatchResult {
        KeyDispatchResult::Passthrough(key.clone())
    }
    fn observe_mouse_all(&mut self, _: &MouseEvent, _: &AppView<'_>) {}
    fn dispatch_mouse_handler(
        &mut self,
        _: &MouseEvent,
        _: InteractiveId,
        _: &AppView<'_>,
    ) -> MouseHandleResult {
        MouseHandleResult::NotHandled
    }
    fn handle_default_scroll(
        &mut self,
        _: DefaultScrollCandidate,
        _: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        None
    }
}

/// Records all effect invocations for test assertions.
#[derive(Default)]
pub struct RecordingEffects {
    pub key_observations: Vec<KeyEvent>,
    pub mouse_observations: Vec<MouseEvent>,
    pub key_dispatches: Vec<KeyEvent>,
    pub mouse_dispatches: Vec<(MouseEvent, InteractiveId)>,
    pub state_notifications: Vec<DirtyFlags>,
}

impl PluginEffects for RecordingEffects {
    fn notify_state_changed(&mut self, _: &AppView<'_>, flags: DirtyFlags) -> RuntimeBatch {
        self.state_notifications.push(flags);
        RuntimeBatch::default()
    }
    fn observe_key_all(&mut self, key: &KeyEvent, _: &AppView<'_>) {
        self.key_observations.push(key.clone());
    }
    fn dispatch_key_middleware(&mut self, key: &KeyEvent, _: &AppView<'_>) -> KeyDispatchResult {
        self.key_dispatches.push(key.clone());
        KeyDispatchResult::Passthrough(key.clone())
    }
    fn observe_mouse_all(&mut self, event: &MouseEvent, _: &AppView<'_>) {
        self.mouse_observations.push(event.clone());
    }
    fn dispatch_mouse_handler(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        _: &AppView<'_>,
    ) -> MouseHandleResult {
        self.mouse_dispatches.push((event.clone(), id));
        MouseHandleResult::NotHandled
    }
    fn handle_default_scroll(
        &mut self,
        _: DefaultScrollCandidate,
        _: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        None
    }
}
