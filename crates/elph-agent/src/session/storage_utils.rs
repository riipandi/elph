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

pub fn build_index(entries: Vec<SessionTreeEntry>, leaf_id: Option<String>) -> Result<SessionIndex, SessionError> {
    let by_id: HashMap<String, SessionTreeEntry> = entries
        .iter()
        .map(|entry| (entry.id().to_string(), entry.clone()))
        .collect();
    let labels_by_id = build_labels_by_id(&entries);
    let mut resolved_leaf = leaf_id;
    if resolved_leaf.is_none() {
        for entry in &entries {
            resolved_leaf = leaf_id_after_entry(entry);
        }
    }
    if let Some(leaf) = &resolved_leaf
        && !by_id.contains_key(leaf)
    {
        return Err(SessionError::new(
            SessionErrorCode::InvalidSession,
            format!("Entry {leaf} not found"),
        ));
    }
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

/// Get path to root or stop at the nearest compaction boundary.
/// Returns entries from the compaction boundary (or root) up to the leaf.
pub fn get_path_to_root_or_compaction(
    by_id: &HashMap<String, SessionTreeEntry>,
    leaf_id: Option<&str>,
) -> Result<Vec<SessionTreeEntry>, SessionError> {
    let Some(leaf_id) = leaf_id else {
        return Ok(Vec::new());
    };
    let mut path = Vec::new();
    let mut current = by_id
        .get(leaf_id)
        .ok_or_else(|| SessionError::new(SessionErrorCode::NotFound, format!("Entry {leaf_id} not found")))?;
    loop {
        path.insert(0, current.clone());
        // Stop at compaction boundary — we have a self-contained checkpoint tail.
        if matches!(current, SessionTreeEntry::Compaction { .. }) {
            break;
        }
        let Some(parent_id) = current.parent_id() else {
            break;
        };
        current = by_id.get(parent_id).ok_or_else(|| {
            SessionError::new(SessionErrorCode::InvalidSession, format!("Entry {parent_id} not found"))
        })?;
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
    index.leaf_id = leaf_id_after_entry(&entry);
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

pub fn get_path_to_root(
    by_id: &HashMap<String, SessionTreeEntry>,
    leaf_id: Option<&str>,
) -> Result<Vec<SessionTreeEntry>, SessionError> {
    let Some(leaf_id) = leaf_id else {
        return Ok(Vec::new());
    };
    let mut path = Vec::new();
    let mut current = by_id
        .get(leaf_id)
        .ok_or_else(|| SessionError::new(SessionErrorCode::NotFound, format!("Entry {leaf_id} not found")))?;
    loop {
        path.insert(0, current.clone());
        let Some(parent_id) = current.parent_id() else {
            break;
        };
        current = by_id.get(parent_id).ok_or_else(|| {
            SessionError::new(SessionErrorCode::InvalidSession, format!("Entry {parent_id} not found"))
        })?;
    }
    Ok(path)
}

pub fn find_entries(entries: &[SessionTreeEntry], entry_type: &str) -> Vec<SessionTreeEntry> {
    entries
        .iter()
        .filter(|entry| entry.entry_type() == entry_type)
        .cloned()
        .collect()
}
