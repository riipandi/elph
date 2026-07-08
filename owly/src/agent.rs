//! Agent integration using elph-agent and elph-ai.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/agent/index.ts`. Original MIT License, Copyright (c) 2026 LangChain.
//!
//! This module uses the Elph agent runtime instead of LangChain/LangGraph.
//! The core agent loop and tool execution are delegated to `elph-agent`,
//! while LLM provider integration uses `elph-ai`.

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use elph_agent::{
    Agent, AgentEvent, AgentOptions, LocalExecutionEnv, PartialAgentState, create_all_tools, create_read_only_tools,
};
use elph_ai::{AssistantMessageEvent, builtin_models, get_builtin_model};

use crate::ask_user::{create_ask_confirm_tool, create_ask_select_tool, create_ask_text_tool};
use crate::cli::print_tool_call;
use crate::config::Config;
use crate::constants::provider_config;
use crate::metadata::UpdateMetadata;
use crate::prompts::{create_chat_prompt, create_init_prompt, create_interactive_system_prompt, create_update_prompt};

/// Create a progress spinner
fn progress_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/** Options for running the agent */
pub struct RunAgentOptions<'a> {
    pub command: &'a str,
    pub system_prompt: &'a str,
    pub user_prompt: &'a str,
    pub config: &'a Config,
    pub cwd: &'a Path,
    pub print_mode: bool,
    pub stream: bool,
    pub verbose: bool,
}

/** Resolve model and auth, returning (model, models_arc, stream_fn) */
async fn resolve_model_and_auth(
    config: &Config,
) -> Result<(elph_ai::Model, Arc<elph_ai::Models>, elph_agent::StreamFn)> {
    let model = get_builtin_model(&config.provider, &config.model_id)
        .or_else(|| {
            let parts: Vec<&str> = config.model_id.splitn(2, '/').collect();
            if parts.len() == 2 {
                get_builtin_model(parts[0], parts[1])
            } else {
                None
            }
        })
        .or_else(|| get_builtin_model(&config.provider, &config.model_id))
        .context(format!(
            "Model not found: {}/{}. Use provider/model format (e.g., opencode/big-pickle)",
            config.provider, config.model_id
        ))?;

    let setup = progress_spinner("Resolving auth...");
    let models = builtin_models(None);
    let auth = models.get_auth(&model).await?;
    setup.finish_and_clear();

    if auth.is_none() {
        let provider_cfg =
            provider_config(&config.provider).context(format!("Unknown provider: {}", config.provider))?;
        anyhow::bail!(
            "No API key configured for {}. Set {} environment variable.",
            provider_cfg.label,
            provider_cfg.api_key_env_key
        );
    }

    let models: Arc<elph_ai::Models> = models.into_arc();
    let stream_fn: elph_agent::StreamFn = {
        let models = models.clone();
        Arc::new(move |m, ctx, opts| models.stream_simple(m, ctx, opts))
    };

    Ok((model, models, stream_fn))
}

/// Create an event subscriber closure for streaming display
fn create_event_subscriber(
    stream: bool,
    verbose: bool,
    generating: ProgressBar,
    saw_any_delta: Arc<AtomicBool>,
) -> elph_agent::AgentListener {
    let verbose_clone = verbose;
    let stream_clone = stream;
    Arc::new(move |event, _token| {
        let generating = generating.clone();
        let saw_any_delta = saw_any_delta.clone();
        let verbose = verbose_clone;
        let stream = stream_clone;
        Box::pin(async move {
            match event {
                AgentEvent::MessageUpdate {
                    assistant_message_event,
                    ..
                } => match &*assistant_message_event {
                    AssistantMessageEvent::TextDelta { delta, .. } => {
                        if !saw_any_delta.swap(true, Ordering::SeqCst) {
                            generating.finish_and_clear();
                        }
                        if stream || verbose {
                            print!("{delta}");
                            let _ = std::io::stdout().flush();
                        }
                    }
                    AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                        if !saw_any_delta.swap(true, Ordering::SeqCst) {
                            generating.finish_and_clear();
                        }
                        if verbose {
                            eprint!("\x1b[2m{delta}\x1b[0m");
                            let _ = std::io::stderr().flush();
                        }
                    }
                    _ => {}
                },
                AgentEvent::ToolExecutionStart { tool_name, .. } => {
                    if !saw_any_delta.load(Ordering::SeqCst) {
                        generating.finish_and_clear();
                    }
                    print_tool_call(&tool_name, verbose);
                }
                AgentEvent::ToolExecutionEnd {
                    tool_name, is_error, ..
                } => {
                    if verbose {
                        let icon = if is_error {
                            "\x1b[31m✗\x1b[0m"
                        } else {
                            "\x1b[32m✓\x1b[0m"
                        };
                        eprintln!("  {icon} {tool_name}");
                    }
                }
                AgentEvent::AgentEnd { .. } if !saw_any_delta.load(Ordering::SeqCst) => {
                    generating.finish_and_clear();
                }
                _ => {}
            }
        })
    })
}

