//! Shared in-memory index helpers for session storage backends.

use std::collections::HashMap;

use crate::session::id::generate_entry_id;
use crate::session::types::{SessionError, SessionErrorCode, SessionIndex, SessionTreeEntry};

pub fn build_labels_by_id(entries: &[SessionTreeEntry]) -> HashMap<String, String> {
    let mut labels_by_id = HashMap::new();
    for entry in entries {
        update_label_cache(&mut labels_by_id, entry);
    }
    labels_by_id
}

pub fn update_label_cache(labels_by_id: &mut HashMap<String, String>, entry: &SessionTreeEntry) {
    let SessionTreeEntry::Label { target_id, label, .. } = entry else {
        return;
    };
    if let Some(label) = label.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        labels_by_id.insert(target_id.clone(), label.to_string());
    } else {
        labels_by_id.remove(target_id);
    }
}

pub fn leaf_id_after_entry(entry: &SessionTreeEntry) -> Option<String> {
    match entry {
        SessionTreeEntry::Leaf { target_id, .. } => target_id.clone(),
        other => Some(other.id().to_string()),
    }
}

/// Resolve the persisted leaf to a real entry, tolerating stale pointers.
///
/// Preference order:
/// 1. The persisted leaf pointer when it names an entry in `by_id`.
/// 2. The last `Leaf { target_id }` entry whose target exists.
/// 3. The newest entry (regardless of type) whose target exists.
/// 4. `None` (empty session) — never an error at construction time.
///
/// A stale pointer can happen when a crash lands between writing a `leaf`
/// entry and its new child, after snapshot pruning removed rows, or after
/// partial recovery writes. Falling back — instead of failing the whole
/// session open with `InvalidSession` — lets `reconcile_session` regenerate a
/// coherent tree from whatever entries remain.
pub fn resolve_leaf_id(
    entries: &[SessionTreeEntry],
    persisted_leaf: Option<&str>,
) -> Result<Option<String>, SessionError> {
    if let Some(id) = persisted_leaf
        && !id.is_empty()
        && entries.iter().any(|e| e.id() == id)
    {
        return Ok(Some(id.to_string()));
    }

    // Last explicit Leaf whose target still exists (forward in tree order).
    for entry in entries.iter().rev() {
        if let SessionTreeEntry::Leaf {
            target_id: Some(target),
            ..
        } = entry
            && entries.iter().any(|e| e.id() == target)
        {
            return Ok(Some(target.clone()));
        }
    }

    // Newest entry that can serve as a leaf. Explicit Leaf entries are skipped —
    // step 2 already used the last *valid* explicit leaf, and any remaining Leaf
    // is stale (target missing) or a move-to-root marker, neither of which makes
    // a good leaf. Non-leaf entries always become the leaf when appended.
    for entry in entries.iter().rev() {
        if matches!(entry, SessionTreeEntry::Leaf { .. }) {
            continue;
        }
        return Ok(Some(entry.id().to_string()));
    }

    Ok(None)
}

pub fn build_index(entries: Vec<SessionTreeEntry>, leaf_id: Option<String>) -> Result<SessionIndex, SessionError> {
    let by_id: HashMap<String, SessionTreeEntry> = entries
        .iter()
        .map(|entry| (entry.id().to_string(), entry.clone()))
        .collect();
    let labels_by_id = build_labels_by_id(&entries);

    // Resolve the leaf against real entries instead of failing open when the
    // pointer is missing (crash between leaf write + child write, rows pruned,
    // partial recovery). `reconcile_session` regenerates a coherent tree from
    // whatever entries remain.
    let resolved_leaf = resolve_leaf_id(&entries, leaf_id.as_deref())?;

    Ok(SessionIndex {
        entries,
        by_id,
        labels_by_id,
        leaf_id: resolved_leaf,
        checkpoints: HashMap::new(),
        name: None,
    })
}

/// Compute session statistics from an index.
pub fn compute_statistics(index: &SessionIndex) -> crate::session::types::SessionStatistics {
    let total_entries = index.entries.len() as u64;
    let mut message_count = 0u64;
    let mut compaction_count = 0u64;
    let mut branch_summary_count = 0u64;
    for entry in &index.entries {
        match entry.entry_type() {
            "message" => message_count += 1,
            "compaction" => compaction_count += 1,
            "branch_summary" => branch_summary_count += 1,
            _ => {}
        }
    }
    crate::session::types::SessionStatistics {
        total_entries,
        message_count,
        compaction_count,
        branch_summary_count,
        approximate_tokens: 0,
        name: index.name.clone(),
    }
}

