//! PaneMap: bidirectional SurfaceId ↔ SessionId mapping for multi-client panes.
//!
//! Each pane in a multi-pane layout is backed by an independent Kakoune client
//! connection. PaneMap tracks which surface is bound to which session, and
//! provides `PaneStates` for resolving per-pane AppState during rendering.

use std::collections::{HashMap, HashSet};

use crate::session::{SessionId, SessionStateStore};
use crate::state::AppState;

use super::SurfaceId;

/// Bidirectional mapping between workspace surfaces and Kakoune sessions.
pub struct PaneMap {
    surface_to_session: HashMap<SurfaceId, SessionId>,
    session_to_surface: HashMap<SessionId, SurfaceId>,
    server_session_name: Option<String>,
    /// Pane clients whose first Kakoune event hasn't arrived yet.
    /// These sessions need their initial Resize deferred (same race condition
    /// as the primary session — Kakoune's JSON UI may buffer stdin data
    /// during initialization and never process it without a subsequent read).
    pending_initial_resize: HashSet<SessionId>,
    /// Last Resize dimensions sent to each session.
    /// Used to suppress redundant Resize commands that would otherwise
    /// create an infinite Resize → Draw → dirty → Resize loop.
    last_resize: HashMap<SessionId, (u16, u16)>,
}

impl PaneMap {
    pub fn new() -> Self {
        PaneMap {
            surface_to_session: HashMap::new(),
            session_to_surface: HashMap::new(),
            server_session_name: None,
            pending_initial_resize: HashSet::new(),
            last_resize: HashMap::new(),
        }
    }

    /// Bind a surface to a session. Overwrites any previous binding for either side.
    pub fn bind(&mut self, surface_id: SurfaceId, session_id: SessionId) {
        // Remove stale reverse mappings
        if let Some(old_session) = self.surface_to_session.insert(surface_id, session_id) {
            self.session_to_surface.remove(&old_session);
        }
        if let Some(old_surface) = self.session_to_surface.insert(session_id, surface_id) {
            self.surface_to_session.remove(&old_surface);
        }
    }

    /// Remove a binding by surface ID. Returns the previously bound session, if any.
    pub fn unbind_surface(&mut self, surface_id: SurfaceId) -> Option<SessionId> {
        let session_id = self.surface_to_session.remove(&surface_id)?;
        self.session_to_surface.remove(&session_id);
        self.pending_initial_resize.remove(&session_id);
        self.last_resize.remove(&session_id);
        Some(session_id)
    }

    /// Remove a binding by session ID. Returns the previously bound surface, if any.
    pub fn unbind_session(&mut self, session_id: SessionId) -> Option<SurfaceId> {
        let surface_id = self.session_to_surface.remove(&session_id)?;
        self.surface_to_session.remove(&surface_id);
        self.pending_initial_resize.remove(&session_id);
        self.last_resize.remove(&session_id);
        Some(surface_id)
    }

    /// Look up the session bound to a surface.
    pub fn session_for_surface(&self, surface_id: SurfaceId) -> Option<SessionId> {
        self.surface_to_session.get(&surface_id).copied()
    }

    /// Look up the surface bound to a session.
    pub fn surface_for_session(&self, session_id: SessionId) -> Option<SurfaceId> {
        self.session_to_surface.get(&session_id).copied()
    }

    /// Get the Kakoune server session name (used for `-c` connections).
    pub fn server_session_name(&self) -> Option<&str> {
        self.server_session_name.as_deref()
    }

    /// Set the Kakoune server session name.
    pub fn set_server_session_name(&mut self, name: String) {
        self.server_session_name = Some(name);
    }

    /// Number of bound pane↔session pairs.
    pub fn len(&self) -> usize {
        self.surface_to_session.len()
    }

    /// Whether there are no bindings.
    pub fn is_empty(&self) -> bool {
        self.surface_to_session.is_empty()
    }

    /// Returns `true` if the session is a secondary pane client (not the primary buffer).
    ///
    /// The primary session (bound to `SurfaceId::BUFFER`) is **not** a pane client —
    /// its lifecycle is managed by the normal session death path, not the pane cleanup path.
    pub fn is_pane_client(&self, session_id: SessionId) -> bool {
        self.session_to_surface
            .get(&session_id)
            .is_some_and(|surface_id| *surface_id != SurfaceId::BUFFER)
    }

    /// Mark a pane client as needing its initial Resize deferred.
    ///
    /// Called when a new pane client is spawned. The Resize is deferred
    /// until the first Kakoune event from this session arrives, proving
    /// the kak process has initialized its JSON UI.
    pub fn mark_pending_resize(&mut self, session_id: SessionId) {
        self.pending_initial_resize.insert(session_id);
    }

