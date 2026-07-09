//! Agent integration using elph-agent and elph-ai.

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, mpsc};

use elph_agent::{
    Agent, AgentEvent, AgentOptions, AgentToolResult, LocalExecutionEnv, PartialAgentState, ToolResultContent,
    create_all_tools, create_read_only_tools,
};
use elph_ai::{AssistantMessageEvent, builtin_models, get_builtin_model};

use crate::ask_user::{create_ask_confirm_tool, create_ask_select_tool, create_ask_text_tool};
use crate::cli::print_tool_call;
use crate::config::Config;
use crate::constants::provider_config;
use crate::docs::{self, DocumentationSnapshot};
use crate::env;
use crate::metadata::UpdateMetadata;
use crate::prompts::{create_chat_prompt, create_init_prompt, create_update_prompt};
use crate::session::{SessionStore, TurnWriteContext, is_ask_tool};
use crate::ui_events::AgentUiEvent;

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

/// Result of a single agent invocation.
#[derive(Debug)]
pub struct RunAgentResult {
    pub completion_message: String,
    pub docs_changed: bool,
    pub skipped: bool,
}

/// Options for running the agent
pub struct RunAgentOptions<'a> {
    pub command: &'a str,
    pub system_prompt: &'a str,
    pub user_prompt: &'a str,
    pub config: &'a Config,
    pub cwd: &'a Path,
    pub print_mode: bool,
    pub stream: bool,
    pub verbose: bool,
    pub session: Option<&'a mut SessionStore>,
    pub is_followup: bool,
    pub docs_snapshot_before: Option<DocumentationSnapshot>,
    /// Suppress spinners and direct stdout/stderr writes (interactive TUI mode).
    pub quiet: bool,
    /// Optional live event sink for the interactive TUI transcript.
    pub ui_events: Option<mpsc::UnboundedSender<AgentUiEvent>>,
}

fn emit_ui(ui: &Option<mpsc::UnboundedSender<AgentUiEvent>>, event: AgentUiEvent) {
    if let Some(tx) = ui {
        let _ = tx.send(event);
    }
}

fn emit_checkpoint_warning(
    ui_events: &Option<mpsc::UnboundedSender<AgentUiEvent>>,
    quiet: bool,
    message: impl Into<String>,
) {
    let message = message.into();
    emit_ui(ui_events, AgentUiEvent::Status(message.clone()));
    if ui_events.is_none() && !quiet {
        eprintln!("{message}");
    }
}

async fn resolve_model_and_auth(
    config: &Config,
    ui_events: &Option<mpsc::UnboundedSender<AgentUiEvent>>,
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

    let spinner_active = ui_events.is_none();
    let setup = spinner_active.then(|| progress_spinner("Resolving auth..."));
    if ui_events.is_some() {
        emit_ui(ui_events, AgentUiEvent::Status("Resolving auth...".into()));
    }
    let models = builtin_models(None);
    let auth = models.get_auth(&model).await?;
    if let Some(pb) = setup {
        pb.finish_and_clear();
    }

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
                    env::debug_log(format!("tool start: {tool_name}"));
                    print_tool_call(&tool_name, verbose);
                }
                AgentEvent::ToolExecutionEnd {
                    tool_name, is_error, ..
                } => {
                    env::debug_log(format!("tool end: {tool_name} error={is_error}"));
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

fn summarize_tool_args(args: &serde_json::Value) -> String {
    args.to_string()
}

fn summarize_tool_result(result: &AgentToolResult) -> String {
    const MAX: usize = 4_096;
    let mut out = String::new();
    for block in &result.content {
        match block {
            ToolResultContent::Text(text) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&text.text);
            }
            ToolResultContent::Image(_) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str("[image output]");
            }
        }
        if out.len() >= MAX {
            out.truncate(MAX);
            out.push_str("...");
            return out;
        }
    }
    if out.is_empty() {
        let details = serde_json::to_string(&result.details).unwrap_or_default();
        if !details.is_empty() && details != "{}" && details != "null" {
            if details.len() > MAX {
                let mut truncated = details;
                truncated.truncate(MAX);
                truncated.push_str("...");
                return truncated;
            }
            return details;
        }
    }
    out
}

