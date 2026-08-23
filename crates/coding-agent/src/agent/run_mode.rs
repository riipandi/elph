//! Non-interactive `elph run` execution.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use super::events::AgentUiEvent;
use super::headless_status::HeadlessStatus;
use super::pretty_markdown::PrettyMarkdownSink;
use super::runtime::CreateSessionOptions;
use super::runtime::create_coding_session_with_events;
use super::slash_commands::{SlashDispatch, dispatch_slash_command};
use crate::cli::style::{CliStyle, S_MUTED};
use crate::platform::{Paths, Settings};
use crate::tui::labels::format_token_count;
use crate::types::{AgentMode, ThinkingLevel};

/// Headless stdout shape (Grok/Pi-inspired + pretty markdown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Raw model text as-is (token stream).
    Plain,
    /// Streaming CommonMark/markdown rendered to the terminal (rendown + crossterm width).
    Pretty,
    Json,
    StreamJson,
    StreamMessageJson,
}

impl OutputFormat {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "plain" | "text" => Ok(Self::Plain),
            "pretty" | "markdown" | "md" => Ok(Self::Pretty),
            "json" => Ok(Self::Json),
            "stream-json" | "streaming-json" => Ok(Self::StreamJson),
            "stream-message-json" | "streaming-messages-json" | "streaming-message-json" => Ok(Self::StreamMessageJson),
            other => {
                bail!("unknown --output-format `{other}` (expected plain|pretty|json|stream-json|stream-message-json)")
            }
        }
    }
}

pub struct RunModeOptions<'a> {
    pub paths: &'a Paths,
    pub settings: &'a Settings,
    pub cwd: &'a Path,
    pub prompt: &'a str,
    pub model: Option<&'a str>,
    pub resume_id: Option<&'a str>,
    pub create_if_missing: bool,
    /// Headless default is **brave** (auto-approve tools).
    pub mode: AgentMode,
    pub system_prompt_override: Option<&'a str>,
    pub no_session: bool,
    pub max_turns: Option<u32>,
    pub output_format: OutputFormat,
    pub effort: Option<ThinkingLevel>,
    pub name: Option<&'a str>,
}

pub struct RunModeResult {
    pub session_id: String,
    pub session_name: Option<String>,
    pub assistant_text: String,
}

/// Ctrl+C / SIGINT cancelled a headless run. CLI maps this to exit 130.
#[derive(Debug, Clone, Copy)]
pub struct RunInterrupted;

impl std::fmt::Display for RunInterrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Interrupted.")
    }
}

impl std::error::Error for RunInterrupted {}

/// One SIGINT. After `tokio::signal::ctrl_c` is first polled, the default
/// terminate disposition is replaced — every later Ctrl+C must be awaited.
async fn wait_for_ctrl_c() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        log::warn!("ctrl_c listener: {err}");
        std::future::pending::<()>().await;
    }
}

