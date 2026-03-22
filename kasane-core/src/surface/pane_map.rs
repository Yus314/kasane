//! Per-pane AppState resolution for multi-client rendering.
//!
//! In multi-pane mode each workspace surface can be bound to an independent
//! Kakoune client session (managed by `SurfaceRegistry`). `PaneStates`
//! resolves the correct `AppState` for each surface during rendering:
//! the live state for the active session, or a snapshot for inactive ones.

use crate::session::{SessionId, SessionStateStore};
use crate::state::AppState;

use super::SurfaceId;
use super::registry::SurfaceRegistry;

/// Read-only accessor for resolving per-pane AppState during rendering.
///
/// The active session's state comes from the live `AppState`, while
/// inactive sessions use snapshots from `SessionStateStore`.
pub struct PaneStates<'a> {
    surface_registry: &'a SurfaceRegistry,
    session_states: &'a SessionStateStore,
    active_state: &'a AppState,
    active_session: Option<SessionId>,
}

impl<'a> PaneStates<'a> {
    pub fn from_registry(
        surface_registry: &'a SurfaceRegistry,
        session_states: &'a SessionStateStore,
        active_state: &'a AppState,
        active_session: Option<SessionId>,
    ) -> Self {
        PaneStates {
            surface_registry,
            session_states,
            active_state,
            active_session,
        }
    }

    /// Resolve the AppState for a given surface.
    ///
    /// Returns the live state for the active session, or a snapshot for
    /// inactive sessions. Returns `None` if the surface has no session binding.
    pub fn state_for_surface(&self, surface_id: SurfaceId) -> Option<&'a AppState> {
        let session_id = self.surface_registry.session_for_surface(surface_id)?;
        if Some(session_id) == self.active_session {
            Some(self.active_state)
        } else {
            self.session_states.get(&session_id)
        }
    }

    /// Resolve the AppState for a given surface, falling back to the focused
    /// pane's state for surfaces not bound to any session (e.g., STATUS).
    ///
    /// In multi-pane mode, shared surfaces like the status bar should reflect
    /// the focused pane's content, not the primary session's.
    pub fn state_for_surface_or_focused(
        &self,
        surface_id: SurfaceId,
        focused_surface: SurfaceId,
    ) -> Option<&'a AppState> {
        self.state_for_surface(surface_id)
            .or_else(|| self.state_for_surface(focused_surface))
    }
}
