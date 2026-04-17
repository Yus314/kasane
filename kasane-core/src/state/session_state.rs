//! Session metadata sub-struct.
//!
//! Contains session-level state managed by SessionManager, preserved across
//! session switches. This is the `S` component of the world model `W = (T, I, Π, S)`.

use crate::session::SessionDescriptor;

/// Session metadata state.
///
/// Every field here carries `#[epistemic(session)]` semantics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionState {
    pub session_descriptors: Vec<SessionDescriptor>,
    pub active_session_key: Option<String>,
}