pub async fn run_non_interactive(options: RunModeOptions<'_>) -> Result<RunModeResult> {
    // Wait line only for non-plain formats (pretty/json/streams). Plain = raw model
    // text only — no spinner / status chrome.
    let status = if matches!(options.output_format, OutputFormat::Plain) {
        HeadlessStatus::silent()
    } else {
        let s = HeadlessStatus::start(bootstrap_message(&options));
        s.set("Loading providers, tools, and session…");
        s
    };

    let create_opts = CreateSessionOptions {
        paths: options.paths,
        settings: options.settings,
        cwd: options.cwd,
        resume_id: options.resume_id,
        create_if_missing: options.create_if_missing,
        session_name: options.name,
        provider_override: None,
        model_override: options.model,
        agent_mode: Some(options.mode),
        system_prompt_override: options.system_prompt_override,
        preloaded_resources: None,
        defer_mcp_load: false,
        defer_session_gc: false,
        defer_memory_warm: false,
        headless: true,
        extension_host: None,
    };
    let session_result = tokio::select! {
        result = create_coding_session_with_events(create_opts) => result,
        _ = wait_for_ctrl_c() => {
            status.finish();
            log::warn!("headless run interrupted during session create");
            return Err(RunInterrupted.into());
        }
    };

    let (session, mut ui_rx) = match session_result {
        Ok(pair) => pair,
        Err(err) => {
            status.finish();
            log::error!("headless session create failed: {err:#}");
            return Err(err);
        }
    };
    let session = Arc::new(session);
    session.start_worker_inbox_poller();

    if let Some(level) = options.effort {
        status.set(format!("Setting effort ({})…", level.label()));
        if let Err(err) = session.set_thinking_level(level).await {
            status.finish();
            return Err(err);
        }
    }
    if let Some(name) = options.name
        && !name.trim().is_empty()
    {
        let _ = session.harness().set_session_name(name.trim()).await;
    }

    let session_id = session.session_id().to_string();
    let model_label = format!("{}/{}", session.model_provider(), session.model_id());
    log::info!(
        "headless session ready id={session_id} model={model_label} mode={:?}",
        options.mode
    );
    let format = options.output_format;
    let max_turns = options.max_turns;
    let tool_starts = Arc::new(AtomicU32::new(0));
    let tool_starts_watch = Arc::clone(&tool_starts);
    let plain_streamed = Arc::new(AtomicBool::new(false));
    let plain_streamed_w = Arc::clone(&plain_streamed);
    let harness_for_abort = session.harness();

    let turn_kind = match resolve_headless_turn(&session, options.prompt).await {
        Ok(kind) => kind,
        Err(err) => {
            status.finish();
            return Err(err);
        }
    };
    match &turn_kind {
        HeadlessTurn::Prompt => {
            status.set(format!("Running · {model_label} · mode {}", options.mode.footer_label()));
        }
        HeadlessTurn::Skill { name, .. } => {
            status.set(format!("Skill `{name}` · {model_label}…"));
        }
        HeadlessTurn::PromptTemplate { name, .. } => {
            status.set(format!("Prompt `/{name}` · {model_label}…"));
        }
    }
    let turn_kind_footer = turn_kind_label(&turn_kind);

    // Event task: live status + streaming plain / pretty markdown.
    let status_handle = status.handle();
    let mode_label = options.mode.footer_label().to_string();
    let model_for_events = model_label.clone();
    let stream_task = tokio::spawn(async move {
        let mut msg_started = false;
        let mut streaming_out = false;
        let mut pretty = PrettyMarkdownSink::new();
        while let Some(event) = ui_rx.recv().await {
            match format {
                // Plain: raw response only (no tool chrome / status).
                OutputFormat::Plain => {
                    if let AgentUiEvent::TextDelta(text) = &event {
                        streaming_out = true;
                        print!("{text}");
                        let _ = std::io::stdout().flush();
                        plain_streamed_w.store(true, Ordering::Relaxed);
                    }
                }
                OutputFormat::Pretty => match &event {
                    AgentUiEvent::TextDelta(text) => {
                        if !streaming_out {
                            status_handle.finish();
                            streaming_out = true;
                        }
                        if let Err(err) = pretty.push_delta(text) {
                            log::warn!("pretty markdown render: {err}");
                        }
                        if pretty.wrote_output() {
                            plain_streamed_w.store(true, Ordering::Relaxed);
                        }
                    }
                    AgentUiEvent::ToolStart { name, args_summary, .. } if streaming_out => {
                        emit_tool_stderr(name, args_summary);
                    }
                    _ if !streaming_out => {
                        update_status_for_event(&status_handle, &event, &model_for_events, &mode_label);
                    }
                    _ => {}
                },
                OutputFormat::Json => {
                    update_status_for_event(&status_handle, &event, &model_for_events, &mode_label);
                }
                OutputFormat::StreamJson => {
                    if matches!(
                        &event,
                        AgentUiEvent::TextDelta(_) | AgentUiEvent::ToolStart { .. } | AgentUiEvent::ThinkingDelta(_)
                    ) {
                        status_handle.finish_quiet();
                    } else {
                        update_status_for_event(&status_handle, &event, &model_for_events, &mode_label);
                    }
                    if let Some(line) = stream_json_line(&event) {
                        println!("{line}");
                        let _ = std::io::stdout().flush();
                    }
                }
                OutputFormat::StreamMessageJson => {
                    if matches!(&event, AgentUiEvent::TextDelta(_) | AgentUiEvent::ToolStart { .. }) {
                        status_handle.finish_quiet();
                    } else {
                        update_status_for_event(&status_handle, &event, &model_for_events, &mode_label);
                    }
                    for line in stream_message_json_lines(&event, &mut msg_started) {
                        println!("{line}");
                        let _ = std::io::stdout().flush();
                    }
                }
            }
            if let AgentUiEvent::ToolStart { .. } = &event {
                let n = tool_starts_watch.fetch_add(1, Ordering::Relaxed) + 1;
                if let Some(max) = max_turns
                    && n > max
                {
                    log::warn!("max-turns exceeded ({n} > {max}); aborting run");
                    status_handle.set(format!("Max turns reached ({n}/{max}) — aborting…"));
                    let _ = harness_for_abort.abort().await;
                }
            }
            if matches!(event, AgentUiEvent::RunCompleted { .. }) {
                break;
            }
        }
        if matches!(format, OutputFormat::Pretty) {
            if let Err(err) = pretty.finish() {
                log::warn!("pretty markdown finalize: {err}");
            }
            if pretty.wrote_output() {
                plain_streamed_w.store(true, Ordering::Relaxed);
            }
        }
    });

    // Keep the turn future alive while aborting: dropping `prompt()` skips
    // `finish_run`, and `abort()` → `wait_for_idle` would hang forever.
    let prompt_owned = options.prompt.to_string();
    let session_for_turn = Arc::clone(&session);
    let mut prompt_task =
        tokio::spawn(async move { execute_headless_input(&session_for_turn, &turn_kind, &prompt_owned).await });

    let prompt_result = tokio::select! {
        join = &mut prompt_task => match join {
            Ok(result) => result,
            Err(err) => Err(anyhow::anyhow!("headless turn task: {err}")),
        },
        _ = wait_for_ctrl_c() => {
            status.set("Interrupted — aborting…");
            tokio::select! {
                result = session.abort() => {
                    if let Err(err) = result {
                        log::warn!("headless abort: {err:#}");
                    }
                }
                _ = wait_for_ctrl_c() => {
                    status.finish();
                    eprintln!("Interrupted.");
                    std::process::exit(130);
                }
                () = tokio::time::sleep(Duration::from_secs(3)) => {
                    status.finish();
                    eprintln!("Interrupted.");
                    std::process::exit(130);
                }
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), prompt_task).await;
            Err(RunInterrupted.into())
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(5), stream_task).await;

    // Ensure wait line is gone (no-op if already finished on first token).
    status.finish();

    let assistant_text = collect_last_assistant_text(&session).await;
    let session_name = session.harness().session_name().await;
    let (tokens_used, context_limit) = session
        .estimate_context_usage()
        .await
        .unwrap_or((0, session.context_window() as u64));
    let model_label = format!("{}/{}", session.model_provider(), session.model_id());
    let turn_meta = TurnMeta {
        session_id: session_id.clone(),
        session_name: session_name.clone(),
        model: model_label.clone(),
        mode: options.mode,
        tokens_used,
        context_limit: context_limit.max(1),
        cwd: options.cwd.display().to_string(),
        turn_kind: turn_kind_footer,
    };

    if let Err(err) = prompt_result {
        let interrupted = err.downcast_ref::<RunInterrupted>().is_some();
        if format == OutputFormat::Json {
            let body = json!({
                "ok": false,
                "error": err.to_string(),
                "session": turn_meta.to_json(),
                "result": assistant_text,
            });
            println!("{}", serde_json::to_string_pretty(&body)?);
        } else if interrupted {
            eprintln!("Interrupted.");
        } else {
            eprintln!("error: {err:#}");
        }
        if options.no_session {
            let _ = session.session_manager().delete_by_id(&session_id).await;
        } else {
            // Discard the session record when the headless run never produced a turn
            // (e.g. prompt rejected/aborted before any agent turn persisted).
            if let Err(e) = session.session_manager().delete_if_no_turns(&session_id).await {
                log::warn!("delete empty session on run error: {e:#}");
            }
            emit_turn_footer(format, &turn_meta);
        }
        return Err(err);
    }

    match format {
        OutputFormat::Plain => {
            if !plain_streamed.load(Ordering::Relaxed) {
                if !assistant_text.is_empty() {
                    println!("{assistant_text}");
                }
            } else {
                println!();
            }
        }
        OutputFormat::Pretty => {
            // Primary path streams via PrettyMarkdownSink. Fallback: full answer once.
            if !plain_streamed.load(Ordering::Relaxed) && !assistant_text.is_empty() {
                let mut pretty = PrettyMarkdownSink::new();
                pretty.push_delta(&assistant_text)?;
                pretty.finish()?;
                println!();
            } else if plain_streamed.load(Ordering::Relaxed) {
                println!();
            }
        }
        OutputFormat::Json => {
            let body = json!({
                "ok": true,
                "session": turn_meta.to_json(),
                "result": assistant_text,
            });
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        OutputFormat::StreamJson => {
            println!(
                "{}",
                json!({
                    "type": "result",
                    "ok": true,
                    "session": turn_meta.to_json(),
                    "text": assistant_text,
                })
            );
        }
        OutputFormat::StreamMessageJson => {
            println!(
                "{}",
                json!({
                    "type": "result",
                    "session": turn_meta.to_json(),
                })
            );
        }
    }

    if options.no_session {
        let _ = session.session_manager().delete_by_id(&session_id).await;
    } else {
        // Discard the session record when the headless run never produced a turn
        // (e.g. prompt rejected/aborted before any agent turn persisted).
        if let Err(e) = session.session_manager().delete_if_no_turns(&session_id).await {
            log::warn!("delete empty session on run complete: {e:#}");
        }
        emit_turn_footer(format, &turn_meta);
    }

    log::info!("headless run ok session={session_id} model={model_label}");
    Ok(RunModeResult {
        session_id,
        session_name,
        assistant_text,
    })
}

/// How the headless turn was launched (plain prompt vs skill vs template).
enum HeadlessTurn {
    Prompt,
    Skill { name: String, args: String },
    PromptTemplate { name: String, args: String },
}

fn turn_kind_label(kind: &HeadlessTurn) -> Option<String> {
    match kind {
        HeadlessTurn::Prompt => None,
        HeadlessTurn::Skill { name, .. } => Some(format!("skill:{name}")),
        HeadlessTurn::PromptTemplate { name, .. } => Some(format!("/{name}")),
    }
}

/// Map user input to a skill, prompt template, or plain prompt (TUI slash parity).
async fn resolve_headless_turn(session: &super::session::CodingAgentSession, input: &str) -> Result<HeadlessTurn> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return Ok(HeadlessTurn::Prompt);
    }

    let resources = session.harness().get_resources().await;
    let dispatch = dispatch_slash_command(
        trimmed,
        None,
        Some(resources.prompt_templates.as_slice()),
        Some(resources.skills.as_slice()),
    );

    match dispatch {
        Some(SlashDispatch::Skill { name, args }) => {
            // Unknown skill returns Unimplemented from dispatch; only known skills land here.
            Ok(HeadlessTurn::Skill { name, args })
        }
        Some(SlashDispatch::PromptTemplate { name, args }) => Ok(HeadlessTurn::PromptTemplate { name, args }),
        Some(SlashDispatch::Unimplemented(cmd)) => {
            // Friendlier errors for the two headless-supported slash families.
            if let Some(rest) = cmd.strip_prefix("/skill:") {
                let name = rest.split_whitespace().next().unwrap_or(rest);
                let available: Vec<_> = resources.skills.iter().map(|s| s.name.as_str()).collect();
                bail!(
                    "unknown skill `{name}`\n  Hint: use `/skill:name` (loaded skills: {})",
                    if available.is_empty() {
                        "none".into()
                    } else {
                        available.join(", ")
                    }
                );
            }
            if cmd.starts_with('/') {
                let name = cmd.trim_start_matches('/').split_whitespace().next().unwrap_or("");
                let available: Vec<_> = resources.prompt_templates.iter().map(|t| t.name.as_str()).collect();
                if !name.is_empty()
                    && !resources.prompt_templates.iter().any(|t| t.name == name)
                    && !resources.skills.iter().any(|s| s.name == name)
                {
                    bail!(
                        "unknown slash command `/{name}`\n  \
                         Headless supports `/skill:name [args]` and `/prompt-template-name [args]`.\n  \
                         Skills: {}\n  Templates: {}",
                        if resources.skills.is_empty() {
                            "none".into()
                        } else {
                            resources
                                .skills
                                .iter()
                                .map(|s| s.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        },
                        if available.is_empty() {
                            "none".into()
                        } else {
                            available.join(", ")
                        }
                    );
                }
            }
            bail!(
                "slash command `{cmd}` is not supported in headless mode\n  \
                 Use a plain prompt, `/skill:name [args]`, or `/template-name [args]`"
            );
        }
        Some(_) => {
            bail!(
                "this slash command is not supported in headless mode\n  \
                 Use a plain prompt, `/skill:name [args]`, or `/template-name [args]`"
            );
        }
        None => Ok(HeadlessTurn::Prompt),
    }
}