/** Run the agent with the given command */
pub async fn run_agent(opts: RunAgentOptions<'_>) -> Result<String> {
    let RunAgentOptions {
        command,
        system_prompt,
        user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
    } = opts;
    let _ = command;
    let start_time = Instant::now();

    let (model, _models_arc, stream_fn) = resolve_model_and_auth(config).await?;

    // Create execution environment with the working directory
    let env = Arc::new(LocalExecutionEnv::new(cwd));
    // Create tools based on the command type
    // For init/update: use all tools (read, bash, edit, write, grep, find, ls)
    // For chat: use read-only tools (read, grep, find, ls)
    let (mut agent_tools, base_tool_str) = if command == "chat" {
        (
            create_read_only_tools(env.clone()),
            "read, grep, find, ls (read-only mode)",
        )
    } else {
        (create_all_tools(env.clone()), "read, bash, edit, write, grep, find, ls")
    };

    // Add ask_user tools for chat
    if command == "chat" {
        agent_tools.push(create_ask_text_tool());
        agent_tools.push(create_ask_select_tool());
        agent_tools.push(create_ask_confirm_tool());
    }

    let tool_names_str = if command == "chat" {
        format!("{base_tool_str}, ask_text, ask_select, ask_confirm")
    } else {
        base_tool_str.to_string()
    };
    // Add ask_user tools for chat
    if command == "chat" {
        agent_tools.push(create_ask_text_tool());
        agent_tools.push(create_ask_select_tool());
        agent_tools.push(create_ask_confirm_tool());
    }

    let tool_names_str = if command == "chat" {
        format!("{tool_names_str}, ask_text, ask_select, ask_confirm")
    } else {
        tool_names_str.to_string()
    };
    let full_system_prompt = format!("{system_prompt}\n\nAvailable tools for this session: {tool_names_str}");

    if verbose {
        let tool_names: Vec<&str> = agent_tools.iter().map(|t| t.name()).collect();
        eprintln!("Tools: {}", tool_names.join(", "));
    }

    // Create the agent with tools
    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(full_system_prompt),
            model: Some(model),
            tools: Some(agent_tools),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // Subscribe to events for streaming display
    let generating = progress_spinner("Thinking...");
    let saw_any_delta = Arc::new(AtomicBool::new(false));

    agent
        .subscribe(create_event_subscriber(
            stream,
            verbose,
            generating.clone(),
            saw_any_delta.clone(),
        ))
        .await;

    // Send the user prompt
    agent.prompt_text(user_prompt, None).await?;

    // Wait for completion
    agent.wait_for_idle().await;

    let elapsed = start_time.elapsed();

    // Get the final state
    let state = agent.state().await;

    // Print final message only when not streaming (avoids duplication)
    if print_mode && !stream {
        if let Some(elph_ai::Message::Assistant(assistant)) = state.messages.last().and_then(|m| m.as_llm()) {
            if !verbose {
                for block in &assistant.content {
                    if let elph_ai::AssistantContentBlock::Text(t) = block {
                        print!("{}", t.text);
                        let _ = std::io::stdout().flush();
                    }
                }
                println!();
            }
            Ok(String::new())
        } else {
            Ok(String::new())
        }
    } else {
        Ok(format!("\x1b[90mCompleted in {:.1}s\x1b[0m", elapsed.as_secs_f64()))
    }
}