    /// If the session has a pending initial Resize, remove it from the set
    /// and return `true`. Returns `false` if no pending resize.
    pub fn take_pending_resize(&mut self, session_id: SessionId) -> bool {
        self.pending_initial_resize.remove(&session_id)
    }

    /// Whether the session is waiting for its initial Resize.
    pub fn has_pending_resize(&self, session_id: SessionId) -> bool {
        self.pending_initial_resize.contains(&session_id)
    }

    /// Check whether the session needs a Resize with the given dimensions.
    /// Returns `true` if the dimensions differ from the last sent Resize
    /// (or if no Resize has been sent yet). Updates the cached dimensions.
    pub fn needs_resize(&mut self, session_id: SessionId, rows: u16, cols: u16) -> bool {
        let dims = (rows, cols);
        if self.last_resize.get(&session_id) == Some(&dims) {
            return false;
        }
        self.last_resize.insert(session_id, dims);
        true
    }

    /// Record that a Resize was sent to a session (for the deferred resize path).
    pub fn record_resize(&mut self, session_id: SessionId, rows: u16, cols: u16) {
        self.last_resize.insert(session_id, (rows, cols));
    }

    /// Clear cached resize dimensions for a session (e.g., on unbind).
    pub fn clear_resize_cache(&mut self, session_id: SessionId) {
        self.last_resize.remove(&session_id);
    }
}

impl Default for PaneMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Read-only accessor for resolving per-pane AppState during rendering.
///
/// The active session's state comes from the live `AppState`, while
/// inactive sessions use snapshots from `SessionStateStore`.
pub struct PaneStates<'a> {
    pane_map: &'a PaneMap,
    session_states: &'a SessionStateStore,
    active_state: &'a AppState,
    active_session: Option<SessionId>,
}

impl<'a> PaneStates<'a> {
    pub fn new(
        pane_map: &'a PaneMap,
        session_states: &'a SessionStateStore,
        active_state: &'a AppState,
        active_session: Option<SessionId>,
    ) -> Self {
        PaneStates {
            pane_map,
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
        let session_id = self.pane_map.session_for_surface(surface_id)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_and_lookup() {
        let mut map = PaneMap::new();
        let sid = SurfaceId(100);
        let session = SessionId(1);

        map.bind(sid, session);
        assert_eq!(map.session_for_surface(sid), Some(session));
        assert_eq!(map.surface_for_session(session), Some(sid));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn unbind_surface() {
        let mut map = PaneMap::new();
        let sid = SurfaceId(100);
        let session = SessionId(1);

        map.bind(sid, session);
        assert_eq!(map.unbind_surface(sid), Some(session));
        assert!(map.is_empty());
        assert_eq!(map.session_for_surface(sid), None);
        assert_eq!(map.surface_for_session(session), None);
    }

    #[test]
    fn unbind_session() {
        let mut map = PaneMap::new();
        let sid = SurfaceId(100);
        let session = SessionId(1);

        map.bind(sid, session);
        assert_eq!(map.unbind_session(session), Some(sid));
        assert!(map.is_empty());
    }

    #[test]
    fn rebind_overwrites_previous() {
        let mut map = PaneMap::new();
        let s1 = SurfaceId(100);
        let s2 = SurfaceId(200);
        let session = SessionId(1);

        map.bind(s1, session);
        map.bind(s2, session);

        // s1 should no longer be bound
        assert_eq!(map.session_for_surface(s1), None);
        assert_eq!(map.surface_for_session(session), Some(s2));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn is_pane_client_distinguishes_primary() {
        let mut map = PaneMap::new();
        let primary_session = SessionId(1);
        let pane_session = SessionId(2);

        map.bind(SurfaceId::BUFFER, primary_session);
        map.bind(SurfaceId(SurfaceId::PLUGIN_BASE), pane_session);

        assert!(!map.is_pane_client(primary_session));
        assert!(map.is_pane_client(pane_session));
        // Unknown session is not a pane client
        assert!(!map.is_pane_client(SessionId(99)));
    }

    #[test]
    fn server_session_name() {
        let mut map = PaneMap::new();
        assert!(map.server_session_name().is_none());

        map.set_server_session_name("kasane-1234".to_string());
        assert_eq!(map.server_session_name(), Some("kasane-1234"));
    }

    #[test]
    fn pending_initial_resize() {
        let mut map = PaneMap::new();
        let session = SessionId(1);

        assert!(!map.has_pending_resize(session));
        assert!(!map.take_pending_resize(session));

        map.mark_pending_resize(session);
        assert!(map.has_pending_resize(session));

        // First take succeeds
        assert!(map.take_pending_resize(session));
        // Second take is a no-op
        assert!(!map.has_pending_resize(session));
        assert!(!map.take_pending_resize(session));
    }
}
