//! Prompt OpenCode Zen `big-pickle` through `elph-agent::Agent`.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --example opencode_big_pickle_agent
//!
//! # Custom prompt:
//! cargo run -p elph-agent --example opencode_big_pickle_agent -- --prompt "Explain async in Rust briefly."
//! ```

use std::io::{IsTerminal, Write, stderr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use elph_agent::{Agent, AgentEvent, AgentOptions, PartialAgentState};
use elph_ai::{AssistantContentBlock, Message, StopReason, builtin_models, get_builtin_model};
use indicatif::{ProgressBar, ProgressStyle};

const PROVIDER: &str = "opencode";
const MODEL_ID: &str = "big-pickle";

const DEFAULT_PROMPT: &str = "In one short paragraph, what makes Rust a good language for systems programming?";

struct Args {
    prompt: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;

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

    let model = get_builtin_model(PROVIDER, MODEL_ID)
        .ok_or_else(|| anyhow::anyhow!("model not found: {PROVIDER}/{MODEL_ID}"))?;

    println!("Provider: OpenCode Zen");
    println!("Model:    {} ({})", model.name, model.id);
    println!("API:      {}", model.api);
    println!();

    let setup = progress_spinner("Resolving auth...");
    let models = builtin_models(None);
    let auth = models.get_auth(&model).await?;
    setup.finish_and_clear();

    if let Some(auth) = &auth {
        println!(
            "Auth:     configured via {}",
            auth.source.as_deref().unwrap_or("unknown")
        );
    } else {
        anyhow::bail!("OpenCode Zen is not configured (missing OPENCODE_API_KEY?)");
    }
    println!();

    let models: Arc<elph_ai::Models> = models.into_arc();
    let stream_fn: elph_agent::StreamFn = {
        let models = models.clone();
        Arc::new(move |m, ctx, opts| models.stream_simple(m, ctx, opts))
    };

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("You are a concise, helpful assistant.".into()),
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    let generating = progress_spinner("Streaming from big-pickle via elph-agent...");
    let saw_delta = Arc::new(AtomicBool::new(false));

    agent
        .subscribe(Arc::new(move |event, _token| {
            let generating = generating.clone();
            let saw_delta = saw_delta.clone();
            Box::pin(async move {
                match event {
                    AgentEvent::MessageUpdate {
                        assistant_message_event,
                        ..
                    } => {
                        if let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = &*assistant_message_event {
                            if !saw_delta.swap(true, Ordering::SeqCst) {
                                generating.finish_and_clear();
                            }
                            print!("{delta}");
                            let _ = std::io::stdout().flush();
                        }
                    }
                    AgentEvent::AgentEnd { .. } if !saw_delta.load(Ordering::SeqCst) => {
                        generating.finish_and_clear();
                    }
                    AgentEvent::AgentEnd { .. } => {}
                    _ => {}
                }
            })
        }))
        .await;

    print!("Assistant: ");
    let _ = std::io::stdout().flush();

    agent.prompt_text(&args.prompt, None).await?;
    agent.wait_for_idle().await;
    println!();

    let state = agent.state().await;
    println!("Transcript messages: {}", state.messages.len());

    if let Some(Message::Assistant(assistant)) = state.messages.last().and_then(|m| m.as_llm()) {
        let printed_text = assistant
            .content
            .iter()
            .any(|block| matches!(block, AssistantContentBlock::Text(_)));
        if !printed_text {
            for block in &assistant.content {
                if let AssistantContentBlock::Text(text) = block {
                    println!("{}", text.text);
                }
            }
        }
        print_usage(assistant);
    }

    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut prompt = DEFAULT_PROMPT.to_string();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--prompt" => {
                prompt = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--prompt requires a value"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help for usage."),
        }
    }

    Ok(Args { prompt })
}

fn print_help() {
    println!(
        "elph-agent + OpenCode Zen big-pickle\n\
         \n\
         Environment:\n\
           OPENCODE_API_KEY   Required API key (https://opencode.ai)\n\
         \n\
         Options:\n\
           --prompt <text>    User message (default: built-in systems-programming question)\n\
           -h, --help         Show this help\n\
         \n\
         Examples:\n\
           cargo run -p elph-agent --example opencode_big_pickle_agent\n\
           cargo run -p elph-agent --example opencode_big_pickle_agent -- --prompt \"Hello!\""
    );
}

fn progress_spinner(message: &str) -> ProgressBar {
    if !progress_enabled() {
        eprintln!("{message}");
        return ProgressBar::hidden();
    }

    let bar = ProgressBar::new_spinner();
    bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg:.cyan}")
            .expect("valid spinner template"),
    );
    bar.set_message(message.to_string());
    bar.enable_steady_tick(Duration::from_millis(80));
    bar
}

fn progress_enabled() -> bool {
    if std::env::var("NO_COLOR").as_deref() == Ok("true") {
        return false;
    }
    stderr().is_terminal()
}

fn print_usage(message: &elph_ai::AssistantMessage) {
    println!();
    println!("Stop reason: {:?}", message.stop_reason);
    println!(
        "Tokens: {} in / {} out (total {})",
        message.usage.input, message.usage.output, message.usage.total_tokens
    );
    if let Some(reasoning) = message.usage.reasoning {
        println!("Reasoning tokens: {reasoning}");
    }
    if message.usage.cost.total > 0.0 {
        println!("Cost: ${:.6}", message.usage.cost.total);
    }
    if let Some(error) = &message.error_message {
        println!("Error: {error}");
        if message.stop_reason == StopReason::Error {
            std::process::exit(1);
        }
    }
}
