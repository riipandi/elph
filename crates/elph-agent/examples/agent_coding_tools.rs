//! Live coding tools demo — read, bash, edit, write, grep, find, ls with real API.
//!
//! Uses OpenCode big-pickle to perform actual file operations.
//! The agent reads files, runs commands, edits code, and writes new files.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --example agent_coding_tools
//!
//! # Custom task:
//! cargo run -p elph-agent --example agent_coding_tools -- --task "List all .rs files in src/"
//! ```

use std::io::{IsTerminal, Write, stderr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use elph_agent::{
    Agent, AgentEvent, AgentOptions, LocalExecutionEnv, PartialAgentState, create_all_tools, create_edit_tools,
    create_search_tools,
};
use elph_ai::{Message, StopReason, builtin_models, get_builtin_model};
use indicatif::{ProgressBar, ProgressStyle};

const PROVIDER: &str = "opencode";
const MODEL_ID: &str = "big-pickle";

const DEFAULT_TASK: &str = concat!(
    "Read the file `Cargo.toml` in this workspace, then run `ls -la` to list directory contents. ",
    "After that, create a new file called `TOOLS_DEMO.txt` with the current date and a summary of what you found."
);

struct Args {
    task: String,
    tools: String,
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
    println!("Tools:    {}", args.tools);
    println!();

    let setup = progress_spinner("Resolving auth...");
    let models = builtin_models(None);
    let auth = models.get_auth(&model).await?;
    setup.finish_and_clear();

    if let Some(auth) = &auth {
        println!("Auth:     configured via {}", auth.source.as_deref().unwrap_or("unknown"));
    } else {
        anyhow::bail!("OpenCode Zen is not configured (missing OPENCODE_API_KEY?)");
    }
    println!();

    let models: Arc<elph_ai::Models> = models.into_arc();
    let stream_fn: elph_agent::StreamFn = {
        let models = models.clone();
        Arc::new(move |m, ctx, opts| models.stream_simple(m, ctx, opts))
    };

    // ── Create execution environment ──
    let cwd = std::env::current_dir()?;
    let env = Arc::new(LocalExecutionEnv::new(&cwd));

    // ── Select tools based on --tools flag ──
    let agent_tools = match args.tools.as_str() {
        "read-only" => {
            println!("Using read-only tools: read, grep, find, ls");
            create_search_tools(env.clone())
        }
        "coding" => {
            println!("Using coding tools: read, bash, edit, write");
            create_edit_tools(env.clone())
        }
        "all" => {
            println!("Using all tools: read, bash, edit, write, grep, find, ls");
            create_all_tools(env.clone())
        }
        _ => {
            println!("Using coding tools (default): read, bash, edit, write");
            create_edit_tools(env.clone())
        }
    };

    // ── Build agent ──
    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(
                "You are a coding assistant. Use the available tools to read files, \
                 run commands, edit code, and write files. Always explain what you're doing."
                    .into(),
            ),
            model: Some(model),
            tools: Some(agent_tools),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // ── Subscribe to events ──
    let generating = progress_spinner("Streaming from big-pickle...");
    let saw_delta = Arc::new(AtomicBool::new(false));
    let tool_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    {
        let generating = generating.clone();
        let saw_delta = saw_delta.clone();
        let tool_calls = tool_calls.clone();
        agent
            .subscribe(Arc::new(move |event, _token| {
                let generating = generating.clone();
                let saw_delta = saw_delta.clone();
                let tool_calls = tool_calls.clone();
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
                        AgentEvent::ToolExecutionStart { tool_name, .. } => {
                            if !saw_delta.load(Ordering::SeqCst) {
                                generating.finish_and_clear();
                            }
                            tool_calls.fetch_add(1, Ordering::SeqCst);
                            println!();
                            println!("🔧 Calling tool: {tool_name}");
                        }
                        AgentEvent::ToolExecutionEnd {
                            tool_name, is_error, ..
                        } => {
                            let status = if is_error { "❌" } else { "✅" };
                            println!("{status} Tool finished: {tool_name}");
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
    println!("Task: {}", args.task);
    println!();
    agent.prompt_text(&args.task, None).await?;
    agent.wait_for_idle().await;
    println!();

    // ── Summary ──
    let state = agent.state().await;
    println!("Transcript messages: {}", state.messages.len());
    println!("Tool calls executed: {}", tool_calls.load(Ordering::SeqCst));

    if let Some(Message::Assistant(assistant)) = state.messages.last().and_then(|m| m.as_llm()) {
        print_usage(assistant);
    }

    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut task = DEFAULT_TASK.to_string();
    let mut tools = "coding".to_string();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--task" => {
                task = args.next().ok_or_else(|| anyhow::anyhow!("--task requires a value"))?;
            }
            "--tools" => {
                tools = args.next().ok_or_else(|| anyhow::anyhow!("--tools requires a value"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help for usage."),
        }
    }

    Ok(Args { task, tools })
}

fn print_help() {
    println!(
        "elph-agent coding tools demo\n\
         \n\
         Environment:\n\
           OPENCODE_API_KEY   Required API key (https://opencode.ai)\n\
         \n\
         Options:\n\
           --task <text>      Task for the agent (default: read Cargo.toml + create file)\n\
           --tools <type>     Tool set: read-only | coding | all (default: coding)\n\
           -h, --help         Show this help\n\
         \n\
         Examples:\n\
           cargo run -p elph-agent --example agent_coding_tools\n\
           cargo run -p elph-agent --example agent_coding_tools -- --task \"Read src/lib.rs\"\n\
           cargo run -p elph-agent --example agent_coding_tools -- --tools all"
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
