//! Short session entry ID generation (elph-compatible).

use std::collections::HashMap;

use uuid::Uuid;

use crate::session::types::SessionTreeEntry;

pub fn generate_entry_id(by_id: &HashMap<String, SessionTreeEntry>) -> String {
    for _ in 0..100 {
        let id = Uuid::now_v7().to_string();
        let short = id[id.len().saturating_sub(8)..].to_string();
        if !by_id.contains_key(&short) {
            return short;
        }
    }
    Uuid::now_v7().to_string()
}

pub fn generate_session_id() -> String {
    Uuid::now_v7().to_string()
}

/// UUID v7 string (upstream `uuidv7`).
pub fn uuidv7() -> String {
    Uuid::now_v7().to_string()
}