async fn execute_headless_input(
    session: &super::session::CodingAgentSession,
    kind: &HeadlessTurn,
    raw_prompt: &str,
) -> Result<()> {
    match kind {
        HeadlessTurn::Prompt => session.submit_prompt(raw_prompt.to_string(), false).await,
        HeadlessTurn::Skill { name, args } => session.invoke_skill(name, args).await,
        HeadlessTurn::PromptTemplate { name, args } => session.prompt_from_template(name, args).await,
    }
}

fn bootstrap_message(options: &RunModeOptions<'_>) -> String {
    if options.resume_id.is_some() {
        if options.create_if_missing {
            "Opening session (create if missing)…".into()
        } else {
            "Resuming session…".into()
        }
    } else if options.no_session {
        "Starting ephemeral run…".into()
    } else {
        "Starting session…".into()
    }
}

/// Refresh the wait line from agent UI events (stderr only; no-op once finished).
fn update_status_for_event(status: &HeadlessStatus, event: &AgentUiEvent, model: &str, mode: &str) {
    if status.is_finished() {
        return;
    }
    match event {
        AgentUiEvent::Status(msg) => {
            let msg = msg.trim();
            if !msg.is_empty() {
                status.set(shorten_status(msg));
            }
        }
        AgentUiEvent::ThinkingDelta(_) => {
            status.set(format!("Thinking · {model}…"));
        }
        AgentUiEvent::TextDelta(_) => {
            status.set(format!("Generating · {model}…"));
        }
        AgentUiEvent::ToolStart { name, args_summary, .. } => {
            let summary = args_summary.trim();
            if summary.is_empty() {
                status.set(format!("Tool `{name}`…"));
            } else {
                status.set(format!("Tool `{name}` · {}…", truncate_chars(summary, 48)));
            }
        }
        AgentUiEvent::ToolUpdate { .. } => {}
        AgentUiEvent::ToolEnd { is_error, .. } => {
            if *is_error {
                status.set(format!("Tool failed — continuing · {model}…"));
            } else {
                status.set(format!("Running · {model} · mode {mode}…"));
            }
        }
        AgentUiEvent::Retrying { attempt } => {
            status.set(format!("Retrying (attempt {attempt})…"));
        }
        AgentUiEvent::SubagentStatus {
            task_name,
            phase,
            message,
            ..
        } => {
            let label = if task_name.is_empty() {
                "subagent"
            } else {
                task_name.as_str()
            };
            let detail = message.trim();
            if detail.is_empty() {
                status.set(format!("Subagent {label} · {}…", phase.as_word()));
            } else {
                status.set(format!(
                    "Subagent {label} · {} · {}…",
                    phase.as_word(),
                    truncate_chars(detail, 40)
                ));
            }
        }
        AgentUiEvent::RunCompleted { .. } => {}
        AgentUiEvent::PlanConfirmationRequired(_) => {
            status.set("Waiting for plan confirmation…");
        }
        AgentUiEvent::ToolApprovalRequired(_) => {
            status.set("Waiting for tool approval…");
        }
        _ => {}
    }
}