fn create_checkpoint_write_subscriber(
    write_ctx: TurnWriteContext,
    ui_events: Option<mpsc::UnboundedSender<AgentUiEvent>>,
    quiet: bool,
) -> elph_agent::AgentListener {
    let tool_args = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    Arc::new(move |event, _token| {
        let write_ctx = write_ctx.clone();
        let tool_args = tool_args.clone();
        let ui_events = ui_events.clone();
        Box::pin(async move {
            match event {
                AgentEvent::MessageUpdate {
                    assistant_message_event,
                    ..
                } => {
                    if let AssistantMessageEvent::TextDelta { delta, .. } = &*assistant_message_event
                        && let Err(err) = write_ctx.record_assistant_delta(delta).await
                    {
                        tracing::warn!(error = %err, "failed to persist assistant draft");
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint draft write failed: {err:#}"),
                        );
                    }
                }
                AgentEvent::ToolExecutionStart {
                    tool_call_id,
                    tool_name,
                    args,
                    ..
                } => {
                    let args_summary = summarize_tool_args(&args);
                    tool_args
                        .lock()
                        .await
                        .insert(tool_call_id.clone(), args_summary.clone());
                    if is_ask_tool(&tool_name)
                        && let Err(err) = write_ctx
                            .record_interrupt(&tool_call_id, &tool_name, &args_summary)
                            .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist interrupt");
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint interrupt write failed ({tool_name}): {err:#}"),
                        );
                    }
                }
                AgentEvent::ToolExecutionUpdate {
                    tool_call_id,
                    tool_name,
                    partial_result,
                    ..
                } => {
                    let args_summary = tool_args.lock().await.get(&tool_call_id).cloned().unwrap_or_default();
                    let output = summarize_tool_result(&partial_result);
                    if let Err(err) = write_ctx
                        .record_tool_partial(&tool_call_id, &tool_name, &args_summary, &output)
                        .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist tool partial");
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint tool partial write failed ({tool_name}): {err:#}"),
                        );
                    }
                }
                AgentEvent::ToolExecutionEnd {
                    tool_call_id,
                    tool_name,
                    is_error,
                    result,
                    ..
                } => {
                    let args_summary = tool_args.lock().await.remove(&tool_call_id).unwrap_or_default();
                    let output = summarize_tool_result(&result);
                    if is_ask_tool(&tool_name)
                        && let Err(err) = write_ctx
                            .record_resume(&tool_call_id, &tool_name, &output, is_error)
                            .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist resume");
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint resume write failed ({tool_name}): {err:#}"),
                        );
                    }
                    if let Err(err) = write_ctx
                        .record_tool_result(&tool_call_id, &tool_name, &args_summary, is_error, &output)
                        .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist tool write");
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint tool write failed ({tool_name}): {err:#}"),
                        );
                    }
                }
                _ => {}
            }
        })
    })
}

fn create_tui_event_subscriber(
    ui_events: mpsc::UnboundedSender<AgentUiEvent>,
    stream_text: bool,
    show_thinking: bool,
) -> elph_agent::AgentListener {
    Arc::new(move |event, _token| {
        let ui_events = ui_events.clone();
        Box::pin(async move {
            let mapped = match event {
                AgentEvent::MessageUpdate {
                    assistant_message_event,
                    ..
                } => match &*assistant_message_event {
                    AssistantMessageEvent::TextDelta { delta, .. } if stream_text => {
                        Some(AgentUiEvent::TextDelta(delta.clone()))
                    }
                    AssistantMessageEvent::ThinkingDelta { delta, .. } if show_thinking => {
                        Some(AgentUiEvent::ThinkingDelta(delta.clone()))
                    }
                    _ => None,
                },
                AgentEvent::ToolExecutionStart {
                    tool_call_id,
                    tool_name,
                    args,
                    ..
                } => Some(AgentUiEvent::ToolStart {
                    id: tool_call_id.clone(),
                    name: tool_name.clone(),
                    args_summary: summarize_tool_args(&args),
                }),
                AgentEvent::ToolExecutionUpdate {
                    tool_call_id,
                    partial_result,
                    ..
                } => {
                    let output = summarize_tool_result(&partial_result);
                    if output.is_empty() {
                        None
                    } else {
                        Some(AgentUiEvent::ToolUpdate {
                            id: tool_call_id.clone(),
                            output,
                        })
                    }
                }
                AgentEvent::ToolExecutionEnd {
                    tool_call_id,
                    is_error,
                    result,
                    ..
                } => Some(AgentUiEvent::ToolEnd {
                    id: tool_call_id.clone(),
                    is_error,
                    output: summarize_tool_result(&result),
                }),
                _ => None,
            };
            if let Some(mapped) = mapped {
                let _ = ui_events.send(mapped);
            }
        })
    })
}

