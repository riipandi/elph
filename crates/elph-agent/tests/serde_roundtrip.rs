use elph_agent::{AgentMessage, llm_message_to_agent};
use elph_ai::{Message, UserContent, faux_assistant_message, faux_text};

#[test]
fn agent_message_roundtrip_user() {
    let msg = llm_message_to_agent(Message::User {
        content: UserContent::Text("one".into()),
        timestamp: 0,
    });
    let json = serde_json::to_string(&msg).unwrap();
    let back: AgentMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back.role(), "user");
}

#[test]
fn agent_message_roundtrip_assistant() {
    let msg = llm_message_to_agent(Message::Assistant(faux_assistant_message(vec![faux_text("two")], None)));
    let json = serde_json::to_string(&msg).unwrap();
    let back: AgentMessage = serde_json::from_str(&json).expect(&json);
    assert_eq!(back.role(), "assistant");
}

#[test]
fn session_tree_entry_roundtrip_message() {
    use elph_agent::SessionTreeEntry;
    let entry = SessionTreeEntry::Message {
        id: "abc".to_string(),
        parent_id: None,
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        message: llm_message_to_agent(Message::Assistant(faux_assistant_message(vec![faux_text("two")], None))),
    };
    let json = serde_json::to_string(&entry).unwrap();
    let back: SessionTreeEntry = serde_json::from_str(&json).expect(&json);
    assert_eq!(back.entry_type(), "message");
}