fn emit_tool_stderr(name: &str, args_summary: &str) {
    let detail = args_summary.trim();
    let line = if detail.is_empty() {
        format!("  · tool `{name}`")
    } else {
        format!("  · tool `{name}` · {}", truncate_chars(detail, 48))
    };
    eprintln!("{}", CliStyle::auto_stderr().paint(S_MUTED, line));
}

fn shorten_status(msg: &str) -> String {
    let one_line = msg.lines().next().unwrap_or(msg).trim();
    truncate_chars(one_line, 72).to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let take = max.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

struct TurnMeta {
    session_id: String,
    session_name: Option<String>,
    model: String,
    mode: AgentMode,
    tokens_used: u64,
    context_limit: u64,
    cwd: String,
    turn_kind: Option<String>,
}

impl TurnMeta {
    fn context_pct(&self) -> f64 {
        if self.context_limit == 0 {
            0.0
        } else {
            (self.tokens_used as f64 / self.context_limit as f64) * 100.0
        }
    }

    fn context_label(&self) -> String {
        format!(
            "{} / {} ({:.1}%)",
            format_token_count(self.tokens_used),
            format_token_count(self.context_limit),
            self.context_pct()
        )
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.session_id,
            "name": self.session_name,
            "cwd": self.cwd,
            "model": self.model,
            "mode": self.mode.footer_label(),
            "tokens_used": self.tokens_used,
            "context_limit": self.context_limit,
            "context_pct": self.context_pct(),
            "turn": self.turn_kind,
        })
    }
}

