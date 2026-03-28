use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use crate::display::unit::DisplayUnit;
use crate::element::InteractiveId;
use crate::input::{KeyEvent, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPlan, ScrollPolicyResult};
use crate::state::DirtyFlags;

use super::{AppView, Command, KeyDispatchResult, PluginId};

/// Lifecycle phase for effect validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecyclePhase {
    /// Plugin initialization. Only `RequestRedraw` is allowed.
    Bootstrap,
    /// Active session ready. `SendToKakoune`, `Paste`, `PluginMessage`,
    /// `RequestRedraw`, and scroll plans are allowed.
    SessionReady,
    /// Full runtime. All commands allowed.
    Runtime,
}

/// Unified plugin effects type used across all lifecycle phases.
///
/// Replaces the previous `BootstrapEffects` / `SessionReadyEffects` /
/// `RuntimeEffects` trio. Framework validates command legality per phase
/// via [`Effects::validate`].
pub struct Effects {
    pub redraw: DirtyFlags,
    pub commands: Vec<Command>,
    pub scroll_plans: Vec<ScrollPlan>,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
            commands: Vec::new(),
            scroll_plans: Vec::new(),
        }
    }
}

impl Effects {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn redraw(flags: DirtyFlags) -> Self {
        Self {
            redraw: flags,
            ..Self::default()
        }
    }

    pub fn with(commands: Vec<Command>) -> Self {
        Self {
            commands,
            ..Self::default()
        }
    }

    pub fn merge(&mut self, mut other: Self) {
        self.redraw |= other.redraw;
        self.commands.append(&mut other.commands);
        self.scroll_plans.append(&mut other.scroll_plans);
    }

    /// Validate and filter commands for the given lifecycle phase.
    ///
    /// - **Bootstrap**: only `RequestRedraw`; no scroll plans.
    /// - **SessionReady**: `SendToKakoune`, `Paste`, `PluginMessage`, `RequestRedraw`; scroll plans allowed.
    /// - **Runtime**: all commands and scroll plans allowed.
    ///
    /// Debug builds panic on illegal commands; release builds warn and drop them.
    pub fn validate(mut self, phase: LifecyclePhase) -> Self {
        match phase {
            LifecyclePhase::Runtime => self,
            LifecyclePhase::Bootstrap => {
                if !self.commands.is_empty() {
                    let before = self.commands.len();
                    self.commands
                        .retain(|cmd| matches!(cmd, Command::RequestRedraw(_)));
                    let dropped = before - self.commands.len();
                    if dropped > 0 {
                        debug_assert!(
                            false,
                            "Bootstrap phase received {dropped} illegal command(s); \
                             only RequestRedraw is allowed"
                        );
                        tracing::warn!(
                            count = dropped,
                            "Bootstrap phase: dropping illegal commands"
                        );
                    }
                }
                if !self.scroll_plans.is_empty() {
                    debug_assert!(false, "Bootstrap phase does not allow scroll plans");
                    tracing::warn!("Bootstrap phase: dropping scroll plans");
                    self.scroll_plans.clear();
                }
                self
            }
            LifecyclePhase::SessionReady => {
                let before = self.commands.len();
                self.commands.retain(|cmd| {
                    matches!(
                        cmd,
                        Command::SendToKakoune(_)
                            | Command::Paste
                            | Command::PluginMessage { .. }
                            | Command::RequestRedraw(_)
                    )
                });
                let dropped = before - self.commands.len();
                if dropped > 0 {
                    debug_assert!(
                        false,
                        "SessionReady phase received {dropped} illegal command(s)"
                    );
                    tracing::warn!(
                        count = dropped,
                        "SessionReady phase: dropping illegal commands"
                    );
                }
                self
            }
        }
    }
}

/// Aggregated effects batch from multiple plugins.
#[derive(Default)]
pub struct EffectsBatch {
    pub effects: Effects,
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
    fn notify_state_changed(&mut self, app: &AppView<'_>, flags: DirtyFlags) -> EffectsBatch;

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

    /// Resolve navigation policy for a display unit via plugin dispatch.
    fn resolve_navigation_policy(&self, unit: &DisplayUnit) -> NavigationPolicy;

    /// Dispatch a navigation action through the plugin chain.
    fn dispatch_navigation_action(
        &mut self,
        unit: &DisplayUnit,
        action: NavigationAction,
    ) -> ActionResult;
}

/// No-op implementation — all observations are discarded, all dispatches pass through.
pub struct NullEffects;

impl PluginEffects for NullEffects {
    fn notify_state_changed(&mut self, _: &AppView<'_>, _: DirtyFlags) -> EffectsBatch {
        EffectsBatch::default()
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
    fn resolve_navigation_policy(&self, unit: &DisplayUnit) -> NavigationPolicy {
        NavigationPolicy::default_for(&unit.role)
    }
    fn dispatch_navigation_action(&mut self, _: &DisplayUnit, _: NavigationAction) -> ActionResult {
        ActionResult::Pass
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
    pub navigation_policy_queries: Vec<DisplayUnit>,
    pub navigation_action_dispatches: Vec<(DisplayUnit, NavigationAction)>,
}

impl PluginEffects for RecordingEffects {
    fn notify_state_changed(&mut self, _: &AppView<'_>, flags: DirtyFlags) -> EffectsBatch {
        self.state_notifications.push(flags);
        EffectsBatch::default()
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
    fn resolve_navigation_policy(&self, unit: &DisplayUnit) -> NavigationPolicy {
        NavigationPolicy::default_for(&unit.role)
    }
    fn dispatch_navigation_action(
        &mut self,
        unit: &DisplayUnit,
        action: NavigationAction,
    ) -> ActionResult {
        self.navigation_action_dispatches
            .push((unit.clone(), action));
        ActionResult::Pass
    }
}
