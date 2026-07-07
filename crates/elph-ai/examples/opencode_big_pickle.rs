//! Stream a chat completion from OpenCode Zen using the `big-pickle` model.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//!
//! # Buffered response (default):
//! cargo run -p elph-ai --example opencode_big_pickle
//!
//! # Stream tokens as they arrive:
//! cargo run -p elph-ai --example opencode_big_pickle -- --stream
//!
//! # Stream and print reasoning tokens to stderr:
//! cargo run -p elph-ai --example opencode_big_pickle -- --stream --show-thinking
//! ```

use std::io::{IsTerminal, Write, stderr};
use std::time::Duration;

use elph_ai::{AssistantContentBlock, AssistantMessageEvent, Message, StopReason, UserContent};
use elph_ai::{Context, builtin_models, get_builtin_model};
use indicatif::{ProgressBar, ProgressStyle};

const PROVIDER: &str = "opencode";
const MODEL_ID: &str = "big-pickle";

struct Args {
    stream: bool,
    show_thinking: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;

    if std::env::var("OPENCODE_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
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
    println!("Mode:     {}", if args.stream { "streaming" } else { "buffered" });
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

    let context = Context {
        system_prompt: Some("You are a concise, helpful assistant.".into()),
        messages: vec![Message::User {
            content: UserContent::Text(
                "In one short paragraph, what makes Rust a good language for systems programming?".into(),
            ),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }],
        tools: None,
    };

    let generating = progress_spinner(if args.stream {
        "Streaming from big-pickle..."
    } else {
        "Waiting for big-pickle..."
    });

    let stream = models.stream(&model, &context, None);
    let mut events = stream.into_stream();

    if args.stream {
        run_streaming(&mut events, &generating, args.show_thinking).await?;
    } else {
        run_buffered(&mut events, &generating).await?;
    }

    Ok(())
}

async fn run_streaming(
    events: &mut elph_ai::EventStreamIterator,
    progress: &ProgressBar,
    show_thinking: bool,
) -> anyhow::Result<()> {
    print!("Assistant: ");
    let _ = stdout().flush();

    let mut started = false;
    let mut printed_text = false;

    while let Some(event) = events.next().await {
        match event {
            AssistantMessageEvent::TextDelta { delta, .. } => {
                if !started {
                    progress.finish_and_clear();
                    started = true;
                }
                print!("{delta}");
                printed_text = true;
                let _ = stdout().flush();
            }
            AssistantMessageEvent::ThinkingDelta { delta, .. } if show_thinking => {
                if !started {
                    progress.finish_and_clear();
                    started = true;
                }
                eprint!("{delta}");
                let _ = stderr().flush();
            }
            AssistantMessageEvent::Done { reason, message } => {
                if !started {
                    progress.finish_and_clear();
                }
                if !printed_text {
                    for block in &message.content {
                        if let AssistantContentBlock::Text(text) = block {
                            print!("{}", text.text);
                        }
                    }
                }
                println!();
                print_usage(&message, reason);
            }
            AssistantMessageEvent::Error { error, .. } => {
                progress.finish_and_clear();
                println!();
                anyhow::bail!(
                    "stream error: {}",
                    error.error_message.unwrap_or_else(|| "unknown".into())
                );
            }
            _ => {}
        }
    }

    if !started {
        progress.finish_and_clear();
    }

    Ok(())
}

async fn run_buffered(events: &mut elph_ai::EventStreamIterator, progress: &ProgressBar) -> anyhow::Result<()> {
    let mut final_message = None;
    let mut stop_reason = StopReason::Stop;

    while let Some(event) = events.next().await {
        match event {
            AssistantMessageEvent::Done { reason, message } => {
                final_message = Some(message);
                stop_reason = reason;
            }
            AssistantMessageEvent::Error { error, .. } => {
                progress.finish_and_clear();
                anyhow::bail!(
                    "stream error: {}",
                    error.error_message.unwrap_or_else(|| "unknown".into())
                );
            }
            _ => {}
        }
    }

    progress.finish_and_clear();

    let message = final_message.ok_or_else(|| anyhow::anyhow!("stream ended without a response"))?;
    print!("Assistant: ");
    for block in &message.content {
        if let AssistantContentBlock::Text(text) = block {
            print!("{}", text.text);
        }
    }
    println!();
    print_usage(&message, stop_reason);

    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut stream = false;
    let mut show_thinking = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--stream" => stream = true,
            "--show-thinking" => show_thinking = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help for usage."),
        }
    }

    Ok(Args { stream, show_thinking })
}

fn print_help() {
    println!(
        "OpenCode Zen big-pickle example\n\
         \n\
         Environment:\n\
           OPENCODE_API_KEY   Required API key (https://opencode.ai)\n\
         \n\
         Options:\n\
           --stream           Print assistant text as tokens arrive\n\
           --show-thinking    With --stream, also print reasoning to stderr\n\
           -h, --help         Show this help\n\
         \n\
         Examples:\n\
           cargo run -p elph-ai --example opencode_big_pickle\n\
           cargo run -p elph-ai --example opencode_big_pickle -- --stream\n\
           cargo run -p elph-ai --example opencode_big_pickle -- --stream --show-thinking"
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

fn stdout() -> impl Write {
    std::io::stdout()
}

fn print_usage(message: &elph_ai::AssistantMessage, reason: StopReason) {
    println!();
    println!("Stop reason: {reason:?}");
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
}
