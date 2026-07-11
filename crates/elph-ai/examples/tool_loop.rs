//! Multi-turn conversation with tool (function) calls.
//!
//! Demonstrates: tool definition, tool-call event handling, tool-result injection,
//! session-based prompt caching, and multi-turn streaming.
//!
//! ```bash
//! cargo run -p elph-ai --example tool_loop
//! ```

use std::io::Write;

use elph_ai::{
    AssistantContentBlock, AssistantMessageEvent, ContentBlock, Context, FauxModelDefinition, FauxResponseStep,
    Message, RegisterFauxProviderOptions, StopReason, Tool, UserContent, create_models, faux_assistant_message,
    faux_provider, faux_text, faux_tool_call,
};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let models = setup_faux_provider();
    let model = models.get_model("demo", "demo-model").expect("demo-model registered");

    // ── Define a tool ──
    let weather_tool = Tool {
        name: "get_weather".into(),
        description: "Get current temperature for a city".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "city": { "type": "string", "description": "City name" }
            },
            "required": ["city"]
        }),
    };

    // ── Context with tools ──
    let mut context = Context {
        system_prompt: Some("You are a helpful weather bot. Use the get_weather tool.".into()),
        messages: vec![Message::User {
            content: UserContent::Text("What's the weather in Jakarta?".into()),
            timestamp: timestamp(),
        }],
        tools: Some(vec![weather_tool]),
    };

    // ── Turn 1: user question → tool call ──
    println!("Turn 1 — expecting a tool call");
    let message = models
        .complete(&model, &context, options_with_session("session-1"))
        .await;

    let mut tool_call_id = String::new();
    for block in &message.content {
        match block {
            AssistantContentBlock::ToolCall(tc) => {
                println!("  Tool call: {} → {}", tc.name, tc.arguments);
                tool_call_id = tc.id.clone();
            }
            AssistantContentBlock::Text(t) => println!("  Text: {}", t.text),
            AssistantContentBlock::Thinking(t) => eprintln!("  Think: {}", t.thinking),
        }
    }
    println!("  Stop: {:?}  Tokens: {}", message.stop_reason, message.usage.total_tokens);
    println!();

    // ── Inject tool result ──
    context.messages.push(Message::ToolResult {
        tool_call_id,
        tool_name: "get_weather".into(),
        content: vec![ContentBlock::Text {
            text: "32°C, high humidity (78%)".into(),
        }],
        details: None,
        added_tool_names: None,
        is_error: false,
        timestamp: timestamp(),
    });

    // ── Turn 2: tool result → final answer (same session for cache demo) ──
    println!("Turn 2 — tool result consumed, expecting summary");
    let message = models
        .complete(&model, &context, options_with_session("session-1"))
        .await;

    for block in &message.content {
        match block {
            AssistantContentBlock::Text(t) => println!("  {}", t.text),
            AssistantContentBlock::Thinking(t) => eprintln!("  Think: {}", t.thinking),
            _ => {}
        }
    }
    println!(
        "  Stop: {:?}  Tokens: {} (cache read: {})",
        message.stop_reason, message.usage.total_tokens, message.usage.cache_read,
    );
    println!();

    // ── Demonstrate streaming with tool events ──
    println!("Turn 3 (streaming) — another question with fresh session");
    context.messages.push(Message::User {
        content: UserContent::Text("What about Tokyo?".into()),
        timestamp: timestamp(),
    });

    let mut events = models
        .stream(&model, &context, options_with_session("session-2"))
        .into_stream();

    while let Some(event) = events.next().await {
        match event {
            AssistantMessageEvent::TextDelta { delta, .. } => {
                print!("{delta}");
                let _ = std::io::stdout().flush();
            }
            AssistantMessageEvent::ToolcallDelta { delta, .. } => {
                // Simulate incremental tool-argument streaming
                print!("[tool args: {delta}]");
                let _ = std::io::stdout().flush();
            }
            AssistantMessageEvent::Done { reason, message } => {
                println!();
                println!("  Stop: {reason:?}  Total tokens: {}", message.usage.total_tokens);
            }
            _ => {}
        }
    }

    Ok(())
}

fn setup_faux_provider() -> elph_ai::MutableModels {
    let mut models = create_models(None);

    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("demo".to_string()),
        models: Some(vec![FauxModelDefinition {
            id: "demo-model".to_string(),
            name: Some("Demo Model".to_string()),
            reasoning: Some(false),
            input: None,
            context_window: None,
            max_tokens: None,
        }]),
        ..Default::default()
    });

    // Queue responses consumed in order by each complete()/stream() call.
    // Turn 1: tool call
    // Turn 2: text summary
    // Turn 3: another tool call + text
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("get_weather", json!({"city": "Jakarta"}), None)],
            Some(StopReason::ToolUse),
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Jakarta is 32°C with 78% humidity — hot and humid.")],
            Some(StopReason::Stop),
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![
                faux_tool_call("get_weather", json!({"city": "Tokyo"}), None),
                faux_text("Tokyo is 22°C with light rain."),
            ],
            Some(StopReason::Stop),
        )),
    ]);

    models.set_provider(faux.provider);
    models
}

fn options_with_session(session_id: &str) -> Option<elph_ai::StreamOptions> {
    Some(elph_ai::StreamOptions {
        session_id: Some(session_id.to_string()),
        headers: Some(std::collections::HashMap::new()),
        ..Default::default()
    })
}

fn timestamp() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