/// Run the agent with the given command.
pub async fn run_agent(opts: RunAgentOptions<'_>) -> Result<RunAgentResult> {
    let RunAgentOptions {
        command,
        system_prompt,
        user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
        mut session,
        is_followup,
        docs_snapshot_before,
        quiet,
        ui_events,
    } = opts;

    env::debug_log(format!("command={command} followup={is_followup}"));
    let start_time = Instant::now();
    let stream_text = stream || ui_events.is_some();
    let show_thinking = verbose;

    let (model, _models_arc, stream_fn) = resolve_model_and_auth(config, &ui_events).await?;
    let env = Arc::new(LocalExecutionEnv::new(cwd));

    let (mut agent_tools, base_tool_str) = if command == "chat" {
        (
            create_read_only_tools(env.clone()),
            "read, grep, find, ls (read-only mode)",
        )
    } else {
        (create_all_tools(env.clone()), "read, bash, edit, write, grep, find, ls")
    };

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
    let full_system_prompt = format!("{system_prompt}\n\nAvailable tools for this session: {tool_names_str}");

    let turn_write_ctx = if let Some(session) = session.as_mut() {
        session.ensure_bootstrap_checkpoint().await?;
        Some(session.turn_write_context())
    } else {
        None
    };

    let restored_messages = if let Some(session) = session.as_ref() {
        if is_followup || command == "chat" {
            session.load_messages().await?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let session_id = session.as_ref().map(|s| s.thread_id().to_string());

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(full_system_prompt),
            model: Some(model),
            tools: Some(agent_tools),
            messages: if restored_messages.is_empty() {
                None
            } else {
                Some(restored_messages)
            },
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        session_id,
        ..Default::default()
    });

    let generating = if quiet {
        None
    } else {
        Some(progress_spinner("Thinking..."))
    };
    let saw_any_delta = Arc::new(AtomicBool::new(false));

    if let Some(write_ctx) = turn_write_ctx {
        agent
            .subscribe(create_checkpoint_write_subscriber(write_ctx, ui_events.clone(), quiet))
            .await;
    }

    if let Some(tx) = ui_events.clone() {
        agent
            .subscribe(create_tui_event_subscriber(tx, stream_text, show_thinking))
            .await;
    } else if !quiet {
        agent
            .subscribe(create_event_subscriber(
                stream,
                verbose,
                generating.as_ref().expect("spinner").clone(),
                saw_any_delta.clone(),
            ))
            .await;
    }

    agent.prompt_text(user_prompt.to_string(), None).await?;
    agent.wait_for_idle().await;

    let elapsed = start_time.elapsed();
    let state = agent.state().await;

    if let Some(session) = session {
        session.save_messages(&state.messages, command).await?;
    }

    let docs_changed = if let Some(before) = docs_snapshot_before.as_ref() {
        let after = docs::create_snapshot(cwd)?;
        docs::has_changed(before, &after)
    } else {
        false
    };

    let completion_message = if print_mode && !stream {
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
            String::new()
        } else {
            String::new()
        }
    } else if quiet && ui_events.is_some() {
        String::new()
    } else if quiet {
        format!("Completed in {:.1}s", elapsed.as_secs_f64())
    } else {
        format!("\x1b[90mCompleted in {:.1}s\x1b[0m", elapsed.as_secs_f64())
    };

    if let Some(tx) = ui_events {
        let _ = tx.send(AgentUiEvent::RunCompleted {
            elapsed_secs: elapsed.as_secs_f64(),
        });
    }

    Ok(RunAgentResult {
        completion_message,
        docs_changed,
        skipped: false,
    })
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
        "{base}\n\n- This is an initial documentation run.\n- Assume {OWLY_DIR}/ does not yet contain useful documentation.\n- Build the documentation structure from scratch.\n- First build a repository inventory: existing docs, graph/app entrypoints, package/config files, major domain folders, tests/evals, data/schema files, skill/playbook files, and operational scripts.\n- Use git evidence during init to understand how important files and workflows came to be.\n- Create {OWLY_DIR}/quickstart.md first, then the linked section pages.\n- Use at most 8 documentation pages on the initial run unless the repository is clearly tiny.\n- Do not try to document every source file. Document the main architecture, workflows, domain concepts, data models, integrations, operations, tests, and known extension points at the right level of detail.\n- The CLI will record successful run metadata only when documentation content changes.",
        OWLY_DIR = crate::constants::OWLY_DIR
    )
}

fn create_system_prompt_for_update() -> String {
    let base = crate::prompts::create_system_prompt();
    format!(
        "{base}\n\n- This is a maintenance update run.\n- Inspect the existing {OWLY_DIR}/ documentation before editing.\n- Always use git-oriented repository evidence to understand recent changes.\n- Before editing, build a docs impact plan from the changed source files.\n- Update runs must be surgical. Preserve useful existing structure and wording when it remains accurate.\n- Only edit pages whose current content is inaccurate, incomplete, or misleading because of the recent changes.\n- Keep each concept in one canonical page.\n- Do not make formatting-only edits.\n- Use a soft diff budget: if fewer than about 5 source files changed, update at most 1-2 wiki pages.\n- Updates may be a no-op. If there are no relevant changes, do not edit files.\n- The CLI will record successful run metadata only when documentation content changes.",
        OWLY_DIR = crate::constants::OWLY_DIR
    )
}

fn create_system_prompt_for_chat() -> String {
    let base = crate::prompts::create_system_prompt();
    format!(
        "{base}\n\n- This is an interactive chat turn.\n- Answer the user's message directly.\n- Do not create or update Owly documentation unless the user explicitly asks you to modify documentation.\n- If the user asks to initialize or update the wiki, explain that they can run owly --init or owly --update."
    )
}
