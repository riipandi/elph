//! Cross-provider handoff live test for elph-ai.
//! Run with: `cargo test -p elph-ai --test cross_provider_handoff_live -- --ignored`

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use elph_ai::types::{
    AssistantContentBlock, Context, Message, SimpleStreamOptions, StopReason, ThinkingLevel, Tool, UserContent,
};
use elph_ai::{builtin_models, get_builtin_model};
use serde_json::json;

struct ProviderPair {
    provider: &'static str,
    model: &'static str,
    label: &'static str,
    env: &'static str,
    reasoning: bool,
}

const PAIRS: &[ProviderPair] = &[
    ProviderPair {
        provider: "anthropic",
        model: "claude-sonnet-4-5",
        label: "anthropic-claude-sonnet-4-5",
        env: "ANTHROPIC_API_KEY",
        reasoning: true,
    },
    ProviderPair {
        provider: "google",
        model: "gemini-2.5-flash",
        label: "google-gemini-2.5-flash",
        env: "GEMINI_API_KEY",
        reasoning: true,
    },
    ProviderPair {
        provider: "openai",
        model: "gpt-5-mini",
        label: "openai-responses-gpt-5-mini",
        env: "OPENAI_API_KEY",
        reasoning: false,
    },
    ProviderPair {
        provider: "mistral",
        model: "devstral-medium-latest",
        label: "mistral-devstral-medium",
        env: "MISTRAL_API_KEY",
        reasoning: false,
    },
];

fn has_env(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty())
}

fn available_pairs() -> Vec<&'static ProviderPair> {
    PAIRS.iter().filter(|p| has_env(p.env)).collect()
}

fn test_tool() -> Tool {
    Tool {
        name: "double_number".to_string(),
        description: "Doubles a number and returns the result".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "value": { "type": "number", "description": "A number to double" }
            },
            "required": ["value"]
        }),
    }
}

async fn generate_fixture(pair: &ProviderPair) -> Option<Vec<Message>> {
    let model = get_builtin_model(pair.provider, pair.model)?;
    let models = builtin_models(None);
    let tool = test_tool();
    let user = Message::User {
        content: UserContent::Text("Please double the number 21 using the double_number tool.".to_string()),
        timestamp: 0,
    };
    let context = Context {
        system_prompt: Some("You are a helpful assistant. Use the provided tool to complete the task.".to_string()),
        messages: vec![user.clone()],
        tools: Some(vec![tool.clone()]),
    };
    let options = pair.reasoning.then_some(SimpleStreamOptions {
        base: Default::default(),
        reasoning: Some(ThinkingLevel::High),
        thinking_budgets: None,
    });

    let assistant = models.complete_simple(&model, &context, options.clone()).await;
    if assistant.stop_reason == StopReason::Error {
        eprintln!("[{}] initial request failed: {:?}", pair.label, assistant.error_message);
        return None;
    }
    let assistant_message = Message::Assistant(assistant.clone());

    let tool_call = assistant.content.iter().find_map(|block| match block {
        AssistantContentBlock::ToolCall(tc) => Some(tc.clone()),
        _ => None,
    });
    let Some(tool_call) = tool_call else {
        return Some(vec![user, assistant_message]);
    };

    let tool_result = Message::ToolResult {
        tool_call_id: tool_call.id.clone(),
        tool_name: tool_call.name.clone(),
        content: vec![elph_ai::types::ContentBlock::Text { text: "42".to_string() }],
        details: None,
        is_error: false,
        timestamp: 1,
    };
    let final_context = Context {
        system_prompt: Some("You are a helpful assistant.".to_string()),
        messages: vec![user.clone(), assistant_message.clone(), tool_result.clone()],
        tools: Some(vec![tool]),
    };
    let final_response = models.complete_simple(&model, &final_context, options).await;
    if final_response.stop_reason == StopReason::Error {
        eprintln!(
            "[{}] final request failed: {:?}",
            pair.label, final_response.error_message
        );
        return None;
    }

    Some(vec![
        user,
        assistant_message,
        tool_result,
        Message::Assistant(final_response),
    ])
}

fn fixtures() -> Arc<HashMap<String, Vec<Message>>> {
    static FIXTURES: OnceLock<Arc<HashMap<String, Vec<Message>>>> = OnceLock::new();
    FIXTURES
        .get_or_init(|| {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            rt.block_on(async {
                let mut map = HashMap::new();
                for pair in available_pairs() {
                    if let Some(messages) = generate_fixture(pair).await
                        && messages.len() >= 4
                    {
                        map.insert(pair.label.to_string(), messages);
                    }
                }
                Arc::new(map)
            })
        })
        .clone()
}

#[test]
#[ignore = "requires API keys for at least two providers"]
fn cross_provider_handoffs_succeed_for_each_target() {
    let available = available_pairs();
    if available.len() < 2 {
        eprintln!("Skipping handoff test: need at least 2 providers with API keys");
        return;
    }

    let contexts = fixtures();
    if contexts.len() < 2 {
        panic!("expected at least 2 generated fixtures, got {}", contexts.len());
    }

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        let models = builtin_models(None);
        let mut failures = Vec::new();

        for target in &available {
            let Some(model) = get_builtin_model(target.provider, target.model) else {
                continue;
            };
            let mut other_messages = Vec::new();
            for (label, messages) in contexts.iter() {
                if label == target.label {
                    continue;
                }
                other_messages.extend(messages.clone());
            }
            if other_messages.is_empty() {
                continue;
            }
            other_messages.push(Message::User {
                content: UserContent::Text(
                    "Great, thanks for all that help! Now just say 'Hello, handoff successful!' to confirm you received everything.".to_string(),
                ),
                timestamp: chrono::Utc::now().timestamp_millis(),
            });

            let context = Context {
                system_prompt: Some("You are a helpful assistant.".to_string()),
                messages: other_messages,
                tools: Some(vec![test_tool()]),
            };
            let options = target.reasoning.then_some(SimpleStreamOptions {
                base: Default::default(),
                reasoning: Some(ThinkingLevel::High),
                thinking_budgets: None,
            });

            let response = models.complete_simple(&model, &context, options).await;
            if response.stop_reason == StopReason::Error {
                failures.push(format!(
                    "{}: {}",
                    target.label,
                    response.error_message.unwrap_or_else(|| "unknown error".to_string())
                ));
            }
        }

        if !failures.is_empty() {
            panic!("cross-provider handoff failures:\n{}", failures.join("\n"));
        }
    });
}
