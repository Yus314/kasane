//! Re-entrancy / cycle / depth guard for `PluginView::paint_inline_box`.
//!
//! A plugin's `paint_inline_box(outer)` may produce an `Element` tree that
//! contains another inline-box, which the host then resolves via a fresh
//! `paint_inline_box(inner)` call. Without bounds, buggy or hostile plugins
//! can blow the host stack via self-cycles (`inner == outer`) or mutual
//! cycles (plugin A → box_B → plugin B → box_A → …). The host enforces
//! both depth and cycle bounds; plugins are not trusted.
//!
//! ADR-031 Phase 10 Step 2 — see "再帰深さ + 循環検出".

use std::cell::RefCell;

use super::PluginId;

/// Maximum nesting depth for `paint_inline_box` reentrancy. Eight is
/// chosen as a generous practical bound — real plugin authors using
/// inline boxes recursively (e.g. nested code-fold previews) are
/// unlikely to need more, while the bound is small enough to keep host
/// stack usage trivial.
pub(super) const MAX_INLINE_BOX_DEPTH: usize = 8;

thread_local! {
    /// Per-thread stack tracking the chain of `box_id`s currently being
    /// painted. Used by [`PluginView::paint_inline_box`](super::registry)
    /// to detect self-cycles, mutual cycles between plugins, and runaway
    /// nesting. `Vec` (not `HashSet`) because the depth bound is small
    /// and linear scan beats hashing at this scale; ordering also lets
    /// us emit useful "owner=X box_id=Y" diagnostics.
    static INLINE_BOX_STACK: RefCell<InlineBoxStackInner> =
        RefCell::new(InlineBoxStackInner::default());
}

#[derive(Default)]
pub(super) struct InlineBoxStackInner {
    /// Active painting chain. Newest at the back.
    pub(super) chain: Vec<u64>,
    /// `(plugin_id, box_id)` pairs that have already emitted an error
    /// log this process. Prevents log-volume blow-up when a buggy
    /// plugin re-enters the cycle path every frame.
    pub(super) logged_overflow: std::collections::HashSet<(String, u64)>,
    pub(super) logged_cycle: std::collections::HashSet<(String, u64)>,
}

pub(super) struct InlineBoxStack;

impl InlineBoxStack {
    pub(super) fn with<R>(f: impl FnOnce(&mut InlineBoxStackInner) -> R) -> R {
        INLINE_BOX_STACK.with(|cell| f(&mut cell.borrow_mut()))
    }
}

impl InlineBoxStackInner {
    pub(super) fn depth(&self) -> usize {
        self.chain.len()
    }

    pub(super) fn contains(&self, box_id: u64) -> bool {
        self.chain.contains(&box_id)
    }

    pub(super) fn push(&mut self, box_id: u64) {
        self.chain.push(box_id);
    }

    pub(super) fn pop(&mut self) {
        self.chain.pop();
    }

    pub(super) fn log_overflow_once(&mut self, owner: &PluginId, box_id: u64) {
        let key = (owner.0.clone(), box_id);
        if self.logged_overflow.insert(key) {
            tracing::error!(
                target: "kasane::plugin::inline_box",
                plugin = %owner.0,
                box_id,
                depth = self.chain.len(),
                limit = MAX_INLINE_BOX_DEPTH,
                "paint_inline_box exceeded recursion depth limit; falling back to empty paint"
            );
        }
    }

    pub(super) fn log_cycle_once(&mut self, owner: &PluginId, box_id: u64) {
        let key = (owner.0.clone(), box_id);
        if self.logged_cycle.insert(key) {
            tracing::error!(
                target: "kasane::plugin::inline_box",
                plugin = %owner.0,
                box_id,
                chain = ?self.chain,
                "paint_inline_box cycle detected; falling back to empty paint"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    //! Verify the recursion-depth and cycle guards. These tests directly
    //! exercise [`InlineBoxStackInner`] (the thread-local guard state)
    //! since constructing a full `PluginView` with mock plugins for the
    //! same purpose adds significant fixture overhead without testing
    //! anything beyond the guard logic itself.

    use super::*;

    fn fresh_stack() -> InlineBoxStackInner {
        InlineBoxStackInner::default()
    }

    #[test]
    fn depth_under_limit_admits_paint() {
        let mut stack = fresh_stack();
        for i in 0..MAX_INLINE_BOX_DEPTH {
            assert!(stack.depth() < MAX_INLINE_BOX_DEPTH);
            stack.push(i as u64);
        }
        assert_eq!(stack.depth(), MAX_INLINE_BOX_DEPTH);
    }

    #[test]
    fn depth_at_limit_blocks_further_paint() {
        let mut stack = fresh_stack();
        for i in 0..MAX_INLINE_BOX_DEPTH {
            stack.push(i as u64);
        }
        // The guard rejects when depth >= MAX, before push.
        assert_eq!(stack.depth(), MAX_INLINE_BOX_DEPTH);
        assert!(stack.depth() >= MAX_INLINE_BOX_DEPTH);
    }

    #[test]
    fn self_cycle_detected() {
        let mut stack = fresh_stack();
        stack.push(42);
        assert!(stack.contains(42));
    }

    #[test]
    fn mutual_cycle_detected() {
        // A → box_B → B → box_A → A
        let mut stack = fresh_stack();
        stack.push(0xA);
        stack.push(0xB);
        // The next paint_inline_box(0xA) would re-enter box A.
        assert!(stack.contains(0xA));
    }

    #[test]
    fn pop_restores_depth() {
        let mut stack = fresh_stack();
        stack.push(1);
        stack.push(2);
        stack.pop();
        assert_eq!(stack.depth(), 1);
        assert!(!stack.contains(2));
        assert!(stack.contains(1));
    }

    #[test]
    fn overflow_log_dedupes_per_owner_box_pair() {
        let mut stack = fresh_stack();
        let owner = PluginId("plugin_a".to_string());
        // First call records the entry.
        stack.log_overflow_once(&owner, 100);
        let after_first = stack.logged_overflow.len();
        // Second call with the same key must not grow the set.
        stack.log_overflow_once(&owner, 100);
        assert_eq!(stack.logged_overflow.len(), after_first);
        // Different box_id should add a new entry.
        stack.log_overflow_once(&owner, 200);
        assert_eq!(stack.logged_overflow.len(), after_first + 1);
    }

    #[test]
    fn cycle_log_dedupes_per_owner_box_pair() {
        let mut stack = fresh_stack();
        let owner = PluginId("plugin_b".to_string());
        stack.log_cycle_once(&owner, 7);
        let after_first = stack.logged_cycle.len();
        stack.log_cycle_once(&owner, 7);
        assert_eq!(stack.logged_cycle.len(), after_first);
    }
}
