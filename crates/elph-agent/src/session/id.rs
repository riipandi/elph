//! Session and entry ID generation with optional prefixed Kalid support.
//!
//! Unprefixed IDs (16-char Kalid) are used for session IDs and entry IDs.
//! Prefixed IDs (`goal_<16>`, `msg_<16>`, `todo_<16>`, `skc_<16>`) are used
//! for goals, messages, todos, and skill cache entries respectively.

use std::cell::RefCell;
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use kalid::Kalid;

use crate::session::types::SessionTreeEntry;

thread_local! {
    static LAST_KALID: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Known prefixes for entity IDs.
const GOAL_PREFIX: &str = "goal";
const MESSAGE_PREFIX: &str = "msg";
const TODO_PREFIX: &str = "todo";
const SKILL_CACHE_PREFIX: &str = "skc";

/// K-sortable ID string (16-char Kalid, no prefix).
pub fn create_kalid() -> String {
    next_unique_kalid()
}

pub fn generate_entry_id(by_id: &HashMap<String, SessionTreeEntry>) -> String {
    for _ in 0..100 {
        let id = next_unique_kalid();
        if !by_id.contains_key(&id) {
            return id;
        }
        thread::sleep(Duration::from_millis(1));
    }
    next_unique_kalid()
}

pub fn generate_session_id() -> String {
    next_unique_kalid()
}

/// Create a prefixed Kalid string: `{prefix}_<16-char body>`.
///
/// The body is a unique, K-sortable 16-char Kalid.
pub fn create_prefixed_kalid(prefix: &str) -> String {
    format!("{}_{}", prefix, next_unique_kalid())
}

/// Create a goal ID (`goal_<16>`).
pub fn create_goal_id() -> String {
    create_prefixed_kalid(GOAL_PREFIX)
}

/// Create a message ID (`msg_<16>`).
pub fn create_message_id() -> String {
    create_prefixed_kalid(MESSAGE_PREFIX)
}

/// Create a todo ID (`todo_<16>`).
pub fn create_todo_id() -> String {
    create_prefixed_kalid(TODO_PREFIX)
}

/// Create a skill cache ID (`skc_<16>`).
pub fn create_skill_cache_id() -> String {
    create_prefixed_kalid(SKILL_CACHE_PREFIX)
}

/// Returns `true` when `id` is a valid Kalid string, with or without prefix.
///
/// Accepts:
/// - Unprefixed 16-char Kalid (e.g. `a1b2c3d4e5f6g7h8`)
/// - Prefixed Kalid with a known prefix + `_` separator + 16-char body
///   (e.g. `goal_a1b2c3d4e5f6g7h8`)
pub fn is_valid_kalid(id: &str) -> bool {
    let body = strip_prefix(id).unwrap_or(id);
    body.len() == 16 && Kalid::parse(body).is_ok()
}

/// Strip a known prefix and separator from a Kalid string, returning the 16-char body.
///
/// Returns `None` if no known prefix is found.
fn strip_prefix(id: &str) -> Option<&str> {
    let underscore = id.find('_')?;
    let prefix = &id[..underscore];
    // Only strip known prefixes to avoid false positives
    match prefix {
        GOAL_PREFIX | MESSAGE_PREFIX | TODO_PREFIX | SKILL_CACHE_PREFIX => {
            let body = &id[underscore + 1..];
            Some(body)
        }
        _ => None,
    }
}

fn next_unique_kalid() -> String {
    for _ in 0..100 {
        let id = kalid::generate_kalid();
        let duplicate = LAST_KALID.with(|cell| {
            let mut last = cell.borrow_mut();
            if last.as_deref() == Some(id.as_str()) {
                true
            } else {
                *last = Some(id.clone());
                false
            }
        });
        if !duplicate {
            return id;
        }
        thread::sleep(Duration::from_millis(1));
    }
    kalid::generate_kalid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_kalid_is_16_chars_without_prefix() {
        let id = create_kalid();
        assert_eq!(id.len(), 16);
        assert!(!id.contains('_'));
        assert!(is_valid_kalid(&id));
    }

    #[test]
    fn generate_session_id_is_valid_kalid() {
        assert!(is_valid_kalid(&generate_session_id()));
    }

    #[test]
    fn rapid_create_kalid_produces_distinct_ids() {
        let ids: std::collections::HashSet<String> = (0..8).map(|_| create_kalid()).collect();
        assert_eq!(ids.len(), 8);
    }

    #[test]
    fn create_goal_id_has_prefix() {
        let id = create_goal_id();
        assert!(id.starts_with("goal_"), "goal_ prefix");
        assert_eq!(id.len(), 21); // "goal_" (5) + 16 body
        assert!(is_valid_kalid(&id));
    }

    #[test]
    fn create_message_id_has_prefix() {
        let id = create_message_id();
        assert!(id.starts_with("msg_"), "msg_ prefix");
        assert_eq!(id.len(), 20); // "msg_" (4) + 16 body
        assert!(is_valid_kalid(&id));
    }

    #[test]
    fn create_todo_id_has_prefix() {
        let id = create_todo_id();
        assert!(id.starts_with("todo_"), "todo_ prefix");
        assert_eq!(id.len(), 21); // "todo_" (5) + 16 body
        assert!(is_valid_kalid(&id));
    }

    #[test]
    fn create_skill_cache_id_has_prefix() {
        let id = create_skill_cache_id();
        assert!(id.starts_with("skc_"), "skc_ prefix");
        assert_eq!(id.len(), 20); // "skc_" (4) + 16 body
        assert!(is_valid_kalid(&id));
    }

    #[test]
    fn is_valid_kalid_accepts_unprefixed() {
        let id = create_kalid();
        assert!(is_valid_kalid(&id));
    }

    #[test]
    fn is_valid_kalid_accepts_prefixed() {
        assert!(is_valid_kalid(&create_goal_id()));
        assert!(is_valid_kalid(&create_message_id()));
        assert!(is_valid_kalid(&create_todo_id()));
        assert!(is_valid_kalid(&create_skill_cache_id()));
    }

    #[test]
    fn is_valid_kalid_rejects_invalid() {
        assert!(!is_valid_kalid(""));
        assert!(!is_valid_kalid("short"));
        assert!(!is_valid_kalid("goal_short"));
        assert!(!is_valid_kalid("unknown_a1b2c3d4e5f6g7h8"));
    }

    #[test]
    fn is_valid_kalid_rejects_unknown_prefix() {
        // Unknown prefix should be treated as unprefixed → invalid (not 16 chars)
        assert!(!is_valid_kalid("unknown_a1b2c3d4e5f6g7h8"));
    }

    #[test]
    fn rapid_prefixed_ids_produce_distinct_ids() {
        let ids: std::collections::HashSet<String> = (0..8).map(|_| create_goal_id()).collect();
        assert_eq!(ids.len(), 8);
    }
}
