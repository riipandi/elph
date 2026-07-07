//! Session ID generation tests.

use elph_agent::session::id::generate_session_id;
use elph_agent::{AgentMessage, convert_to_llm, default_convert_to_llm, uuidv7};
use elph_ai::{Message, UserContent};
use uuid::{Uuid, Version};

#[test]
fn generate_session_id_produces_valid_uuid_v7() {
    let id = generate_session_id();
    let parsed = Uuid::parse_str(&id).expect("valid uuid string");
    assert_eq!(parsed.get_version(), Some(Version::SortRand));
    assert_eq!(parsed.as_bytes().len(), 16);
}

#[test]
fn generate_session_id_is_monotonically_ordered() {
    let ids: Vec<String> = (0..20).map(|_| generate_session_id()).collect();
    let uuids: Vec<Uuid> = ids.iter().map(|id| Uuid::parse_str(id).expect("valid uuid")).collect();

    for window in uuids.windows(2) {
        assert!(
            window[0] <= window[1],
            "expected monotonic ordering, got {} then {}",
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
fn uuidv7_matches_generate_session_id_format() {
    let id = uuidv7();
    let parsed = Uuid::parse_str(&id).expect("valid uuid string");
    assert_eq!(parsed.get_version(), Some(Version::SortRand));
    assert_eq!(parsed.as_bytes().len(), 16);
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
