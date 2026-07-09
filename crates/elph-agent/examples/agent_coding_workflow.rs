//! Coding workflow — read, understand, edit, verify with real API.
//!
//! Uses OpenCode big-pickle to perform a multi-step coding task.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --example agent_coding_workflow
//!
//! # Custom task:
//! cargo run -p elph-agent --example agent_coding_workflow -- --task "Add error handling to src/main.rs"
//! ```

use std::io::{IsTerminal, Write, stderr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use std::time::Duration;

use elph_agent::{Agent, AgentEvent, AgentOptions, LocalExecutionEnv, PartialAgentState, create_coding_tools};
use elph_ai::{Message, StopReason, builtin_models, get_builtin_model};
use indicatif::{ProgressBar, ProgressStyle};

const PROVIDER: &str = "opencode";
const MODEL_ID: &str = "big-pickle";

const DEFAULT_TASK: &str = concat!(
    "Perform a code review of this workspace:\n",
    "1. Read `crates/elph-core/src/lib.rs` to understand the core module\n",
    "2. Run `git status` to check current changes\n",
    "3. Run `cargo check` to verify compilation\n",
    "4. Read `Cargo.toml` to understand dependencies\n",
    "5. Summarize your findings and suggest improvements"
);

struct Args {
    task: String,
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

    println!("╔══════════════════════════════════════════════════╗");
    println!("║     elph-agent Coding Workflow Demo              ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("Provider: OpenCode Zen");
    println!("Model:    {} ({})", model.name, model.id);
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

    // ── Create execution environment ──
    let cwd = std::env::current_dir()?;
    let env = Arc::new(LocalExecutionEnv::new(&cwd));
    let agent_tools = create_coding_tools(env);

    // ── Build agent with coding-focused system prompt ──
    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(
                "You are a senior Rust developer performing a code review.\n\
                 \n\
                 Workflow:\n\
                 1. Read relevant source files\n\
                 2. Run build/check commands\n\
                 3. Analyze the code\n\
                 4. Provide clear, actionable feedback\n\
                 \n\
                 Always explain your reasoning and cite specific line numbers."
                    .into(),
            ),
            model: Some(model),
            tools: Some(agent_tools),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // ── Subscribe to events with detailed logging ──
    let generating = progress_spinner("Streaming from big-pickle...");
    let saw_delta = Arc::new(AtomicBool::new(false));
    let tool_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let tool_log = Arc::new(Mutex::new(Vec::<String>::new()));

    {
        let generating = generating.clone();
        let saw_delta = saw_delta.clone();
        let tool_calls = tool_calls.clone();
        let tool_log = tool_log.clone();
        agent
            .subscribe(Arc::new(move |event, _token| {
                let generating = generating.clone();
                let saw_delta = saw_delta.clone();
                let tool_calls = tool_calls.clone();
                let tool_log = tool_log.clone();
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
                        AgentEvent::ToolExecutionStart { tool_name, args, .. } => {
                            if !saw_delta.load(Ordering::SeqCst) {
                                generating.finish_and_clear();
                            }
                            let count = tool_calls.fetch_add(1, Ordering::SeqCst) + 1;
                            let log_entry = match tool_name.as_str() {
                                "read" => {
                                    let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("?");
                                    format!("[{count}] READ {path}")
                                }
                                "bash" => {
                                    let cmd = args.get("command").and_then(|c| c.as_str()).unwrap_or("?");
                                    format!("[{count}] BASH {cmd}")
                                }
                                "edit" => {
                                    let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("?");
                                    format!("[{count}] EDIT {path}")
                                }
                                "write" => {
                                    let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("?");
                                    format!("[{count}] WRITE {path}")
                                }
                                _ => format!("[{count}] {tool_name}"),
                            };
                            tool_log.lock().push(log_entry.clone());
                            println!();
                            println!("🔧 {log_entry}");
                        }
                        AgentEvent::ToolExecutionEnd {
                            tool_name, is_error, ..
                        } => {
                            let status = if is_error { "❌" } else { "✅" };
                            println!("{status} {tool_name} completed");
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
    }

    // ── Run ──
    println!("═══ Task ═══");
    println!("{}", args.task);
    println!();
    println!("═══ Execution ═══");

    agent.prompt_text(&args.task, None).await?;
    agent.wait_for_idle().await;
    println!();

    // ── Summary ──
    let state = agent.state().await;
    let log = tool_log.lock();

    println!();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║                  Summary                        ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!("Messages:      {}", state.messages.len());
    println!("Tool calls:    {}", tool_calls.load(Ordering::SeqCst));
    println!();

    if !log.is_empty() {
        println!("Tool execution log:");
        for entry in log.iter() {
            println!("  {entry}");
        }
    }

    if let Some(Message::Assistant(assistant)) = state.messages.last().and_then(|m| m.as_llm()) {
        println!();
        print_usage(assistant);
    }

    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut task = DEFAULT_TASK.to_string();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--task" => {
                task = args.next().ok_or_else(|| anyhow::anyhow!("--task requires a value"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help for usage."),
        }
    }

    Ok(Args { task })
}

fn print_help() {
    println!(
        "elph-agent coding workflow demo\n\
         \n\
         Environment:\n\
           OPENCODE_API_KEY   Required API key (https://opencode.ai)\n\
         \n\
         Options:\n\
           --task <text>      Task for the agent (default: code review workflow)\n\
           -h, --help         Show this help\n\
         \n\
         Examples:\n\
           cargo run -p elph-agent --example agent_coding_workflow\n\
           cargo run -p elph-agent --example agent_coding_workflow -- --task \"Fix the bug in main.rs\""
    );
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

fn print_usage(message: &elph_ai::AssistantMessage) {
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
