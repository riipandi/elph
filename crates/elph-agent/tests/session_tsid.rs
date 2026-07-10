//! Session ID generation tests.

use elph_agent::session::id::{create_tsid, generate_session_id, is_valid_tsid};
use elph_agent::{AgentMessage, convert_to_llm, default_convert_to_llm};
use elph_ai::{Message, UserContent};
use tsid::TSID;

#[test]
fn generate_session_id_produces_valid_tsid() {
    let id = generate_session_id();
    assert!(is_valid_tsid(&id));
}

#[test]
fn generate_session_id_is_monotonically_ordered() {
    let ids: Vec<TSID> = (0..20)
        .map(|_| TSID::try_from(generate_session_id().as_str()).expect("valid tsid"))
        .collect();

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
fn create_tsid_matches_generate_session_id_format() {
    let id = create_tsid();
    assert!(is_valid_tsid(&id));
    assert!(is_valid_tsid(&generate_session_id()));
}

#[test]
fn convert_to_llm_is_default_convert_to_llm_alias() {
    let messages = vec![AgentMessage::Llm(Box::new(Message::User {
        content: UserContent::Text("hello".into()),
        timestamp: 0,
    }))];
    let aliased = convert_to_llm(messages.clone());
    let default = default_convert_to_llm(messages);
    assert_eq!(aliased.len(), default.len());
    assert_eq!(aliased[0].role(), default[0].role());
    match (&aliased[0], &default[0]) {
        (Message::User { content: a, .. }, Message::User { content: b, .. }) => {
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        (a, b) => panic!("expected matching user messages, got {a:?} and {b:?}"),
    }
}