/// Run an interactive multi-turn chat session.
/// Uses the same agent across turns so conversation history persists.
pub async fn run_interactive(config: &Config, cwd: &Path, stream: bool, verbose: bool) -> Result<()> {
    let (model, _models_arc, stream_fn) = resolve_model_and_auth(config).await?;

    // Open checkpoint store
    let _checkpoint = crate::checkpoint::SqliteSaver::default().await?;
    // Create execution environment
    let env = Arc::new(LocalExecutionEnv::new(cwd));

    // Create tools: read-only + ask_user tools
    let mut agent_tools = create_read_only_tools(env);
    agent_tools.push(create_ask_text_tool());
    agent_tools.push(create_ask_select_tool());
    agent_tools.push(create_ask_confirm_tool());
    let tool_names_str = "read, grep, find, ls, ask_text, ask_select, ask_confirm";

    let system_prompt = create_interactive_system_prompt();
    let full_system_prompt = format!("{system_prompt}\n\nAvailable tools for this session: {tool_names_str}");

    if verbose {
        let tool_names: Vec<&str> = agent_tools.iter().map(|t| t.name()).collect();
        eprintln!("Tools: {}", tool_names.join(", "));
    }

    // Create the agent once — reuse across turns
    let agent = Arc::new(Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(full_system_prompt),
            model: Some(model),
            tools: Some(agent_tools),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    }));

    // Subscribe to events
    let generating = progress_spinner("Thinking...");
    let saw_any_delta = Arc::new(AtomicBool::new(false));

    agent
        .subscribe(create_event_subscriber(
            stream,
            verbose,
            generating.clone(),
            saw_any_delta.clone(),
        ))
        .await;

    // Print welcome banner
    println!();
    println!("\x1b[36;1m>_ Owly interactive\x1b[0m");
    println!("provider: \x1b[32m{}\x1b[0m", config.provider);
    println!("model: \x1b[32m{}\x1b[0m", config.model_id);
    println!("Type \x1b[33m/exit\x1b[0m or \x1b[33mCtrl+C\x1b[0m to quit.");
    println!();

    // Interactive loop: stdin -> agent -> wait -> loop
    use std::io::BufRead;
    let stdin = std::io::stdin();

    loop {
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            break Ok(());
        }
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        // Check for exit commands
        if trimmed.eq_ignore_ascii_case("/exit")
            || trimmed.eq_ignore_ascii_case("/quit")
            || trimmed.eq_ignore_ascii_case("exit")
            || trimmed.eq_ignore_ascii_case("quit")
        {
            println!("\x1b[2mGoodbye!\x1b[0m");
            break Ok(());
        }

        // Persist user message to checkpoint (future: SqliteSaver::put)
        // Reset delta tracker for this turn
        saw_any_delta.store(false, Ordering::SeqCst);

        // Send message to agent (agent is idle between turns)
        agent.prompt_text(&trimmed, None).await?;
        agent.wait_for_idle().await;

        // Persist assistant response (future: SqliteSaver::put)

        // Print a newline between turns
        println!();
    }
}

/// Prepare the init command
pub fn prepare_init_command(_cwd: &Path, user_message: Option<&str>, _model: &str) -> (String, String) {
    let system_prompt = create_system_prompt_for_init();
    let user_prompt = create_init_prompt("", user_message);

    (system_prompt, user_prompt)
}

/// Prepare the update command
pub fn prepare_update_command(
    cwd: &Path,
    user_message: Option<&str>,
    _model: &str,
    last_update: Option<&UpdateMetadata>,
) -> (String, String) {
    let system_prompt = create_system_prompt_for_update();
    let git_summary = crate::docs::get_git_summary(cwd);
    let user_prompt = create_update_prompt(last_update, &git_summary, user_message);

    (system_prompt, user_prompt)
}

/// Prepare the chat command
pub fn prepare_chat_command(message: &str) -> (String, String) {
    let system_prompt = create_system_prompt_for_chat();
    let user_prompt = create_chat_prompt(message);

    (system_prompt, user_prompt)
}

fn create_system_prompt_for_init() -> String {
    let base = crate::prompts::create_system_prompt();
    format!(
        "{base}\n\n- This is an initial documentation run.\n- Assume {OWLY_DIR}/ does not yet contain useful documentation.\n- Build the documentation structure from scratch.\n- First build a repository inventory: existing docs, graph/app entrypoints, package/config files, major domain folders, tests/evals, data/schema files, skill/playbook files, and operational scripts.\n- Use git evidence during init to understand how important files and workflows came to be.\n- Create {OWLY_DIR}/quickstart.md first, then the linked section pages.\n- Use at most 8 documentation pages on the initial run unless the repository is clearly tiny.\n- Do not try to document every source file. Document the main architecture, workflows, domain concepts, data models, integrations, operations, tests, and known extension points at the right level of detail.\n- The CLI will record successful run metadata after you finish.",
        OWLY_DIR = crate::constants::OWLY_DIR
    )
}

fn create_system_prompt_for_update() -> String {
    let base = crate::prompts::create_system_prompt();
    format!(
        "{base}\n\n- This is a maintenance update run.\n- Inspect the existing {OWLY_DIR}/ documentation before editing.\n- Always use git-oriented repository evidence to understand recent changes.\n- Before editing, build a docs impact plan from the changed source files.\n- Update runs must be surgical. Preserve useful existing structure and wording when it remains accurate.\n- Only edit pages whose current content is inaccurate, incomplete, or misleading because of the recent changes.\n- Keep each concept in one canonical page.\n- Do not make formatting-only edits.\n- Use a soft diff budget: if fewer than about 5 source files changed, update at most 1-2 wiki pages.\n- Updates may be a no-op. If there are no relevant changes, do not edit files.\n- The CLI will record successful run metadata after you finish.",
        OWLY_DIR = crate::constants::OWLY_DIR
    )
}

fn create_system_prompt_for_chat() -> String {
    let base = crate::prompts::create_system_prompt();
    format!(
        "{base}\n\n- This is an interactive chat turn.\n- Answer the user's message directly.\n- Do not create or update Owly documentation unless the user explicitly asks you to modify documentation.\n- If the user asks to initialize or update the wiki, explain that they can run owly --init or owly --update."
    )
}
