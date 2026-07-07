use elph_ai::api::github_copilot_headers::{
    build_copilot_dynamic_headers, has_copilot_vision_input, infer_copilot_initiator,
};
use elph_ai::types::{ContentBlock, Message, UserContent};

#[test]
fn infers_initiator_from_first_user_message() {
    let messages = vec![Message::User {
        content: UserContent::Text("hello".to_string()),
        timestamp: 0,
    }];
    assert_eq!(infer_copilot_initiator(&messages), "user");
}

#[test]
fn detects_vision_input_from_image_blocks() {
    let messages = vec![Message::User {
        content: UserContent::Blocks(vec![ContentBlock::Image {
            data: "aGVsbG8=".to_string(),
            mime_type: "image/png".to_string(),
        }]),
        timestamp: 0,
    }];
    assert!(has_copilot_vision_input(&messages));
}

#[test]
fn build_dynamic_headers_includes_session_affinity_when_requested() {
    let messages = vec![Message::User {
        content: UserContent::Text("hi".to_string()),
        timestamp: 0,
    }];
    let headers = build_copilot_dynamic_headers(&messages, false);
    assert_eq!(headers.get("X-Initiator"), Some(&"user".to_string()));
    assert_eq!(headers.get("Openai-Intent"), Some(&"conversation-edits".to_string()));
}
