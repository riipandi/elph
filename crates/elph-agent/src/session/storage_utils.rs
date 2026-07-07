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
    })
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
