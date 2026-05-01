//! Kakoune-transparent effects projection.
//!
//! `KakouneSafeEffects` is the Level 5 enforcement of ADR-030.
//! Where `KakouneSafeCommand` restricts construction to non-writing variants,
//! `KakouneSafeEffects` restricts an entire `Effects` return value to contain
//! only transparent commands. A handler returning `(S, KakouneSafeEffects)`
//! provides a compile-time witness that it cannot emit Kakoune-writing effects.

use crate::scroll::ScrollPlan;
use crate::state::DirtyFlags;

use super::Effects;
use super::kakoune_safe_command::KakouneSafeCommand;

/// An effects value guaranteed not to contain Kakoune-writing commands.
///
/// Construction is restricted: commands can only be added via
/// [`KakouneSafeCommand`], which statically excludes `SendToKakoune`,
/// `InsertText`, and `EditBuffer`. Converts to [`Effects`] before the
/// type erasure boundary in `HandlerTable`.
pub struct KakouneSafeEffects {
    redraw: DirtyFlags,
    commands: Vec<KakouneSafeCommand>,
    scroll_plans: Vec<ScrollPlan>,
}

impl std::fmt::Debug for KakouneSafeEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KakouneSafeEffects")
            .field("redraw", &self.redraw)
            .field("commands", &self.commands)
            .field("scroll_plans_len", &self.scroll_plans.len())
            .finish()
    }
}

impl Default for KakouneSafeEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
            commands: Vec::new(),
            scroll_plans: Vec::new(),
        }
    }
}

impl KakouneSafeEffects {
    /// No effects.
    pub fn none() -> Self {
        Self::default()
    }

    /// Redraw-only effects.
    pub fn redraw(flags: DirtyFlags) -> Self {
        Self {
            redraw: flags,
            ..Self::default()
        }
    }

    /// Effects with transparent commands.
    pub fn with(commands: Vec<KakouneSafeCommand>) -> Self {
        Self {
            commands,
            ..Self::default()
        }
    }

    /// Set redraw flags.
    pub fn set_redraw(&mut self, flags: DirtyFlags) {
        self.redraw |= flags;
    }

    /// Add a transparent command.
    pub fn push(&mut self, cmd: KakouneSafeCommand) {
        self.commands.push(cmd);
    }

    /// Add a scroll plan.
    pub fn push_scroll(&mut self, plan: ScrollPlan) {
        self.scroll_plans.push(plan);
    }
}

impl From<KakouneSafeEffects> for Effects {
    fn from(te: KakouneSafeEffects) -> Self {
        Effects {
            redraw: te.redraw,
            commands: te.commands.into_iter().map(Into::into).collect(),
            scroll_plans: te.scroll_plans,
        }
    }
}