/// Dimmed turn footer on stderr — separated from the AI response with blank lines.
fn emit_turn_footer(format: OutputFormat, meta: &TurnMeta) {
    // Machine formats already embed session metadata in stdout JSON; still print a
    // short dimmed footer so interactive users see it, without polluting parsers that
    // only read stdout.
    let _ = format;

    let sty = CliStyle::auto_stderr();
    let mut err = std::io::stderr().lock();

    // Spinner already ended with a newline; add one more blank line before metadata.
    let _ = writeln!(err);

    let line = |key: &str, value: &str| format!("  {:<12} {}", key, value);
    let dim = |s: String| sty.paint(S_MUTED, s);

    let _ = writeln!(err, "{}", dim(line("session", &meta.session_id)));
    if let Some(name) = meta.session_name.as_deref().filter(|s| !s.is_empty()) {
        let _ = writeln!(err, "{}", dim(line("name", name)));
    }
    if let Some(kind) = meta.turn_kind.as_deref() {
        let _ = writeln!(err, "{}", dim(line("turn", kind)));
    }
    let _ = writeln!(err, "{}", dim(line("model", &meta.model)));
    let _ = writeln!(err, "{}", dim(line("context", &meta.context_label())));
    let _ = writeln!(
        err,
        "{}",
        dim(line("resume", &format!("elph run --session-id={} \"…\"", meta.session_id)))
    );
    let _ = writeln!(err);
}

