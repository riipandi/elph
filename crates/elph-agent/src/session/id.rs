//! Session and entry ID generation with optional prefixed Kalid support.
//!
//! Unprefixed IDs (16-char Kalid) are used for session IDs and entry IDs.
//! Prefixed IDs (`goal_<16>`, `msg_<16>`, `todo_<16>`, `turn_<16>`) are used
//! for goals, messages, todos, and turns respectively.

use std::cell::RefCell;
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use kalid::Kalid;
use memorable_ids::GenerateOptions;
use memorable_ids::generate;

use crate::session::types::SessionTreeEntry;

thread_local! {
    static LAST_KALID: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Known prefixes for entity IDs.
const GOAL_PREFIX: &str = "goal";
const MESSAGE_PREFIX: &str = "msg";
const TODO_PREFIX: &str = "todo";
const TURN_PREFIX: &str = "turn";
const WORKER_PREFIX: &str = "wrk";
const WORKER_MSG_PREFIX: &str = "wmsg";

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

/// Create a turn ID (`turn_<16>`).
pub fn create_turn_id() -> String {
    create_prefixed_kalid(TURN_PREFIX)
}

/// Create a worker ID with memorable name using underscore separator.
///
/// Example: `wrk_quick_fox`
pub fn create_worker_id() -> String {
    let options = GenerateOptions {
        components: 2,
        separator: "_".to_string(),
        suffix: None,
    };
    for _ in 0..100 {
        let core = generate(options.clone()).expect("valid memorable id options");
        let parts: Vec<&str> = core.split('_').collect();
        if parts.len() == 2
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphabetic()))
        {
            return format!("{}_{}", WORKER_PREFIX, core);
        }
        thread::sleep(Duration::from_millis(1));
    }
    // Fallback: use kalid if memorable_ids fails to generate valid core
    create_prefixed_kalid(WORKER_PREFIX)
}

/// Create a worker-message ID (`wmsg_<16>`).
pub fn create_worker_msg_id() -> String {
    create_prefixed_kalid(WORKER_MSG_PREFIX)
}

/// Returns `true` when `id` is a valid Kalid string, with or without prefix.
///
/// Accepts:
/// - Unprefixed 16-char Kalid (e.g. `a1b2c3d4e5f6g7h8`)
/// - Prefixed Kalid with a known prefix + `_` separator + 16-char body
///   (e.g. `goal_a1b2c3d4e5f6g7h8`)
/// - Worker IDs with memorable names (e.g. `wrk_quick_fox`)
pub fn is_valid_kalid(id: &str) -> bool {
    let underscore = id.find('_');
    if let Some(pos) = underscore {
        let prefix = &id[..pos];
        let body = &id[pos + 1..];
        match prefix {
            WORKER_PREFIX => {
                // Worker IDs accept either 16-char body (fallback) or memorable adjective_noun
                if body.len() == 16 && body.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return true;
                }
                // Check for memorable adjective_noun format
                let parts: Vec<&str> = body.split('_').collect();
                if parts.len() == 2
                    && parts
                        .iter()
                        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphabetic()))
                {
                    return true;
                }
                false
            }
            GOAL_PREFIX | MESSAGE_PREFIX | TODO_PREFIX | TURN_PREFIX | WORKER_MSG_PREFIX => {
                body.len() == 16 && Kalid::parse(body).is_ok()
            }
            _ => {
                // Unknown prefix: treat as unprefixed → must be 16-char Kalid
                id.len() == 16 && Kalid::parse(id).is_ok()
            }
        }
    } else {
        // No prefix: must be 16-char Kalid
        id.len() == 16 && Kalid::parse(id).is_ok()
    }
}

/// Strip a known prefix and separator from a Kalid string, returning the body.
///
/// For worker IDs with memorable names (e.g., `wrk_quick_fox`), returns the memorable core.
/// For prefixed Kalids (e.g., `goal_a1b2c3d4e5f6g7h8`), returns the 16-char body.
/// Returns `None` if no known prefix is found.
fn strip_prefix(id: &str) -> Option<&str> {
    let underscore = id.find('_')?;
    let prefix = &id[..underscore];
    // Only strip known prefixes to avoid false positives
    match prefix {
        GOAL_PREFIX | MESSAGE_PREFIX | TODO_PREFIX | TURN_PREFIX | WORKER_PREFIX | WORKER_MSG_PREFIX => {
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
    fn is_valid_kalid_accepts_unprefixed() {
        let id = create_kalid();
        assert!(is_valid_kalid(&id));
    }

    #[test]
    fn is_valid_kalid_accepts_prefixed() {
        assert!(is_valid_kalid(&create_goal_id()));
        assert!(is_valid_kalid(&create_message_id()));
        assert!(is_valid_kalid(&create_todo_id()));
        assert!(is_valid_kalid(&create_turn_id()));
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

    #[test]
    fn create_worker_id_uses_memorable_underscore_format() {
        for _ in 0..32 {
            let id = create_worker_id();
            assert!(id.starts_with("wrk_"), "expected wrk_ prefix, got {id}");
            let core = id.strip_prefix("wrk_").expect("core");
            let parts: Vec<&str> = core.split('_').collect();
            // Either fallback Kalid (16 chars) or memorable adjective_noun (2 parts)
            if parts.len() == 2 {
                assert!(parts[0].chars().all(|c| c.is_ascii_alphabetic()));
                assert!(parts[1].chars().all(|c| c.is_ascii_alphabetic()));
            } else {
                // Fallback to Kalid
                assert_eq!(core.len(), 16);
            }
            assert!(is_valid_kalid(&id));
        }
    }

    #[test]
    fn is_valid_kalid_accepts_memorable_worker_id() {
        assert!(is_valid_kalid("wrk_quick_fox"));
        assert!(is_valid_kalid("wrk_silent_owl"));
        assert!(is_valid_kalid("wrk_a1b2c3d4e5f6g7h8")); // Fallback Kalid
    }
}
