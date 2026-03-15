use std::collections::HashMap;

use crate::state::AppState;

/// Stable runtime identifier for a managed Kakoune session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub u32);

/// Static spec used to open or identify a managed session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSpec {
    pub key: String,
    pub session: Option<String>,
    pub args: Vec<String>,
}

impl SessionSpec {
    pub fn new(key: impl Into<String>, session: Option<String>, args: Vec<String>) -> Self {
        Self {
            key: key.into(),
            session,
            args,
        }
    }

    pub fn primary(session: Option<String>, args: Vec<String>) -> Self {
        let key = session.clone().unwrap_or_else(|| "primary".to_string());
        Self { key, session, args }
    }

    pub fn with_fallback_key(
        key: Option<String>,
        session: Option<String>,
        args: Vec<String>,
    ) -> Self {
        let key = key
            .or_else(|| session.clone())
            .unwrap_or_else(|| format!("session:{}", next_synthetic_key_fragment(&args)));
        Self { key, session, args }
    }
}

fn next_synthetic_key_fragment(args: &[String]) -> String {
    args.first()
        .map(|arg| arg.replace('/', "_"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unnamed".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    Spawn {
        key: Option<String>,
        session: Option<String>,
        args: Vec<String>,
        activate: bool,
    },
    Close {
        key: Option<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SessionManagerError {
    DuplicateSessionKey(String),
    MissingSession(SessionId),
    NoActiveSession,
}

/// Snapshot store for per-session UI state.
///
/// V1 still renders only one active session at a time, but keeping inactive
/// snapshots warm prevents a session switch from falling back to an empty UI.
#[derive(Debug, Default)]
pub struct SessionStateStore {
    states: HashMap<SessionId, AppState>,
}

impl SessionStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, id: SessionId) -> bool {
        self.states.contains_key(&id)
    }

    pub fn ensure_session(&mut self, id: SessionId, template: &AppState) -> &mut AppState {
        self.states.entry(id).or_insert_with(|| {
            let mut snapshot = template.clone();
            snapshot.reset_for_session_switch();
            snapshot
        })
    }

    pub fn sync_from_active(&mut self, id: SessionId, state: &AppState) {
        self.states.insert(id, state.clone());
    }

    pub fn restore_into(&self, id: SessionId, target: &mut AppState) -> bool {
        if let Some(snapshot) = self.states.get(&id) {
            *target = snapshot.clone();
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, id: SessionId) -> Option<AppState> {
        self.states.remove(&id)
    }

    pub fn sync_active_from_manager<R, W, C>(
        &mut self,
        session_manager: &SessionManager<R, W, C>,
        state: &AppState,
    ) {
        if let Some(active) = session_manager.active_session_id() {
            self.sync_from_active(active, state);
        }
    }
}

struct ManagedSession<R, W, C> {
    spec: SessionSpec,
    reader: Option<R>,
    writer: W,
    child: C,
}

/// Runtime-owned set of Kakoune sessions.
///
/// V1 keeps a single active session bound to the UI, but the manager tracks
/// multiple opened sessions so spawn/close can be layered on later.
pub struct SessionManager<R, W, C> {
    next_id: u32,
    active: Option<SessionId>,
    sessions: HashMap<SessionId, ManagedSession<R, W, C>>,
    ids_by_key: HashMap<String, SessionId>,
    order: Vec<SessionId>,
}

impl<R, W, C> Default for SessionManager<R, W, C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R, W, C> SessionManager<R, W, C> {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            active: None,
            sessions: HashMap::new(),
            ids_by_key: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn active_session_id(&self) -> Option<SessionId> {
        self.active
    }

    pub fn active_spec(&self) -> Option<&SessionSpec> {
        let active = self.active?;
        self.sessions.get(&active).map(|session| &session.spec)
    }

    pub fn session_id_by_key(&self, key: &str) -> Option<SessionId> {
        self.ids_by_key.get(key).copied()
    }

    pub fn ordered_sessions(&self) -> Vec<(SessionId, &SessionSpec)> {
        self.order
            .iter()
            .filter_map(|id| self.sessions.get(id).map(|session| (*id, &session.spec)))
            .collect()
    }

    pub fn insert(
        &mut self,
        spec: SessionSpec,
        reader: R,
        writer: W,
        child: C,
    ) -> Result<SessionId, SessionManagerError> {
        if self.ids_by_key.contains_key(spec.key.as_str()) {
            return Err(SessionManagerError::DuplicateSessionKey(spec.key));
        }

        let id = SessionId(self.next_id);
        self.next_id += 1;
        self.ids_by_key.insert(spec.key.clone(), id);
        self.order.push(id);
        self.sessions.insert(
            id,
            ManagedSession {
                spec,
                reader: Some(reader),
                writer,
                child,
            },
        );
        if self.active.is_none() {
            self.active = Some(id);
        }
        Ok(id)
    }

    pub fn set_active(&mut self, id: SessionId) -> Result<(), SessionManagerError> {
        if self.sessions.contains_key(&id) {
            self.active = Some(id);
            Ok(())
        } else {
            Err(SessionManagerError::MissingSession(id))
        }
    }

    pub fn close(&mut self, id: SessionId) -> Option<(SessionSpec, Option<R>, W, C)> {
        let session = self.sessions.remove(&id)?;
        self.ids_by_key.remove(session.spec.key.as_str());
        let next_active =
            if let Some(index) = self.order.iter().position(|candidate| *candidate == id) {
                self.order.remove(index);
                self.order
                    .get(index)
                    .copied()
                    .or_else(|| self.order.last().copied())
            } else {
                self.order.first().copied()
            };
        if self.active == Some(id) {
            self.active = next_active;
        }
        Some((session.spec, session.reader, session.writer, session.child))
    }

    pub fn take_active_parts(&mut self) -> Result<(SessionSpec, R, W, C), SessionManagerError> {
        let active = self.active.ok_or(SessionManagerError::NoActiveSession)?;
        let (spec, reader, writer, child) = self
            .close(active)
            .ok_or(SessionManagerError::MissingSession(active))?;
        let reader = reader.ok_or(SessionManagerError::MissingSession(active))?;
        Ok((spec, reader, writer, child))
    }

    pub fn take_reader(&mut self, id: SessionId) -> Result<R, SessionManagerError> {
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or(SessionManagerError::MissingSession(id))?;
        session
            .reader
            .take()
            .ok_or(SessionManagerError::MissingSession(id))
    }

    pub fn take_active_reader(&mut self) -> Result<R, SessionManagerError> {
        let active = self.active.ok_or(SessionManagerError::NoActiveSession)?;
        self.take_reader(active)
    }

    pub fn active_writer_mut(&mut self) -> Result<&mut W, SessionManagerError> {
        let active = self.active.ok_or(SessionManagerError::NoActiveSession)?;
        self.writer_mut(active)
    }

    pub fn writer_mut(&mut self, id: SessionId) -> Result<&mut W, SessionManagerError> {
        self.sessions
            .get_mut(&id)
            .map(|session| &mut session.writer)
            .ok_or(SessionManagerError::MissingSession(id))
    }

    pub fn sync_and_activate(
        &mut self,
        session_states: &mut SessionStateStore,
        next: SessionId,
        state: &AppState,
    ) -> Result<(), SessionManagerError> {
        session_states.sync_active_from_manager(self, state);
        self.set_active(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_sets_first_session_active() {
        let mut sessions = SessionManager::<(), (), ()>::new();
        let first = sessions
            .insert(SessionSpec::primary(None, vec![]), (), (), ())
            .unwrap();

        assert_eq!(sessions.active_session_id(), Some(first));
        assert_eq!(sessions.active_spec().unwrap().key, "primary");
    }

    #[test]
    fn duplicate_keys_are_rejected() {
        let mut sessions = SessionManager::<(), (), ()>::new();
        sessions
            .insert(SessionSpec::new("work", None, vec![]), (), (), ())
            .unwrap();

        assert_eq!(
            sessions.insert(SessionSpec::new("work", None, vec![]), (), (), ()),
            Err(SessionManagerError::DuplicateSessionKey("work".to_string()))
        );
    }

    #[test]
    fn close_active_promotes_remaining_session() {
        let mut sessions = SessionManager::<(), (), ()>::new();
        let first = sessions
            .insert(SessionSpec::new("first", None, vec![]), (), (), ())
            .unwrap();
        let second = sessions
            .insert(SessionSpec::new("second", None, vec![]), (), (), ())
            .unwrap();

        let (spec, _, _, _) = sessions.close(first).unwrap();

        assert_eq!(spec.key, "first");
        assert_eq!(sessions.active_session_id(), Some(second));
        assert_eq!(sessions.active_spec().unwrap().key, "second");
    }

    #[test]
    fn close_active_promotes_next_session_in_creation_order() {
        let mut sessions = SessionManager::<(), (), ()>::new();
        let first = sessions
            .insert(SessionSpec::new("first", None, vec![]), (), (), ())
            .unwrap();
        let second = sessions
            .insert(SessionSpec::new("second", None, vec![]), (), (), ())
            .unwrap();
        let third = sessions
            .insert(SessionSpec::new("third", None, vec![]), (), (), ())
            .unwrap();

        sessions.set_active(second).unwrap();
        let (spec, _, _, _) = sessions.close(second).unwrap();

        assert_eq!(spec.key, "second");
        assert_eq!(sessions.active_session_id(), Some(third));

        let ordered_keys: Vec<_> = sessions
            .ordered_sessions()
            .into_iter()
            .map(|(_, spec)| spec.key.as_str())
            .collect();
        assert_eq!(ordered_keys, vec!["first", "third"]);
        assert_eq!(sessions.active_spec().unwrap().key, "third");
        assert_eq!(first, SessionId(1));
    }

    #[test]
    fn close_last_active_promotes_previous_session_in_creation_order() {
        let mut sessions = SessionManager::<(), (), ()>::new();
        let first = sessions
            .insert(SessionSpec::new("first", None, vec![]), (), (), ())
            .unwrap();
        let second = sessions
            .insert(SessionSpec::new("second", None, vec![]), (), (), ())
            .unwrap();

        sessions.set_active(second).unwrap();
        let _ = sessions.close(second).unwrap();

        assert_eq!(sessions.active_session_id(), Some(first));
        assert_eq!(sessions.active_spec().unwrap().key, "first");
    }

    #[test]
    fn take_active_parts_returns_primary_session() {
        let mut sessions = SessionManager::<usize, usize, usize>::new();
        sessions
            .insert(
                SessionSpec::new("main", Some("main".into()), vec!["file".into()]),
                1,
                2,
                3,
            )
            .unwrap();

        let (spec, reader, writer, child) = sessions.take_active_parts().unwrap();

        assert_eq!(spec.key, "main");
        assert_eq!(spec.session.as_deref(), Some("main"));
        assert_eq!(spec.args, vec!["file".to_string()]);
        assert_eq!((reader, writer, child), (1, 2, 3));
        assert!(sessions.is_empty());
    }

    #[test]
    fn session_state_store_uses_reset_template_for_new_sessions() {
        let mut template = AppState::default();
        template.cols = 120;
        template.rows = 40;
        template.focused = true;
        template.shadow_enabled = true;
        template.status_at_top = true;
        template.lines = vec![vec![]];
        template.lines_dirty = vec![true];
        template.cursor_count = 3;

        let mut store = SessionStateStore::new();
        let snapshot = store.ensure_session(SessionId(7), &template);

        assert!(snapshot.lines.is_empty());
        assert!(snapshot.lines_dirty.is_empty());
        assert_eq!(snapshot.cursor_count, 0);
        assert_eq!(snapshot.cols, 120);
        assert_eq!(snapshot.rows, 40);
        assert!(snapshot.focused);
        assert!(snapshot.shadow_enabled);
        assert!(snapshot.status_at_top);
    }

    #[test]
    fn session_state_store_sync_and_restore_round_trip() {
        let mut store = SessionStateStore::new();
        let id = SessionId(3);

        let mut source = AppState::default();
        source.cols = 90;
        source.rows = 25;
        source.lines = vec![vec![]];
        source.cursor_count = 2;
        store.sync_from_active(id, &source);

        let mut target = AppState::default();
        assert!(store.restore_into(id, &mut target));
        assert_eq!(target.cols, 90);
        assert_eq!(target.rows, 25);
        assert_eq!(target.lines.len(), 1);
        assert_eq!(target.cursor_count, 2);
    }

    #[test]
    fn session_state_store_remove_discards_snapshot() {
        let mut store = SessionStateStore::new();
        let id = SessionId(5);
        store.sync_from_active(id, &AppState::default());
        assert!(store.contains(id));
        assert!(store.remove(id).is_some());
        assert!(!store.contains(id));
    }

    #[test]
    fn sync_and_activate_preserves_previous_active_snapshot() {
        let mut sessions = SessionManager::<(), (), ()>::new();
        let first = sessions
            .insert(SessionSpec::new("first", None, vec![]), (), (), ())
            .unwrap();
        let second = sessions
            .insert(SessionSpec::new("second", None, vec![]), (), (), ())
            .unwrap();

        let mut state = AppState::default();
        state.cols = 100;
        state.rows = 30;
        state.cursor_count = 4;

        let mut store = SessionStateStore::new();
        store.ensure_session(first, &state);
        sessions
            .sync_and_activate(&mut store, second, &state)
            .expect("activation should succeed");

        assert_eq!(sessions.active_session_id(), Some(second));

        let mut restored = AppState::default();
        assert!(store.restore_into(first, &mut restored));
        assert_eq!(restored.cols, 100);
        assert_eq!(restored.rows, 30);
        assert_eq!(restored.cursor_count, 4);
    }
}
