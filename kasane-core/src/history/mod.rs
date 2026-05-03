//! ADR-035 §2 — Time as a Salsa input dimension.
//!
//! `Time` is the primitive that lets every buffer / display / selection
//! query take a temporal coordinate. `Time::Now` is the constant fast
//! path that retains today's behaviour; `Time::At(v)` materialises
//! against a `HistoryBackend` that the embedder configures (default:
//! `InMemoryRing`).
//!
//! This module ships in parallel with the existing observed/derived
//! state surface; queries are not yet rewritten to take `Time` — that
//! is the migration step tracked separately.

pub mod in_memory;

pub use in_memory::InMemoryRing;

use std::sync::Arc;

use super::state::selection::{BufferId, BufferVersion};
use super::state::selection_set::SelectionSet;

/// A monotonic, opaque version handle. The `0` value is the initial
/// version; subsequent versions are minted in protocol-echo order by
/// the host's history backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VersionId(pub u64);

impl VersionId {
    pub const INITIAL: Self = Self(0);

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Time coordinate for a query. `Now` short-circuits; `At(v)` requires
/// the configured backend to materialise the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Time {
    Now,
    At(VersionId),
}

impl Time {
    pub fn at(v: VersionId) -> Self {
        Time::At(v)
    }

    pub fn is_now(&self) -> bool {
        matches!(self, Time::Now)
    }

    pub fn version(&self) -> Option<VersionId> {
        match self {
            Time::At(v) => Some(*v),
            Time::Now => None,
        }
    }
}

/// A point-in-time snapshot of buffer state. The exact payload is
/// pluggable per backend; for the default `InMemoryRing` we store the
/// full text and the active `SelectionSet` plus the canonical
/// `BufferVersion`. Future backends may store a diff plus the
/// materialisation function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub version: VersionId,
    pub buffer: BufferId,
    pub buffer_version: BufferVersion,
    /// The buffer text at this version. `Arc<str>` so consumers can
    /// hold a reference cheaply through Salsa's value-cache layer.
    pub text: Arc<str>,
    /// The canonical `SelectionSet` active at this version. Empty for
    /// snapshots committed without selection wiring (e.g. the apply
    /// auto-commit currently passes `SelectionSet::empty` until a
    /// follow-up wires the protocol-derived selection projection).
    pub selection: SelectionSet,
}

impl Snapshot {
    pub fn new(
        version: VersionId,
        buffer: BufferId,
        buffer_version: BufferVersion,
        text: impl Into<Arc<str>>,
        selection: SelectionSet,
    ) -> Self {
        Self {
            version,
            buffer,
            buffer_version,
            text: text.into(),
            selection,
        }
    }
}

/// Errors a history backend may surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    /// The requested version exists in the canonical timeline but the
    /// backend has evicted it (e.g. ring buffer rolled over).
    Evicted,
    /// The requested version was never observed.
    Unknown,
}

/// Pluggable history backend. The default is `InMemoryRing`; embedders
/// can swap in `GitBacked` or `RocksDb` per ADR-035 §2 by configuring
/// `kasane.kdl` `history.backend` (the wiring is a follow-up).
pub trait HistoryBackend: Send + Sync {
    /// Materialise the snapshot for `v`, or surface an `Evicted` /
    /// `Unknown` error.
    fn snapshot(&self, v: VersionId) -> Result<Snapshot, HistoryError>;

    /// The most recent committed version. Always returns a value (the
    /// initial `VersionId::INITIAL` is committed at construction).
    fn current_version(&self) -> VersionId;

    /// The earliest still-materialisable version. For ring buffers,
    /// this advances as old entries are evicted.
    fn earliest_version(&self) -> VersionId;

    /// Commit a new snapshot. The backend assigns the resulting
    /// `VersionId` (monotonic, > current_version). The `selection` is
    /// the canonical `SelectionSet` active at this version; pass
    /// `SelectionSet::empty(...)` when no selection state is being
    /// committed.
    fn commit(
        &self,
        snapshot_text: Arc<str>,
        selection: SelectionSet,
        buffer: BufferId,
        buf_ver: BufferVersion,
    ) -> VersionId;
}
