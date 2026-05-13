use crate::input::{KeyEvent, KeyResponse};
use crate::state::{self, DirtyFlags};

use super::Command;
use super::effect::effects::StateUpdates;

/// Result of key middleware dispatch.
#[derive(Default)]
pub enum KeyHandleResult {
    Consumed(Vec<Command>),
    Transformed(KeyEvent),
    #[default]
    Passthrough,
}

impl From<KeyResponse> for KeyHandleResult {
    fn from(response: KeyResponse) -> Self {
        match response {
            KeyResponse::Pass => KeyHandleResult::Passthrough,
            KeyResponse::Consume => KeyHandleResult::Consumed(vec![]),
            KeyResponse::ConsumeRedraw => {
                KeyHandleResult::Consumed(vec![Command::RequestRedraw(state::DirtyFlags::ALL)])
            }
            KeyResponse::ConsumeWith(commands) => KeyHandleResult::Consumed(commands),
        }
    }
}

/// Result of key pre-dispatch (before the middleware chain).
///
/// Pre-dispatch handlers run before `observe_key_all` and `dispatch_key_middleware`.
/// They are used for features like the shadow cursor that need to intercept keys
/// before any other plugin sees them.
///
/// `Cmd` parameter selects the command tier: defaults to [`Command`] (the
/// full set); aliased as [`KakouneSideKeyPreDispatchResult`] when the
/// handler is narrowed to ADR-044 Tier 1 (Kakoune-side-only, no process
/// spawn). The asymmetric `From` impl widens tier-1 → full via the
/// existing `KakouneSideCommand → Command` lift; no reverse impl exists
/// (you cannot widen a process-spawning command into the tier-1 subset).
pub enum KeyPreDispatchResult<Cmd = Command> {
    /// Key was consumed by the pre-dispatch handler.
    Consumed {
        flags: DirtyFlags,
        commands: Vec<Cmd>,
        state_updates: StateUpdates,
        /// Algebraic shadow-cursor commit pending intercept dispatch.
        /// Producers (e.g. `BuiltinShadowCursorPlugin`) populate this
        /// in lieu of pre-serialized commands; the dispatch loop
        /// (`PluginRuntime::dispatch_key_pre_dispatch`) runs
        /// `on_buffer_edit_intercept` across registered plugins,
        /// folds `BufferEditVerdict::Replace` / `Veto`, and serializes
        /// the final edit (if any) via
        /// `state::shadow_cursor::edit_to_commands` into `commands`
        /// before returning. Most pre-dispatch consumers leave this
        /// `None` — only the shadow-cursor builtin uses it.
        pending_buffer_edit: Option<crate::state::shadow_cursor::BufferEdit>,
    },
    /// Pass through to normal key dispatch. Commands (if any) are applied first.
    /// This allows pre-dispatch handlers to update state (e.g., deactivate shadow cursor)
    /// while still letting the key proceed through normal dispatch.
    Pass {
        commands: Vec<Cmd>,
        state_updates: StateUpdates,
    },
}

/// Tier-1 alias of [`KeyPreDispatchResult`]. Same shape, but `commands`
/// is `Vec<KakouneSideCommand>` so process spawn variants are rejected
/// at compile time. `pending_buffer_edit` is preserved verbatim — the
/// algebraic shadow-cursor commit is orthogonal to the tier hierarchy
/// (the dispatch loop later folds it into Kakoune-side commands via
/// `state::shadow_cursor::edit_to_commands`).
pub type KakouneSideKeyPreDispatchResult = KeyPreDispatchResult<super::KakouneSideCommand>;

impl From<KakouneSideKeyPreDispatchResult> for KeyPreDispatchResult {
    fn from(tier1: KakouneSideKeyPreDispatchResult) -> Self {
        match tier1 {
            KeyPreDispatchResult::Consumed {
                flags,
                commands,
                state_updates,
                pending_buffer_edit,
            } => KeyPreDispatchResult::Consumed {
                flags,
                commands: commands.into_iter().map(Into::into).collect(),
                state_updates,
                pending_buffer_edit,
            },
            KeyPreDispatchResult::Pass {
                commands,
                state_updates,
            } => KeyPreDispatchResult::Pass {
                commands: commands.into_iter().map(Into::into).collect(),
                state_updates,
            },
        }
    }
}

/// Result of mouse pre-dispatch (before observation and hit-test dispatch).
///
/// Pre-dispatch handlers run before `observe_mouse_all` and `dispatch_mouse_handler`.
/// They are used for features like drag state tracking and shadow cursor deactivation
/// that need to intercept mouse events before any other plugin sees them.
///
/// `Cmd` parameter selects the command tier; see [`KeyPreDispatchResult`]
/// for the ADR-044 tier-typing rationale.
pub enum MousePreDispatchResult<Cmd = Command> {
    /// Mouse event was consumed by the pre-dispatch handler.
    Consumed {
        flags: DirtyFlags,
        commands: Vec<Cmd>,
        state_updates: StateUpdates,
    },
    /// Pass through to normal mouse dispatch. Commands (if any) are applied first.
    Pass {
        commands: Vec<Cmd>,
        state_updates: StateUpdates,
    },
}

/// Tier-1 alias of [`MousePreDispatchResult`].
///
/// Pre-dispatch handlers fire on every mouse tick (move included), so
/// the tier-1 narrowing is the natural enforcement against the
/// silent-spawn class of bugs that motivated ADR-044.
pub type KakouneSideMousePreDispatchResult = MousePreDispatchResult<super::KakouneSideCommand>;

impl From<KakouneSideMousePreDispatchResult> for MousePreDispatchResult {
    fn from(tier1: KakouneSideMousePreDispatchResult) -> Self {
        match tier1 {
            MousePreDispatchResult::Consumed {
                flags,
                commands,
                state_updates,
            } => MousePreDispatchResult::Consumed {
                flags,
                commands: commands.into_iter().map(Into::into).collect(),
                state_updates,
            },
            MousePreDispatchResult::Pass {
                commands,
                state_updates,
            } => MousePreDispatchResult::Pass {
                commands: commands.into_iter().map(Into::into).collect(),
                state_updates,
            },
        }
    }
}

/// Result of text input pre-dispatch (before the text input handler chain).
///
/// `Cmd` parameter selects the command tier; see [`KeyPreDispatchResult`]
/// for the ADR-044 tier-typing rationale. The `Pass` variant carries no
/// payload (no commands path) and is identical across tiers.
pub enum TextInputPreDispatchResult<Cmd = Command> {
    /// Text input was consumed by the pre-dispatch handler.
    Consumed {
        flags: DirtyFlags,
        commands: Vec<Cmd>,
        state_updates: StateUpdates,
    },
    /// Pass through to normal text input dispatch.
    Pass,
}

/// Tier-1 alias of [`TextInputPreDispatchResult`].
pub type KakouneSideTextInputPreDispatchResult =
    TextInputPreDispatchResult<super::KakouneSideCommand>;

impl From<KakouneSideTextInputPreDispatchResult> for TextInputPreDispatchResult {
    fn from(tier1: KakouneSideTextInputPreDispatchResult) -> Self {
        match tier1 {
            TextInputPreDispatchResult::Consumed {
                flags,
                commands,
                state_updates,
            } => TextInputPreDispatchResult::Consumed {
                flags,
                commands: commands.into_iter().map(Into::into).collect(),
                state_updates,
            },
            TextInputPreDispatchResult::Pass => TextInputPreDispatchResult::Pass,
        }
    }
}
