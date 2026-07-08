//! Multi-turn conversation — maintain context across multiple prompts.
//!
//! Uses OpenCode big-pickle to show how the agent remembers previous turns.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --example agent_multi_turn
//! ```

use std::io::{IsTerminal, Write, stderr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use elph_agent::{Agent, AgentEvent, AgentOptions, PartialAgentState};
use elph_ai::{AssistantContentBlock, Message, builtin_models, get_builtin_model};
use indicatif::{ProgressBar, ProgressStyle};

const PROVIDER: &str = "opencode";
const MODEL_ID: &str = "big-pickle";

/// Multi-turn conversation: agent remembers context across prompts.
const TURNS: &[&str] = &[
    "My name is Alice. Remember this.",
    "What is my name?",
    "Now write a short Rust function that calculates factorial.",
    "Can you explain how the recursion works in that function?",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    check_api_key()?;

    let model = get_builtin_model(PROVIDER, MODEL_ID)
        .ok_or_else(|| anyhow::anyhow!("model not found: {PROVIDER}/{MODEL_ID}"))?;

    println!("Provider: OpenCode Zen");
    println!("Model:    {} ({})", model.name, model.id);
    println!("Turns:    {}", TURNS.len());
    println!();

    let setup = progress_spinner("Resolving auth...");
    let models = builtin_models(None);
    let auth = models.get_auth(&model).await?;
    setup.finish_and_clear();

    if auth.is_none() {
        anyhow::bail!("OpenCode Zen is not configured (missing OPENCODE_API_KEY?)");
    }
    println!("Auth:     configured");
    println!();

    let models: Arc<elph_ai::Models> = models.into_arc();
    let stream_fn: elph_agent::StreamFn = {
        let models = models.clone();
        Arc::new(move |m, ctx, opts| models.stream_simple(m, ctx, opts))
    };

    // ── Build agent ──
    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("You are a helpful assistant. Remember what the user tells you across turns.".into()),
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // ── Subscribe to events ──
    let saw_delta = Arc::new(AtomicBool::new(false));
    {
        let saw_delta = saw_delta.clone();
        agent
            .subscribe(Arc::new(move |event, _token| {
                let saw_delta = saw_delta.clone();
                Box::pin(async move {
                    match event {
                        AgentEvent::MessageUpdate {
                            assistant_message_event,
                            ..
                        } => {
                            if let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = &*assistant_message_event {
                                if !saw_delta.swap(true, Ordering::SeqCst) {
                                    // First delta — clear any spinner
                                }
                                print!("{delta}");
                                let _ = std::io::stdout().flush();
                            }
                        }
                        AgentEvent::TurnStart => {
                            saw_delta.store(false, Ordering::SeqCst);
                        }
                        _ => {}
                    }
                })
            }))
            .await;
    }

    // ── Run multi-turn conversation ──
    for (i, prompt) in TURNS.iter().enumerate() {
        println!("═══ Turn {} ═══", i + 1);
        println!("User: {prompt}");
        print!("Assistant: ");
        let _ = std::io::stdout().flush();

        agent.prompt_text(*prompt, None).await?;
        agent.wait_for_idle().await;
        println!();
        println!();
    }

    // ── Show full transcript ──
    let state = agent.state().await;
    println!("═══ Transcript ═══");
    println!("Total messages: {}", state.messages.len());
    for msg in &state.messages {
        match msg {
            elph_agent::AgentMessage::Llm(m) => match m.as_ref() {
                Message::User { content, .. } => {
                    let text = match content {
                        elph_ai::UserContent::Text(t) => t.as_str(),
                        elph_ai::UserContent::Blocks(blocks) => {
                            // Extract first text block
                            blocks
                                .iter()
                                .find_map(|b| match b {
                                    elph_ai::ContentBlock::Text { text } => Some(text.as_str()),
                                    _ => None,
                                })
                                .unwrap_or("[complex content]")
                        }
                    };
                    println!("  User: {text}");
                }
                Message::Assistant(a) => {
                    for block in &a.content {
                        if let AssistantContentBlock::Text(t) = block {
                            println!("  Assistant: {}", t.text.chars().take(100).collect::<String>());
                        }
                    }
                }
                Message::ToolResult { tool_name, .. } => {
                    println!("  ToolResult: {tool_name}");
                }
            },
            elph_agent::AgentMessage::Custom(c) => {
                println!("  Custom: {:?}", c.role());
            }
        }
    }

    println!();
    print_usage(&state);

    Ok(())
}

fn check_api_key() -> anyhow::Result<()> {
    if std::env::var("OPENCODE_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .is_none()
    {
        anyhow::bail!(
            "Set OPENCODE_API_KEY to your OpenCode Zen API key.\n\
             Get one at https://opencode.ai"
        );
    }
    Ok(())
}

fn progress_spinner(message: &str) -> ProgressBar {
    if !stderr().is_terminal() || std::env::var("NO_COLOR").as_deref() == Ok("true") {
        return ProgressBar::hidden();
    }
    let bar = ProgressBar::new_spinner();
    bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg:.cyan}")
            .expect("valid template"),
    );
    bar.set_message(message.to_string());
    bar.enable_steady_tick(Duration::from_millis(80));
    bar
}

fn print_usage(state: &elph_agent::AgentState) {
    // Count messages by role
    let user_count = state.messages.iter().filter(|m| m.role() == "user").count();
    let assistant_count = state.messages.iter().filter(|m| m.role() == "assistant").count();
    println!("User messages: {user_count}");
    println!("Assistant messages: {assistant_count}");
}
