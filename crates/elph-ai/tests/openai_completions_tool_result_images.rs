use elph_ai::api::openai_compat::get_compat;
use elph_ai::api::openai_completions::convert_messages;
use elph_ai::get_builtin_model;
use elph_ai::types::{ContentBlock, Context, Message, UserContent};
use serde_json::json;

#[test]
fn groups_tool_result_images_into_follow_up_user_message() {
    let model = get_builtin_model("openai", "gpt-4o").expect("model exists");
    let compat = get_compat(&model);
    let context = Context {
        system_prompt: None,
        messages: vec![
            Message::ToolResult {
                tool_call_id: "call_1".to_string(),
                tool_name: "screenshot".to_string(),
                content: vec![ContentBlock::Image {
                    data: "aGVsbG8=".to_string(),
                    mime_type: "image/png".to_string(),
                }],
                details: None,
                is_error: false,
                timestamp: 0,
            },
            Message::User {
                content: UserContent::Text("next".to_string()),
                timestamp: 1,
            },
        ],
        tools: None,
    };

    let messages = convert_messages(&model, &context, &compat);
    let tool_msg = messages
        .iter()
        .find(|m| m.get("role") == Some(&json!("tool")))
        .expect("tool result");
    assert_eq!(
        tool_msg.get("content").and_then(|v| v.as_str()),
        Some("(see attached image)")
    );

    let image_user = messages
        .iter()
        .find(|m| {
            m.get("role") == Some(&json!("user"))
                && m.pointer("/content/0/text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains("Attached image"))
                    .unwrap_or(false)
        })
        .expect("image follow-up user message");
    assert_eq!(
        image_user.pointer("/content/1/type").and_then(|v| v.as_str()),
        Some("image_url")
    );
}
