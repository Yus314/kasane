//! Session binding and resize tracking.

use super::*;

impl SurfaceRegistry {
    /// Bind a surface to a Kakoune session. Overwrites any previous binding
    /// for either side (same semantics as the former `PaneMap::bind`).
    pub fn bind_session(&mut self, surface_id: SurfaceId, session_id: SessionId) {
        // Remove stale reverse binding for this session
        if let Some(old_surface) = self.session_to_surface.insert(session_id, surface_id)
            && old_surface != surface_id
            && let Some(entry) = self.surfaces.get_mut(&old_surface)
        {
            entry.session_binding = None;
        }
        // Remove stale forward binding for this surface
        if let Some(entry) = self.surfaces.get(&surface_id)
            && let Some(old_binding) = &entry.session_binding
            && old_binding.session_id != session_id
        {
            self.session_to_surface.remove(&old_binding.session_id);
        }
        if let Some(entry) = self.surfaces.get_mut(&surface_id) {
            entry.session_binding = Some(super::super::types::SessionBindingState {
                session_id,
                pending_initial_resize: false,
                last_resize: None,
            });
        }
    }

    /// Remove a session binding by surface ID. Returns the previously bound session.
    pub fn unbind_session_by_surface(&mut self, surface_id: SurfaceId) -> Option<SessionId> {
        let entry = self.surfaces.get_mut(&surface_id)?;
        let binding = entry.session_binding.take()?;
        self.session_to_surface.remove(&binding.session_id);
        Some(binding.session_id)
    }

    /// Remove a session binding by session ID. Returns the previously bound surface.
    pub fn unbind_session_by_session(&mut self, session_id: SessionId) -> Option<SurfaceId> {
        let surface_id = self.session_to_surface.remove(&session_id)?;
        if let Some(entry) = self.surfaces.get_mut(&surface_id) {
            entry.session_binding = None;
        }
        Some(surface_id)
    }

    /// Look up the session bound to a surface.
    pub fn session_for_surface(&self, surface_id: SurfaceId) -> Option<SessionId> {
        self.surfaces
            .get(&surface_id)
            .and_then(|e| e.session_binding.as_ref())
            .map(|b| b.session_id)
    }

    /// Look up the surface bound to a session.
    pub fn surface_for_session(&self, session_id: SessionId) -> Option<SurfaceId> {
        self.session_to_surface.get(&session_id).copied()
    }

    /// Returns `true` if the session is a secondary pane client (not the primary buffer).
    pub fn is_pane_client(&self, session_id: SessionId) -> bool {
        self.session_to_surface
            .get(&session_id)
            .is_some_and(|surface_id| *surface_id != SurfaceId::BUFFER)
    }

    /// Returns `true` when more than one surface has a session binding.
    pub fn is_multi_pane(&self) -> bool {
        self.session_to_surface.len() > 1
    }

    /// Mark a pane client as needing its initial Resize deferred.
    pub fn mark_pending_resize(&mut self, session_id: SessionId) {
        if let Some(surface_id) = self.session_to_surface.get(&session_id)
            && let Some(entry) = self.surfaces.get_mut(surface_id)
            && let Some(binding) = &mut entry.session_binding
        {
            binding.pending_initial_resize = true;
        }
    }

    /// If the session has a pending initial Resize, clear the flag and return `true`.
    pub fn take_pending_resize(&mut self, session_id: SessionId) -> bool {
        if let Some(surface_id) = self.session_to_surface.get(&session_id)
            && let Some(entry) = self.surfaces.get_mut(surface_id)
            && let Some(binding) = &mut entry.session_binding
            && binding.pending_initial_resize
        {
            binding.pending_initial_resize = false;
            return true;
        }
        false
    }

    /// Whether the session is waiting for its initial Resize.
    pub fn has_pending_resize(&self, session_id: SessionId) -> bool {
        self.session_to_surface
            .get(&session_id)
            .and_then(|sid| self.surfaces.get(sid))
            .and_then(|e| e.session_binding.as_ref())
            .is_some_and(|b| b.pending_initial_resize)
    }

    /// Check whether the session needs a Resize with the given dimensions.
    /// Returns `true` if the dimensions differ from the last sent Resize.
    /// Updates the cached dimensions.
    pub fn needs_resize(&mut self, session_id: SessionId, rows: u16, cols: u16) -> bool {
        let dims = (rows, cols);
        if let Some(surface_id) = self.session_to_surface.get(&session_id)
            && let Some(entry) = self.surfaces.get_mut(surface_id)
            && let Some(binding) = &mut entry.session_binding
        {
            if binding.last_resize == Some(dims) {
                return false;
            }
            binding.last_resize = Some(dims);
            return true;
        }
        false
    }

    /// Record that a Resize was sent to a session (for the deferred resize path).
    pub fn record_resize(&mut self, session_id: SessionId, rows: u16, cols: u16) {
        if let Some(surface_id) = self.session_to_surface.get(&session_id)
            && let Some(entry) = self.surfaces.get_mut(surface_id)
            && let Some(binding) = &mut entry.session_binding
        {
            binding.last_resize = Some((rows, cols));
        }
    }

    /// Get the Kakoune server session name (used for `-c` connections).
    pub fn server_session_name(&self) -> Option<&str> {
        self.server_session_name.as_deref()
    }

