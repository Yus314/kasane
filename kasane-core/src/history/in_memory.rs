//! Default `HistoryBackend`: a fixed-capacity ring buffer.
//!
//! Holds the most recent N snapshots in process memory; older entries
//! are FIFO-evicted. `commit` assigns monotonic `VersionId`s so the
//! caller always knows which version it just produced; queries against
//! evicted versions return `HistoryError::Evicted` (versions that were
//! never observed return `Unknown`).
//!
//! Concurrency: the ring lives behind a `Mutex`. Snapshot payloads are
//! `Arc<str>`, so `snapshot()` clones the handle, not the bytes.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::state::selection::{BufferId, BufferVersion};
use crate::state::selection_set::SelectionSet;

use super::{HistoryBackend, HistoryError, Snapshot, VersionId};

/// Default ring capacity. The choice of 256 keeps the working set
/// bounded for typical interactive use; long-running sessions that
/// need deep history should switch to the git-backed or rocksdb
/// backend (deferred — see ADR-035 §2).
pub const DEFAULT_CAPACITY: usize = 256;

pub struct InMemoryRing {
    inner: Mutex<RingInner>,
}

impl std::fmt::Debug for InMemoryRing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid dumping every snapshot — print only the range and
        // capacity so debug output stays bounded.
        match self.inner.lock() {
            Ok(inner) => f
                .debug_struct("InMemoryRing")
                .field("len", &inner.entries.len())
                .field("capacity", &inner.capacity)
                .field("earliest", &inner.entries.front().map(|s| s.version))
                .field("current", &inner.entries.back().map(|s| s.version))
                .finish(),
            Err(_) => f
                .debug_struct("InMemoryRing")
                .field("state", &"<poisoned>")
                .finish(),
        }
    }
}

struct RingInner {
    /// FIFO; oldest at front, newest at back. Both `current_version`
    /// and `earliest_version` are derivable from this deque (it is the
    /// canonical state).
    entries: VecDeque<Snapshot>,
    next_version: VersionId,
    capacity: usize,
}

impl InMemoryRing {
    /// Construct a ring of `DEFAULT_CAPACITY` entries.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Construct a ring with an explicit capacity. Capacity must be
    /// at least 1; smaller values are rounded up.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Mutex::new(RingInner {
                entries: VecDeque::with_capacity(capacity),
                next_version: VersionId::INITIAL,
                capacity,
            }),
        }
    }
}

impl Default for InMemoryRing {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryBackend for InMemoryRing {
    fn snapshot(&self, v: VersionId) -> Result<Snapshot, HistoryError> {
        let inner = self.inner.lock().expect("history ring poisoned");
        // Empty ring: the caller has not committed anything yet.
        let earliest = match inner.entries.front() {
            Some(snap) => snap.version,
            None => return Err(HistoryError::Unknown),
        };
        // Versions newer than the most recent commit are unknown.
        if v >= inner.next_version {
            return Err(HistoryError::Unknown);
        }
        if v < earliest {
            return Err(HistoryError::Evicted);
        }
        // Linear scan is fine at capacity 256; an indexed lookup is a
        // future optimisation if profiling demands it.
        inner
            .entries
            .iter()
            .find(|s| s.version == v)
            .cloned()
            .ok_or(HistoryError::Unknown)
    }

    fn current_version(&self) -> VersionId {
        let inner = self.inner.lock().expect("history ring poisoned");
        inner
            .entries
            .back()
            .map(|s| s.version)
            .unwrap_or(VersionId::INITIAL)
    }

    fn earliest_version(&self) -> VersionId {
        let inner = self.inner.lock().expect("history ring poisoned");
        inner
            .entries
            .front()
            .map(|s| s.version)
            .unwrap_or(VersionId::INITIAL)
    }

    fn commit(
        &self,
        snapshot_text: Arc<str>,
        selection: SelectionSet,
        buffer: BufferId,
        buf_ver: BufferVersion,
    ) -> VersionId {
        let mut inner = self.inner.lock().expect("history ring poisoned");
        let v = inner.next_version;
        let snap = Snapshot {
            version: v,
            buffer,
            buffer_version: buf_ver,
            text: snapshot_text,
            selection,
        };
        if inner.entries.len() == inner.capacity {
            inner.entries.pop_front();
        }
        inner.entries.push_back(snap);
        inner.next_version = inner.next_version.next();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf() -> BufferId {
        BufferId::new("test-buffer")
    }

    fn empty_sel() -> SelectionSet {
        SelectionSet::empty(buf(), BufferVersion::INITIAL)
    }

    #[test]
    fn empty_ring_returns_initial_versions() {
        let ring = InMemoryRing::new();
        assert_eq!(ring.current_version(), VersionId::INITIAL);
        assert_eq!(ring.earliest_version(), VersionId::INITIAL);
    }

    #[test]
    fn snapshot_on_empty_ring_is_unknown() {
        let ring = InMemoryRing::new();
        assert_eq!(ring.snapshot(VersionId(0)), Err(HistoryError::Unknown));
    }

    #[test]
    fn commit_returns_monotonic_versions() {
        let ring = InMemoryRing::new();
        let v0 = ring.commit(
            Arc::from("hello"),
            empty_sel(),
            buf(),
            BufferVersion::INITIAL,
        );
        let v1 = ring.commit(
            Arc::from("hello world"),
            empty_sel(),
            buf(),
            BufferVersion(1),
        );
        assert_eq!(v0, VersionId(0));
        assert_eq!(v1, VersionId(1));
        assert_eq!(ring.current_version(), v1);
        assert_eq!(ring.earliest_version(), v0);
    }

    #[test]
    fn snapshot_returns_committed_payload() {
        let ring = InMemoryRing::new();
        let v = ring.commit(
            Arc::from("payload"),
            empty_sel(),
            buf(),
            BufferVersion::INITIAL,
        );
        let snap = ring.snapshot(v).unwrap();
        assert_eq!(&*snap.text, "payload");
        assert_eq!(snap.version, v);
        assert_eq!(snap.buffer, buf());
        assert!(snap.selection.is_empty());
    }

    #[test]
    fn fifo_eviction_when_capacity_exceeded() {
        let ring = InMemoryRing::with_capacity(2);
        let v0 = ring.commit(Arc::from("a"), empty_sel(), buf(), BufferVersion(0));
        let v1 = ring.commit(Arc::from("b"), empty_sel(), buf(), BufferVersion(1));
        let v2 = ring.commit(Arc::from("c"), empty_sel(), buf(), BufferVersion(2));

        assert_eq!(ring.snapshot(v0), Err(HistoryError::Evicted));
        assert_eq!(ring.snapshot(v1).unwrap().text.as_ref(), "b");
        assert_eq!(ring.snapshot(v2).unwrap().text.as_ref(), "c");
        assert_eq!(ring.earliest_version(), v1);
        assert_eq!(ring.current_version(), v2);
    }

    #[test]
    fn future_versions_are_unknown() {
        let ring = InMemoryRing::new();
        ring.commit(Arc::from("a"), empty_sel(), buf(), BufferVersion::INITIAL);
        assert_eq!(ring.snapshot(VersionId(10)), Err(HistoryError::Unknown));
    }

    #[test]
    fn capacity_zero_rounds_up_to_one() {
        let ring = InMemoryRing::with_capacity(0);
        let v = ring.commit(Arc::from("a"), empty_sel(), buf(), BufferVersion::INITIAL);
        assert!(ring.snapshot(v).is_ok());
    }
}
