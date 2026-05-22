// ADR-051 Chunk 1: registry skeleton only. The first consumer
// (syntax-reload migration) lands in a follow-up chunk; until then
// the registry methods are exercised only by unit tests and the dead-
// code lint must be silenced. Remove this attribute when the first
// production caller appears.
#![allow(dead_code)]

//! Host-internal registry for external data sources.
//!
//! `ExternalInputRegistry` is the single mutation surface for data
//! produced outside `AppState` — file watchers, LSP transports, network
//! responses, future AI streams. Transport adapters translate push
//! events into [`ExternalInputRegistry::commit`] calls; the registry
//! buffers them per [`BackPressurePolicy`] until
//! [`ExternalInputRegistry::drain`] is called at the frame boundary,
//! before any plugin pull.
//!
//! This is the push half of the push-to-set / pull-to-derive split
//! introduced by ADR-051 (DDD-CST Phase α). The pull half remains the
//! existing Salsa query path; subsequent chunks of ADR-051 wire
//! specific sources to either Salsa input slots (Category I per
//! ADR-050) or revision-keyed manual caches (Category II).
//!
//! The registry types are deliberately Salsa-agnostic: a source's
//! choice of backing (Salsa-tracked vs manual cache) is decided at
//! the migration site, not in this module.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

/// Typed handle for an external data source.
///
/// Returned by [`ExternalInputRegistry::register`] and held by
/// transport adapters and Salsa-sync code. The type parameter `T` is
/// the value type the source produces; it is enforced at
/// `commit` / `last` time via downcast.
pub struct ExternalInputId<T: 'static> {
    id: u32,
    _phantom: PhantomData<fn() -> T>,
}

impl<T: 'static> Copy for ExternalInputId<T> {}

impl<T: 'static> Clone for ExternalInputId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> PartialEq for ExternalInputId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Eq for ExternalInputId<T> {}

impl<T: 'static> std::hash::Hash for ExternalInputId<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: 'static> fmt::Debug for ExternalInputId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalInputId")
            .field("id", &self.id)
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}

/// Behaviour when commits arrive faster than the frame can drain them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackPressurePolicy {
    /// Replace the pending value on each commit. Most recent value wins
    /// at drain time. Suitable for sources whose values supersede
    /// previous ones (mtime updates, cursor positions, viewport sizes).
    Coalesce,
    /// Buffer up to `cap` commits. Overflow is silently dropped (the
    /// caller is expected to have already lost ordering at the transport).
    /// Suitable for sources whose individual values matter (LSP
    /// diagnostics arriving in sequence).
    Queue { cap: usize },
    /// Buffer up to `cap` commits; on overflow drop the oldest. Suitable
    /// for high-throughput sources where recent values matter more than
    /// ordering integrity.
    DropOldest { cap: usize },
}

/// Drainable per-source slot. The trait lets the registry drain
/// type-erased slots uniformly at frame boundary.
trait Slot: Send {
    /// Move pending values into "last committed" position. Returns the
    /// number of values absorbed (zero if nothing pending).
    fn drain(&mut self) -> usize;
    /// True if the most recent drain produced a new committed value
    /// (i.e. at least one pending was absorbed).
    fn dirty(&self) -> bool;
    /// Reset the dirty flag. Called after consumers have observed the
    /// drained value for this frame.
    fn clear_dirty(&mut self);
    /// Number of pending commits not yet drained.
    fn pending_len(&self) -> usize;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct TypedSlot<T: 'static + Send> {
    pending: Vec<T>,
    policy: BackPressurePolicy,
    last_committed: Option<T>,
    dirty: bool,
}

