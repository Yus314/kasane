//! Kakoune-transparent effects projection.
//!
//! `KakouneTransparentEffects` is the Level 5 enforcement of ADR-030.
//! Where `KakouneTransparentCommand` restricts construction to non-writing variants,
//! `KakouneTransparentEffects` restricts an entire `Effects` return value to contain
//! only transparent commands. A handler returning `(S, KakouneTransparentEffects)`
//! provides a compile-time witness that it cannot emit Kakoune-writing effects.

use crate::scroll::ScrollPlan;
use crate::state::DirtyFlags;

use super::super::Effects;
use super::kakoune_transparent_command::KakouneTransparentCommand;

/// An effects value guaranteed not to contain Kakoune-writing commands.
///
/// Construction is restricted: commands can only be added via
/// [`KakouneTransparentCommand`], which statically excludes `SendToKakoune`,
/// `InsertText`, and `EditBuffer`. Converts to [`Effects`] before the
/// type erasure boundary in `HandlerTable`.
pub struct KakouneTransparentEffects {
    redraw: DirtyFlags,
    commands: Vec<KakouneTransparentCommand>,
    scroll_plans: Vec<ScrollPlan>,
}

impl std::fmt::Debug for KakouneTransparentEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KakouneTransparentEffects")
            .field("redraw", &self.redraw)
            .field("commands", &self.commands)
            .field("scroll_plans_len", &self.scroll_plans.len())
            .finish()
    }
}

impl Default for KakouneTransparentEffects {
    fn default() -> Self {
        Self {
            redraw: DirtyFlags::empty(),
            commands: Vec::new(),
            scroll_plans: Vec::new(),
        }
    }
}

impl KakouneTransparentEffects {
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
    pub fn with(commands: Vec<KakouneTransparentCommand>) -> Self {
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
    pub fn push(&mut self, cmd: KakouneTransparentCommand) {
        self.commands.push(cmd);
    }

    /// Add a scroll plan.
    pub fn push_scroll(&mut self, plan: ScrollPlan) {
        self.scroll_plans.push(plan);
    }
}

impl From<KakouneTransparentEffects> for Effects {
    fn from(te: KakouneTransparentEffects) -> Self {
        Effects {
            redraw: te.redraw,
            commands: te.commands.into_iter().map(Into::into).collect(),
            scroll_plans: te.scroll_plans,
            state_updates: super::super::StateUpdates::default(),
        }
    }
}
