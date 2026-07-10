//! Short session entry ID generation (elph-compatible).

use std::collections::HashMap;

use tsid::TSID;

use crate::session::types::SessionTreeEntry;

/// Time-sortable ID string (13-char TSID).
pub fn create_tsid() -> String {
    tsid::create_tsid().to_string()
}

pub fn generate_entry_id(by_id: &HashMap<String, SessionTreeEntry>) -> String {
    for _ in 0..100 {
        let id = create_tsid();
        if !by_id.contains_key(&id) {
            return id;
        }
    }
    create_tsid()
}

pub fn generate_session_id() -> String {
    create_tsid()
}

/// Returns true when `id` is a valid TSID string.
pub fn is_valid_tsid(id: &str) -> bool {
    id.len() == 13 && TSID::try_from(id).is_ok()
}
