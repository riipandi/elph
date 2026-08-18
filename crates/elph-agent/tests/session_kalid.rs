//! Session ID generation tests.

use elph_agent::session::id::{create_kalid, generate_session_id, is_valid_kalid};

#[test]
fn generate_session_id_produces_valid_kalid() {
    let id = generate_session_id();
    assert!(is_valid_kalid(&id));
    assert_eq!(id.len(), 16);
    assert!(!id.contains('_'));
}

#[test]
fn generate_session_id_is_monotonically_ordered() {
    let ids: Vec<String> = (0..20).map(|_| generate_session_id()).collect();

    for window in ids.windows(2) {
        assert!(
            window[0] <= window[1],
            "expected monotonic ordering, got {:?} then {:?}",
            window[0],
            window[1]
        );
    }
}

#[test]
fn generate_session_id_produces_unique_values() {
    let ids: std::collections::HashSet<String> = (0..50).map(|_| generate_session_id()).collect();
    assert_eq!(ids.len(), 50);
}

#[test]
fn create_kalid_matches_generate_session_id_format() {
    let id = create_kalid();
    assert!(is_valid_kalid(&id));
    assert!(is_valid_kalid(&generate_session_id()));
}