/// Count `Leaf` entries whose target no longer exists in the index.
///
/// A large count means the tree was written by crashes / partial recovery and
/// is worth auto-healing on open (see `maybe_heal_stale_leaves`).
pub fn stale_leaf_count(index: &SessionIndex) -> usize {
    index.entries.iter().fold(0, |mut count, entry| {
        let is_stale = match entry {
            SessionTreeEntry::Leaf {
                target_id: Some(target),
                ..
            } => !index.by_id.contains_key(target),
            _ => false,
        };
        if is_stale {
            count += 1;
        }
        count
    })
}

/// Get path to root or stop at the nearest compaction boundary.
/// Returns entries from the compaction boundary (or root) up to the leaf.
pub fn get_path_to_root_or_compaction(
    by_id: &HashMap<String, SessionTreeEntry>,
    leaf_id: Option<&str>,
) -> Result<Vec<SessionTreeEntry>, SessionError> {
    walk_path_to(by_id, leaf_id, true)
}

/// Get the full path to the root.
pub fn get_path_to_root(
    by_id: &HashMap<String, SessionTreeEntry>,
    leaf_id: Option<&str>,
) -> Result<Vec<SessionTreeEntry>, SessionError> {
    walk_path_to(by_id, leaf_id, false)
}

/// Shared parent-chain walker.
///
/// Heals broken chains instead of failing the whole session: when an entry's
/// `parent_id` is not in `by_id` (rows deleted, e.g. legacy snapshot pruning, or a
/// non-Kalid parent written by an old store), the path simply stops at that entry
/// — the remaining entries stay readable and the agent can keep appending.
///
/// `stop_at_compaction`: stop at `SessionTreeEntry::Compaction` (self-contained
/// checkpoint tail) instead of walking to the root.
fn walk_path_to(
    by_id: &HashMap<String, SessionTreeEntry>,
    leaf_id: Option<&str>,
    stop_at_compaction: bool,
) -> Result<Vec<SessionTreeEntry>, SessionError> {
    let Some(leaf_id) = leaf_id else {
        return Ok(Vec::new());
    };
    let mut path = Vec::new();
    let mut current = by_id
        .get(leaf_id)
        .ok_or_else(|| SessionError::new(SessionErrorCode::InvalidEntry, format!("Entry {leaf_id} not found")))?;
    loop {
        path.insert(0, current.clone());
        if stop_at_compaction && matches!(current, SessionTreeEntry::Compaction { .. }) {
            break;
        }
        let Some(parent_id) = current.parent_id() else {
            break;
        };
        let Some(parent) = by_id.get(parent_id) else {
            // Parent entry missing — stop walking instead of erroring. The root of
            // the survived path is the existing entry whose parent is unknown.
            break;
        };
        current = parent;
    }
    Ok(path)
}

/// Get entries after a cursor position (exclusive `after_id`).
pub fn get_entries_cursor(
    entries: &[SessionTreeEntry],
    cursor: &crate::session::types::CursorPosition,
) -> Result<Vec<SessionTreeEntry>, SessionError> {
    let after_idx = entries.iter().position(|e| e.id() == cursor.after_id).ok_or_else(|| {
        SessionError::new(
            SessionErrorCode::NotFound,
            format!("Cursor entry {} not found", cursor.after_id),
        )
    })?;
    let limit = cursor.limit as usize;
    let start = after_idx + 1;
    let end = std::cmp::min(start + limit, entries.len());
    Ok(entries[start..end].to_vec())
}

pub fn append_to_index(index: &mut SessionIndex, entry: SessionTreeEntry) {
    update_label_cache(&mut index.labels_by_id, &entry);
    // Track the effective leaf:
    // - Non-leaf entries make themselves the new leaf (their id always exists).
    // - `Leaf { target_id: None }` moves the leaf to root (Nothing).
    // - A `Leaf` pointing at a missing target (crash ordering, partial recovery)
    //   is ignored instead of poisoning the index with a phantom leaf.
    let next_leaf = match &entry {
        SessionTreeEntry::Leaf { target_id, .. } => target_id.clone(),
        _ => Some(entry.id().to_string()),
    };
    match &next_leaf {
        Some(target) if index.by_id.contains_key(target) || *target == *entry.id() => {
            index.leaf_id = Some(target.clone());
        }
        None => {
            index.leaf_id = None;
        }
        // Stale Leaf target is not in the index yet → keep the current leaf.
        Some(_) => {}
    }
    index.by_id.insert(entry.id().to_string(), entry.clone());
    index.entries.push(entry);
}

