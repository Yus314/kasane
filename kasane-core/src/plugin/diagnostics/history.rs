//! Bounded history buffer for plugin diagnostics.
//!
//! Captures every diagnostic that flows through the framework so that
//! a UI surface (e.g. the diagnostics panel) can display a persistent
//! list independent of the transient overlay popup.
//!
//! The history is intentionally bounded: a runaway plugin must not
//! be able to grow this without limit. Entries beyond the capacity
//! are dropped from the head; `truncated_count()` exposes how many
//! older entries have been discarded so the UI can hint at "older
//! entries available in the log file".

use std::collections::VecDeque;
use std::time::Instant;

use super::PluginDiagnostic;

/// Default capacity if `with_capacity` is not used.
pub const DEFAULT_DIAGNOSTIC_HISTORY_CAPACITY: usize = 500;

#[derive(Clone, Debug)]
pub struct DiagnosticHistoryEntry {
    pub recorded_at: Instant,
    pub diagnostic: PluginDiagnostic,
    pub seq: u64,
}

#[derive(Clone, Debug)]
pub struct DiagnosticHistory {
    entries: VecDeque<DiagnosticHistoryEntry>,
    capacity: usize,
    next_seq: u64,
    truncated: u64,
}

impl DiagnosticHistory {
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
            next_seq: 0,
            truncated: 0,
        }
    }

    pub fn record(&mut self, diagnostics: &[PluginDiagnostic]) {
        self.record_at(diagnostics, Instant::now());
    }

    pub fn record_at(&mut self, diagnostics: &[PluginDiagnostic], now: Instant) {
        for diagnostic in diagnostics {
            let seq = self.next_seq;
            self.next_seq = self.next_seq.saturating_add(1);
            if self.entries.len() == self.capacity {
                self.entries.pop_front();
                self.truncated = self.truncated.saturating_add(1);
            }
            self.entries.push_back(DiagnosticHistoryEntry {
                recorded_at: now,
                diagnostic: diagnostic.clone(),
                seq,
            });
        }
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &DiagnosticHistoryEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn truncated_count(&self) -> u64 {
        self.truncated
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.truncated = 0;
    }
}

impl Default for DiagnosticHistory {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_DIAGNOSTIC_HISTORY_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginId;
    use crate::plugin::diagnostics::PluginDiagnostic;

    fn err(name: &str) -> PluginDiagnostic {
        PluginDiagnostic::instantiation_failed(PluginId(name.into()), "boom")
    }

    #[test]
    fn empty_by_default() {
        let h = DiagnosticHistory::default();
        assert_eq!(h.len(), 0);
        assert!(h.is_empty());
        assert_eq!(h.truncated_count(), 0);
        assert_eq!(h.capacity(), DEFAULT_DIAGNOSTIC_HISTORY_CAPACITY);
    }

    #[test]
    fn records_diagnostics() {
        let mut h = DiagnosticHistory::with_capacity(10);
        h.record(&[err("a"), err("b")]);
        assert_eq!(h.len(), 2);
        let seqs: Vec<_> = h.entries().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1]);
    }

    #[test]
    fn evicts_oldest_when_capacity_exceeded() {
        let mut h = DiagnosticHistory::with_capacity(2);
        h.record(&[err("a"), err("b"), err("c")]);
        assert_eq!(h.len(), 2);
        let names: Vec<String> = h
            .entries()
            .map(|e| e.diagnostic.plugin_id().unwrap().as_str().to_string())
            .collect();
        assert_eq!(names, vec!["b", "c"]);
        assert_eq!(h.truncated_count(), 1);
    }

    #[test]
    fn seq_is_strictly_increasing_across_records() {
        let mut h = DiagnosticHistory::with_capacity(10);
        h.record(&[err("a")]);
        h.record(&[err("b"), err("c")]);
        let seqs: Vec<_> = h.entries().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2]);
    }

    #[test]
    fn empty_record_is_noop() {
        let mut h = DiagnosticHistory::with_capacity(10);
        h.record(&[]);
        assert_eq!(h.len(), 0);
        assert_eq!(h.truncated_count(), 0);
    }

    #[test]
    fn capacity_zero_is_promoted_to_one() {
        let mut h = DiagnosticHistory::with_capacity(0);
        assert_eq!(h.capacity(), 1);
        h.record(&[err("a"), err("b")]);
        assert_eq!(h.len(), 1);
        assert_eq!(h.truncated_count(), 1);
    }

    #[test]
    fn clear_resets_entries_but_keeps_seq_progression() {
        let mut h = DiagnosticHistory::with_capacity(10);
        h.record(&[err("a"), err("b")]);
        h.clear();
        assert_eq!(h.len(), 0);
        assert_eq!(h.truncated_count(), 0);
        h.record(&[err("c")]);
        let seqs: Vec<_> = h.entries().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![2]);
    }
}