impl<T: 'static + Send> Slot for TypedSlot<T> {
    fn drain(&mut self) -> usize {
        let n = self.pending.len();
        if let Some(latest) = self.pending.drain(..).last() {
            self.last_committed = Some(latest);
            self.dirty = true;
        }
        n
    }
    fn dirty(&self) -> bool {
        self.dirty
    }
    fn clear_dirty(&mut self) {
        self.dirty = false;
    }
    fn pending_len(&self) -> usize {
        self.pending.len()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Host-side registry of external data sources. Single mutation surface
/// for non-`AppState` data per ADR-051.
///
/// The registry lives alongside `SalsaInputHandles` in the host state;
/// it is created once and reused for the session's lifetime. Handles
/// returned by [`Self::register`] are stable across the session.
pub struct ExternalInputRegistry {
    slots: HashMap<u32, Box<dyn Slot>>,
    next_id: u32,
}

impl ExternalInputRegistry {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            next_id: 0,
        }
    }

    /// Register a new external input source. The returned handle is
    /// stable for the session; callers store it in transport-adapter
    /// state and Salsa-sync code.
    ///
    /// `name` is used only for diagnostic / Debug output; it does not
    /// participate in lookup.
    pub fn register<T: 'static + Send>(
        &mut self,
        _name: &'static str,
        policy: BackPressurePolicy,
    ) -> ExternalInputId<T> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("ExternalInputId overflow");
        let slot: TypedSlot<T> = TypedSlot {
            pending: Vec::new(),
            policy,
            last_committed: None,
            dirty: false,
        };
        self.slots.insert(id, Box::new(slot));
        ExternalInputId {
            id,
            _phantom: PhantomData,
        }
    }

    /// Push a new value into the source identified by `handle`. The
    /// value is buffered per the source's [`BackPressurePolicy`] and
    /// becomes observable to readers only after the next [`Self::drain`].
    ///
    /// Panics if `handle` was registered with a different `T`. This is
    /// a host-side bug: transport adapters and Salsa-sync code agree
    /// on the type at registration.
    pub fn commit<T: 'static + Send>(&mut self, handle: ExternalInputId<T>, value: T) {
        let slot = self
            .slots
            .get_mut(&handle.id)
            .expect("ExternalInputId references unregistered slot")
            .as_any_mut()
            .downcast_mut::<TypedSlot<T>>()
            .expect("ExternalInputId type mismatch");
        match slot.policy {
            BackPressurePolicy::Coalesce => {
                slot.pending.clear();
                slot.pending.push(value);
            }
            BackPressurePolicy::Queue { cap } => {
                if slot.pending.len() < cap {
                    slot.pending.push(value);
                }
                // Overflow: silently drop. The caller's transport already
                // lost ordering integrity before this point.
            }
            BackPressurePolicy::DropOldest { cap } => {
                while slot.pending.len() >= cap {
                    slot.pending.remove(0);
                }
                slot.pending.push(value);
            }
        }
    }

    /// Read the most recently drained value. Returns `None` if no commit
    /// has been drained yet.
    ///
    /// Readers see the value as of the most recent [`Self::drain`]
    /// boundary, not the most recent `commit`. This is the
    /// glitch-freedom guarantee of ADR-051 §4.1.9: mid-frame commits
    /// are invisible until the frame boundary.
    pub fn last<T: 'static + Send>(&self, handle: ExternalInputId<T>) -> Option<&T> {
        let slot = self
            .slots
            .get(&handle.id)?
            .as_any()
            .downcast_ref::<TypedSlot<T>>()?;
        slot.last_committed.as_ref()
    }

    /// Number of commits pending against `handle` (not yet drained).
    /// Diagnostic helper; back-pressure decisions live inside `commit`.
    pub fn pending_len<T: 'static + Send>(&self, handle: ExternalInputId<T>) -> usize {
        self.slots
            .get(&handle.id)
            .map(|s| s.pending_len())
            .unwrap_or(0)
    }

    /// Drain all pending commits into their respective "last committed"
    /// positions. Called once per frame at the frame boundary, before
    /// any plugin pull. Returns the total number of slots that received
    /// new values.
    pub fn drain(&mut self) -> usize {
        let mut dirty_count = 0;
        for slot in self.slots.values_mut() {
            if slot.drain() > 0 {
                dirty_count += 1;
            }
        }
        dirty_count
    }

    /// True if any slot was newly committed at the most recent
    /// [`Self::drain`]. Caller may use this to skip downstream
    /// invalidation work when no external source changed this frame.
    pub fn any_dirty(&self) -> bool {
        self.slots.values().any(|s| s.dirty())
    }

    /// Clear dirty flags after consumers have observed the drained
    /// values. Called at the end of the frame, paired with `drain`
    /// at the start.
    pub fn clear_dirty(&mut self) {
        for slot in self.slots.values_mut() {
            slot.clear_dirty();
        }
    }
}