pub fn create_leaf_entry(
    parent_id: Option<String>,
    target_id: Option<String>,
    by_id: &HashMap<String, SessionTreeEntry>,
) -> SessionTreeEntry {
    SessionTreeEntry::Leaf {
        id: generate_entry_id(by_id),
        parent_id,
        timestamp: crate::messages::now_iso_timestamp(),
        target_id,
    }
}

pub fn find_entries(entries: &[SessionTreeEntry], entry_type: &str) -> Vec<SessionTreeEntry> {
    entries
        .iter()
        .filter(|entry| entry.entry_type() == entry_type)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionTreeEntry;
    use crate::types::AgentMessage;

    fn message_entry(id: &str, parent_id: Option<&str>) -> SessionTreeEntry {
        use elph_ai::{Message, UserContent};
        SessionTreeEntry::Message {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: "t".into(),
            message: AgentMessage::Llm(Box::new(Message::User {
                content: UserContent::Text("hi".into()),
                timestamp: 0,
            })),
            prompt_title: String::new(),
            prompt_kind: String::new(),
        }
    }

    fn leaf_entry(id: &str, parent_id: Option<&str>, target: Option<&str>) -> SessionTreeEntry {
        SessionTreeEntry::Leaf {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: "t".into(),
            target_id: target.map(str::to_string),
        }
    }

    #[test]
    fn build_index_resolves_stale_leaf_to_real_entry() {
        let entries = vec![message_entry("a", None), leaf_entry("l1", Some("a"), Some("ghost"))];
        // Persisted pointer is missing entirely.
        let index = build_index(entries, Some("ghost".into())).expect("build");
        // Falls back to the newest real entry — session stays openable.
        assert_eq!(index.leaf_id.as_deref(), Some("a"));
    }

    #[test]
    fn build_index_keeps_valid_persisted_leaf() {
        let entries = vec![message_entry("a", None)];
        let index = build_index(entries, Some("a".into())).expect("build");
        assert_eq!(index.leaf_id.as_deref(), Some("a"));
    }

    #[test]
    fn build_index_empty_session_is_none_not_error() {
        assert!(build_index(Vec::new(), None).expect("empty").leaf_id.is_none());
        // A phantom pointer on an empty tree also resolves to None, not error.
        assert!(
            build_index(Vec::new(), Some("x".into()))
                .expect("phantom")
                .leaf_id
                .is_none()
        );
    }

    #[test]
    fn walk_stops_at_broken_parent_chain() {
        let by_id: HashMap<String, SessionTreeEntry> = [message_entry("leaf", Some("ghost-parent"))]
            .into_iter()
            .map(|e| (e.id().to_string(), e))
            .collect();
        let path = get_path_to_root(&by_id, Some("leaf")).expect("walk");
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].id(), "leaf");

        let path = get_path_to_root_or_compaction(&by_id, Some("leaf")).expect("walk compact");
        assert_eq!(path.len(), 1);
    }

    #[test]
    fn walk_stops_at_compaction_boundary() {
        let by_id: HashMap<String, SessionTreeEntry> = [
            message_entry("root", None),
            SessionTreeEntry::Compaction {
                id: "c1".into(),
                parent_id: Some("root".into()),
                timestamp: "t".into(),
                summary: "summary".into(),
                first_kept_entry_id: "root".into(),
                tokens_before: 10,
                details: None,
                from_hook: None,
            },
            message_entry("after", Some("c1")),
        ]
        .into_iter()
        .map(|e| (e.id().to_string(), e))
        .collect();

        let path = get_path_to_root_or_compaction(&by_id, Some("after")).expect("walk");
        let ids: Vec<_> = path.iter().map(SessionTreeEntry::id).collect();
        assert_eq!(ids, vec!["c1", "after"]);
    }

    #[test]
    fn append_to_index_ignores_phantom_leaf_target() {
        let mut index = build_index(vec![message_entry("a", None)], Some("a".into())).expect("index");
        append_to_index(&mut index, leaf_entry("l1", Some("a"), Some("nope")));
        assert_eq!(index.leaf_id.as_deref(), Some("a"));
        // And Leaf with no target (move to root) clears the leaf.
        append_to_index(&mut index, leaf_entry("l2", Some("l1"), None));
        assert!(index.leaf_id.is_none());
    }

    #[test]
    fn stale_leaf_count_counts_only_phantoms() {
        let mut index = build_index(vec![message_entry("a", None)], Some("a".into())).expect("index");
        append_to_index(&mut index, leaf_entry("l1", Some("a"), Some("bad")));
        append_to_index(&mut index, leaf_entry("l2", Some("a"), Some("a")));
        assert_eq!(stale_leaf_count(&index), 1);
    }
}
