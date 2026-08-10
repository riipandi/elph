//! Work tracker for enforcing honest todo progress.
//!
//! The tracker maintains a monotonically increasing counter that increments
//! whenever a mutating tool call succeeds. When the agent marks a todo item
//! `in_progress`, we snapshot the current counter under that item's id. When it
//! later marks that item `completed`, we verify the counter has advanced —
//! otherwise the agent is claiming progress without doing real work, and we
//! reject the transition.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Shared work-tracker handle.
///
/// Clone is cheap (Arc). The counter advances only on successful mutating
/// tool calls (edit_file, write_file, shell_exec, delete_path, etc.).
#[derive(Clone)]
pub struct WorkTracker {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    counter: AtomicU64,
    // Snapshots of the counter taken when an item is marked `in_progress`.
    // Keyed by todo id. When the item is marked `completed`, we check the
    // counter has advanced past this snapshot.
    snapshots: Mutex<HashMap<String, u64>>,
}

impl WorkTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner::default()),
        }
    }

    /// Record that a unit of actual work was done.
    pub fn record_work(&self) {
        self.inner.counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Current work count (monotonically non-decreasing).
    pub fn current(&self) -> u64 {
        self.inner.counter.load(Ordering::Relaxed)
    }

    /// Snapshot the current counter under `item_id` (call when marking `in_progress`).
    pub fn snapshot_in_progress(&self, item_id: &str) {
        let mut snaps = self.inner.snapshots.lock().expect("snapshots lock");
        snaps.insert(item_id.to_string(), self.current());
    }

    /// Returns `true` if at least one work unit was recorded since the snapshot
    /// taken when `item_id` was marked `in_progress`.
    ///
    /// If no snapshot exists for the item (e.g. `completed` without ever being
    /// `in_progress`, or first run after restart), returns `false` to force the
    // ! agent to actually do work before claiming completion.
    pub fn has_work_since_snapshot(&self, item_id: &str) -> bool {
        let snaps = self.inner.snapshots.lock().expect("snapshots lock");
        match snaps.get(item_id) {
            Some(token) => self.current() > *token,
            None => false,
        }
    }

    /// Remove a snapshot (call when an item is cancelled or reset to pending).
    pub fn clear_snapshot(&self, item_id: &str) {
        let mut snaps = self.inner.snapshots.lock().expect("snapshots lock");
        snaps.remove(item_id);
    }
}

impl Default for WorkTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_starts_at_zero() {
        let t = WorkTracker::new();
        assert_eq!(t.current(), 0);
        assert!(!t.has_work_since_snapshot("1"));
    }

    #[test]
    fn record_advances_and_detects_work() {
        let t = WorkTracker::new();
        t.snapshot_in_progress("1");
        assert!(!t.has_work_since_snapshot("1"));
        t.record_work();
        assert!(t.has_work_since_snapshot("1"));
    }

    #[test]
    fn no_snapshot_rejects_completion() {
        let t = WorkTracker::new();
        t.record_work();
        // No snapshot taken → can't prove work was done for this item.
        assert!(!t.has_work_since_snapshot("1"));
    }

    #[test]
    fn clone_shares_counter() {
        let a = WorkTracker::new();
        let b = a.clone();
        a.record_work();
        assert_eq!(b.current(), 1);
    }

    #[test]
    fn per_item_isolation() {
        let t = WorkTracker::new();
        t.snapshot_in_progress("1");
        t.snapshot_in_progress("2");
        t.record_work(); // work done
        assert!(t.has_work_since_snapshot("1"));
        assert!(t.has_work_since_snapshot("2"));
        t.clear_snapshot("1");
        assert!(!t.has_work_since_snapshot("1"));
    }
}