    /// Set the Kakoune server session name.
    pub fn set_server_session_name(&mut self, name: String) {
        self.server_session_name = Some(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionId;
    use crate::surface::buffer::KakouneBufferSurface;

    /// Helper: create a registry with the primary buffer surface registered.
    fn registry_with_buffer() -> SurfaceRegistry {
        let mut r = SurfaceRegistry::new();
        r.register(Box::new(KakouneBufferSurface::new()));
        r
    }

    #[test]
    fn bind_and_lookup() {
        let mut r = registry_with_buffer();
        let sid = SurfaceId::BUFFER;
        let session = SessionId(1);

        r.bind_session(sid, session);
        assert_eq!(r.session_for_surface(sid), Some(session));
        assert_eq!(r.surface_for_session(session), Some(sid));
    }

    #[test]
    fn unbind_by_surface() {
        let mut r = registry_with_buffer();
        let sid = SurfaceId::BUFFER;
        let session = SessionId(1);

        r.bind_session(sid, session);
        assert_eq!(r.unbind_session_by_surface(sid), Some(session));
        assert_eq!(r.session_for_surface(sid), None);
        assert_eq!(r.surface_for_session(session), None);
    }

    #[test]
    fn unbind_by_session() {
        let mut r = registry_with_buffer();
        let sid = SurfaceId::BUFFER;
        let session = SessionId(1);

        r.bind_session(sid, session);
        assert_eq!(r.unbind_session_by_session(session), Some(sid));
        assert_eq!(r.session_for_surface(sid), None);
        assert_eq!(r.surface_for_session(session), None);
    }

    #[test]
    fn rebind_overwrites_previous() {
        let mut r = SurfaceRegistry::new();
        // Register two surfaces
        r.register(Box::new(KakouneBufferSurface::new()));
        r.register(Box::new(crate::surface::buffer::ClientBufferSurface::new(
            SurfaceId(200),
        )));
        let session = SessionId(1);

        r.bind_session(SurfaceId::BUFFER, session);
        r.bind_session(SurfaceId(200), session);

        // BUFFER should no longer be bound
        assert_eq!(r.session_for_surface(SurfaceId::BUFFER), None);
        assert_eq!(r.surface_for_session(session), Some(SurfaceId(200)));
    }

    #[test]
    fn is_pane_client_distinguishes_primary() {
        let mut r = SurfaceRegistry::new();
        r.register(Box::new(KakouneBufferSurface::new()));
        r.register(Box::new(crate::surface::buffer::ClientBufferSurface::new(
            SurfaceId(SurfaceId::PLUGIN_BASE),
        )));
        let primary = SessionId(1);
        let pane = SessionId(2);

        r.bind_session(SurfaceId::BUFFER, primary);
        r.bind_session(SurfaceId(SurfaceId::PLUGIN_BASE), pane);

        assert!(!r.is_pane_client(primary));
        assert!(r.is_pane_client(pane));
        assert!(!r.is_pane_client(SessionId(99)));
    }

    #[test]
    fn server_session_name() {
        let mut r = SurfaceRegistry::new();
        assert!(r.server_session_name().is_none());

        r.set_server_session_name("kasane-1234".to_string());
        assert_eq!(r.server_session_name(), Some("kasane-1234"));
    }

    #[test]
    fn pending_initial_resize() {
        let mut r = registry_with_buffer();
        let session = SessionId(1);
        r.bind_session(SurfaceId::BUFFER, session);

        assert!(!r.has_pending_resize(session));
        assert!(!r.take_pending_resize(session));

        r.mark_pending_resize(session);
        assert!(r.has_pending_resize(session));

        assert!(r.take_pending_resize(session));
        assert!(!r.has_pending_resize(session));
        assert!(!r.take_pending_resize(session));
    }

    #[test]
    fn is_multi_pane() {
        let mut r = SurfaceRegistry::new();
        r.register(Box::new(KakouneBufferSurface::new()));
        r.register(Box::new(crate::surface::buffer::ClientBufferSurface::new(
            SurfaceId(SurfaceId::PLUGIN_BASE),
        )));

        assert!(!r.is_multi_pane());
        r.bind_session(SurfaceId::BUFFER, SessionId(1));
        assert!(!r.is_multi_pane());
        r.bind_session(SurfaceId(SurfaceId::PLUGIN_BASE), SessionId(2));
        assert!(r.is_multi_pane());
    }

    #[test]
    fn needs_resize_deduplication() {
        let mut r = registry_with_buffer();
        let session = SessionId(1);
        r.bind_session(SurfaceId::BUFFER, session);

        assert!(r.needs_resize(session, 24, 80));
        assert!(!r.needs_resize(session, 24, 80));
        assert!(r.needs_resize(session, 48, 80));
    }

    #[test]
    fn remove_cleans_up_binding() {
        let mut r = registry_with_buffer();
        let session = SessionId(1);
        r.bind_session(SurfaceId::BUFFER, session);
        assert_eq!(r.surface_for_session(session), Some(SurfaceId::BUFFER));

        r.remove(SurfaceId::BUFFER);
        assert_eq!(r.surface_for_session(session), None);
    }
}