async fn collect_last_assistant_text(session: &super::session::CodingAgentSession) -> String {
    let entries = session.harness().session_entries().await;
    let mut text = String::new();
    for entry in entries.into_iter().rev() {
        if let elph_agent::session::SessionTreeEntry::Message { message, .. } = entry
            && message.role() == "assistant"
            && let Some(elph_ai::Message::Assistant(assistant)) = message.as_llm()
        {
            for block in &assistant.content {
                if let elph_ai::AssistantContentBlock::Text(t) = block {
                    text.push_str(&t.text);
                }
            }
            break;
        }
    }
    text
}

fn stream_json_line(event: &AgentUiEvent) -> Option<String> {
    let v = match event {
        AgentUiEvent::TextDelta(text) => json!({"type": "text_delta", "text": text}),
        AgentUiEvent::ThinkingDelta(text) => json!({"type": "thinking_delta", "text": text}),
        AgentUiEvent::ToolStart {
            id, name, args_summary, ..
        } => json!({
            "type": "tool_start",
            "id": id,
            "name": name,
            "args_summary": args_summary,
        }),
        AgentUiEvent::ToolEnd {
            id, is_error, output, ..
        } => json!({
            "type": "tool_end",
            "id": id,
            "is_error": is_error,
            "output": output,
        }),
        AgentUiEvent::RunCompleted { elapsed_secs, .. } => json!({
            "type": "run_completed",
            "elapsed_secs": elapsed_secs,
        }),
        AgentUiEvent::Status(s) => json!({"type": "status", "message": s}),
        _ => return None,
    };
    Some(v.to_string())
}

fn stream_message_json_lines(event: &AgentUiEvent, msg_started: &mut bool) -> Vec<String> {
    let mut out = Vec::new();
    match event {
        AgentUiEvent::TextDelta(text) => {
            if !*msg_started {
                out.push(
                    json!({
                        "type": "message_start",
                        "message": {"role": "assistant", "content": []}
                    })
                    .to_string(),
                );
                out.push(
                    json!({
                        "type": "content_block_start",
                        "index": 0,
                        "content_block": {"type": "text", "text": ""}
                    })
                    .to_string(),
                );
                *msg_started = true;
            }
            out.push(
                json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {"type": "text_delta", "text": text}
                })
                .to_string(),
            );
        }
        AgentUiEvent::RunCompleted { .. } if *msg_started => {
            out.push(json!({"type": "content_block_stop", "index": 0}).to_string());
            out.push(json!({"type": "message_stop"}).to_string());
            *msg_started = false;
        }
        _ => {}
    }
    out
}

/// Resolve system prompt CLI value: literal text, `@path`, or existing file path.
pub fn resolve_system_prompt_arg(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if let Some(path) = trimmed.strip_prefix('@') {
        return std::fs::read_to_string(path).with_context(|| format!("read --system-prompt file `{path}`"));
    }
    let path = Path::new(trimmed);
    if path.is_file() {
        return std::fs::read_to_string(path)
            .with_context(|| format!("read --system-prompt file `{}`", path.display()));
    }
    Ok(trimmed.to_string())
}