impl Default for ExternalInputRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ExternalInputRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalInputRegistry")
            .field("slot_count", &self.slots.len())
            .field("next_id", &self.next_id)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_invisible_before_drain() {
        let mut reg = ExternalInputRegistry::new();
        let id = reg.register::<u32>("test", BackPressurePolicy::Coalesce);
        reg.commit(id, 42);
        // Pre-drain: not yet observable. This is the glitch-freedom
        // guarantee — mid-frame commits don't leak.
        assert_eq!(reg.last(id), None);
        assert_eq!(reg.pending_len(id), 1);

        reg.drain();
        assert_eq!(reg.last(id), Some(&42));
        assert_eq!(reg.pending_len(id), 0);
    }

    #[test]
    fn coalesce_keeps_latest_only() {
        let mut reg = ExternalInputRegistry::new();
        let id = reg.register::<u32>("test", BackPressurePolicy::Coalesce);
        for v in 0..1000 {
            reg.commit(id, v);
        }
        // Memory bound: Coalesce holds at most one pending regardless
        // of how many commits arrived. This is the time-leak guarantee.
        assert_eq!(reg.pending_len(id), 1);
        reg.drain();
        assert_eq!(reg.last(id), Some(&999));
    }

    #[test]
    fn queue_drops_overflow() {
        let mut reg = ExternalInputRegistry::new();
        let id = reg.register::<u32>("test", BackPressurePolicy::Queue { cap: 3 });
        for v in 0..10 {
            reg.commit(id, v);
        }
        assert_eq!(reg.pending_len(id), 3);
        reg.drain();
        // Queue keeps the first cap values; drain takes the last of them.
        assert_eq!(reg.last(id), Some(&2));
    }

    #[test]
    fn drop_oldest_keeps_recent() {
        let mut reg = ExternalInputRegistry::new();
        let id = reg.register::<u32>("test", BackPressurePolicy::DropOldest { cap: 3 });
        for v in 0..10 {
            reg.commit(id, v);
        }
        assert_eq!(reg.pending_len(id), 3);
        reg.drain();
        // DropOldest keeps the last cap values.
        assert_eq!(reg.last(id), Some(&9));
    }

    #[test]
    fn dirty_tracking() {
        let mut reg = ExternalInputRegistry::new();
        let id = reg.register::<u32>("test", BackPressurePolicy::Coalesce);

        assert!(!reg.any_dirty());

        reg.commit(id, 1);
        assert!(!reg.any_dirty(), "commit alone does not mark dirty");

        reg.drain();
        assert!(reg.any_dirty(), "drain after commit marks dirty");

        reg.clear_dirty();
        assert!(!reg.any_dirty());

        // No new commit; drain alone should not re-mark.
        reg.drain();
        assert!(!reg.any_dirty());
    }

    #[test]
    fn distinct_types_share_registry() {
        let mut reg = ExternalInputRegistry::new();
        let counter = reg.register::<u32>("counter", BackPressurePolicy::Coalesce);
        let label = reg.register::<String>("label", BackPressurePolicy::Coalesce);

        reg.commit(counter, 42);
        reg.commit(label, "hello".to_string());
        reg.drain();

        assert_eq!(reg.last(counter), Some(&42));
        assert_eq!(reg.last(label), Some(&"hello".to_string()));
    }

    #[test]
    fn handles_are_copy_and_eq() {
        let mut reg = ExternalInputRegistry::new();
        let id = reg.register::<u32>("test", BackPressurePolicy::Coalesce);
        let id2 = id;
        assert_eq!(id, id2);

        // Distinct registrations produce distinct ids.
        let other = reg.register::<u32>("other", BackPressurePolicy::Coalesce);
        assert_ne!(id, other);
    }

    #[test]
    #[should_panic(expected = "type mismatch")]
    fn type_mismatch_panics() {
        let mut reg = ExternalInputRegistry::new();
        let id = reg.register::<u32>("test", BackPressurePolicy::Coalesce);
        // Mint a handle with a different type but the same numeric id.
        // Production callers can't do this because the type is bound to
        // the id at registration; the test reaches behind the public
        // API to verify the downcast guard fires.
        let bad: ExternalInputId<String> = ExternalInputId {
            id: 0,
            _phantom: PhantomData,
        };
        let _ = id;
        reg.commit(bad, "oops".to_string());
    }
}