pub fn parse_agent_mode(raw: &str) -> Result<AgentMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "build" => Ok(AgentMode::Build),
        "plan" => Ok(AgentMode::Plan),
        "ask" => Ok(AgentMode::Ask),
        "brave" => Ok(AgentMode::Brave),
        other => bail!("unknown --mode `{other}` (expected build|plan|ask|brave)"),
    }
}

pub fn parse_effort(raw: &str) -> Result<ThinkingLevel> {
    let level = ThinkingLevel::from_setting(raw);
    // from_setting maps unknown → Off; reject clearly bad tokens except "off".
    let key = raw.trim().to_ascii_lowercase();
    match key.as_str() {
        "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "x-high" | "max" => Ok(level),
        _ => bail!("unknown --effort `{raw}` (expected off|low|medium|high|xhigh|max)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::harness::{PromptTemplate, Skill};

    #[test]
    fn output_format_aliases() {
        assert_eq!(OutputFormat::parse("text").unwrap(), OutputFormat::Plain);
        assert_eq!(OutputFormat::parse("plain").unwrap(), OutputFormat::Plain);
        assert_eq!(OutputFormat::parse("pretty").unwrap(), OutputFormat::Pretty);
        assert_eq!(OutputFormat::parse("markdown").unwrap(), OutputFormat::Pretty);
        assert_eq!(OutputFormat::parse("streaming-json").unwrap(), OutputFormat::StreamJson);
        assert_eq!(
            OutputFormat::parse("streaming-messages-json").unwrap(),
            OutputFormat::StreamMessageJson
        );
    }

    #[test]
    fn agent_mode_parse() {
        assert_eq!(parse_agent_mode("brave").unwrap(), AgentMode::Brave);
        assert_eq!(parse_agent_mode("PLAN").unwrap(), AgentMode::Plan);
        assert!(parse_agent_mode("rpc").is_err());
    }

    #[test]
    fn effort_parse() {
        assert_eq!(parse_effort("high").unwrap(), ThinkingLevel::High);
        assert_eq!(parse_effort("off").unwrap(), ThinkingLevel::Off);
        assert!(parse_effort("turbo").is_err());
    }

    #[test]
    fn run_interrupted_downcast() {
        let err: anyhow::Error = RunInterrupted.into();
        assert!(err.downcast_ref::<RunInterrupted>().is_some());
        assert_eq!(err.to_string(), "Interrupted.");
    }

    #[test]
    fn slash_dispatch_skill_and_template() {
        let skills = [Skill {
            name: "code-review".into(),
            description: "Review".into(),
            content: "Review the code".into(),
            file_path: "/tmp/SKILL.md".into(),
            ..Default::default()
        }];
        let templates = [PromptTemplate {
            name: "ship-it".into(),
            description: "Ship".into(),
            content: "Ship $ARGS".into(),
            argument_hint: None,
            file_path: "/tmp/ship-it.md".into(),
        }];

        match dispatch_slash_command("/skill:code-review src/", None, Some(&templates), Some(&skills)) {
            Some(SlashDispatch::Skill { name, args }) => {
                assert_eq!(name, "code-review");
                assert_eq!(args, "src/");
            }
            other => panic!("expected Skill, got {other:?}"),
        }
        match dispatch_slash_command("/ship-it fast", None, Some(&templates), Some(&skills)) {
            Some(SlashDispatch::PromptTemplate { name, args }) => {
                assert_eq!(name, "ship-it");
                assert_eq!(args, "fast");
            }
            other => panic!("expected PromptTemplate, got {other:?}"),
        }
    }

    #[test]
    fn turn_meta_context_label() {
        let meta = TurnMeta {
            session_id: "abc".into(),
            session_name: None,
            model: "openai/gpt".into(),
            mode: AgentMode::Brave,
            tokens_used: 12_000,
            context_limit: 200_000,
            cwd: "/tmp".into(),
            turn_kind: Some("skill:code-review".into()),
        };
        assert_eq!(meta.context_label(), "12K / 200K (6.0%)");
        let j = meta.to_json();
        assert_eq!(j["id"], "abc");
        assert_eq!(j["tokens_used"], 12_000);
    }
}
