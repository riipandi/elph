//! Non-interactive `elph run` execution.

use anyhow::{Context, Result, bail};
use elph_tui::CliSpinner;
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::events::AgentUiEvent;
use super::runtime::CreateSessionOptions;
use super::runtime::create_coding_session_with_events;
use super::slash_commands::{SlashDispatch, dispatch_slash_command};
use crate::cli::style::{CliStyle, S_MUTED};
use crate::platform::{Paths, Settings};
use crate::tui::labels::format_token_count;
use crate::types::{AgentMode, ThinkingLevel};

/// Headless stdout shape (Grok/Pi-inspired).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Plain,
    Json,
    StreamJson,
    StreamMessageJson,
}

impl OutputFormat {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "plain" | "text" => Ok(Self::Plain),
            "json" => Ok(Self::Json),
            "stream-json" | "streaming-json" => Ok(Self::StreamJson),
            "stream-message-json" | "streaming-messages-json" | "streaming-message-json" => Ok(Self::StreamMessageJson),
            other => bail!("unknown --output-format `{other}` (expected plain|json|stream-json|stream-message-json)"),
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

pub async fn run_non_interactive(options: RunModeOptions<'_>) -> Result<RunModeResult> {
    // Spinner on stderr only. Always torn down via `SpinnerGuard` so a residual
    // `\r` line cannot stick after the response (the previous failure mode).
    let mut status = RunStatus::start(bootstrap_message(&options));

    status.set("Loading providers, tools, and session…");
    let session_result = create_coding_session_with_events(CreateSessionOptions {
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
        headless: true,
    })
    .await;

    let (session, mut ui_rx) = match session_result {
        Ok(pair) => pair,
        Err(err) => {
            status.finish();
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
    let format = options.output_format;
    let max_turns = options.max_turns;
    let tool_starts = Arc::new(AtomicU32::new(0));
    let tool_starts_watch = Arc::clone(&tool_starts);
    let harness_for_abort = session.harness();

    // Resolve `/skill:…` and `/template-name` (same dispatch as TUI) before the turn.
    let turn_kind = match resolve_headless_turn(&session, options.prompt).await {
        Ok(kind) => kind,
        Err(err) => {
            status.finish();
            return Err(err);
        }
    };
    match &turn_kind {
        HeadlessTurn::Prompt => {
            status.set(format!(
                "Running · {model_label} · mode {}",
                options.mode.footer_label()
            ));
        }
        HeadlessTurn::Skill { name, .. } => {
            status.set(format!("Skill `{name}` · {model_label}…"));
        }
        HeadlessTurn::PromptTemplate { name, .. } => {
            status.set(format!("Prompt `/{name}` · {model_label}…"));
        }
    }

    // Event task: update spinner for tools/thinking; stream-json formats write live
    // NDJSON. Plain mode does **not** stream tokens while the spinner is active —
    // the final answer is printed after the spinner is cleared (clean separation).
    let status_handle = status.handle();
    let mode_label = options.mode.footer_label().to_string();
    let model_for_events = model_label.clone();
    let stream_task = tokio::spawn(async move {
        let mut msg_started = false;
        while let Some(event) = ui_rx.recv().await {
            update_status_for_event(&status_handle, &event, &model_for_events, &mode_label);

            match format {
                OutputFormat::Plain | OutputFormat::Json => {
                    // Final text is collected from the session tree after the turn.
                }
                OutputFormat::StreamJson => {
                    // Clear wait spinner once machine output starts so NDJSON isn't
                    // interleaved with a \r spinner on the same visual row in odd TTYs.
                    if matches!(
                        &event,
                        AgentUiEvent::TextDelta(_)
                            | AgentUiEvent::ToolStart { .. }
                            | AgentUiEvent::ThinkingDelta(_)
                    ) {
                        status_handle.finish_quiet();
                    }
                    if let Some(line) = stream_json_line(&event) {
                        println!("{line}");
                        let _ = std::io::stdout().flush();
                    }
                }
                OutputFormat::StreamMessageJson => {
                    if matches!(&event, AgentUiEvent::TextDelta(_) | AgentUiEvent::ToolStart { .. }) {
                        status_handle.finish_quiet();
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
    });

    let prompt_result = execute_headless_input(&session, &turn_kind, options.prompt).await;
    // Drain RunCompleted (or time out if the harness never emitted it).
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), stream_task).await;

    // Hide spinner + newline so the model answer / footer never share a line with it.
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
        turn_kind: turn_kind_label(&turn_kind),
    };

    if let Err(err) = prompt_result {
        if format == OutputFormat::Json {
            let body = json!({
                "ok": false,
                "error": err.to_string(),
                "session": turn_meta.to_json(),
                "result": assistant_text,
            });
            println!("{}", serde_json::to_string_pretty(&body)?);
        } else {
            eprintln!("error: {err:#}");
        }
        if options.no_session {
            let _ = session.session_manager().delete_by_id(&session_id).await;
        } else {
            emit_turn_footer(format, &turn_meta);
        }
        return Err(err);
    }

    match format {
        OutputFormat::Plain => {
            if !assistant_text.is_empty() {
                println!("{assistant_text}");
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
        emit_turn_footer(format, &turn_meta);
    }

    Ok(RunModeResult {
        session_id,
        session_name,
        assistant_text,
    })
}

/// Owns the headless wait spinner and guarantees it is cleared (with a newline).
struct RunStatus {
    spinner: CliSpinner,
    finished: bool,
}

/// Cheap clone for the event task — shares the same underlying spinner.
#[derive(Clone)]
struct RunStatusHandle {
    spinner: CliSpinner,
}

impl RunStatus {
    fn start(message: impl Into<String>) -> Self {
        Self {
            spinner: CliSpinner::new(message),
            finished: false,
        }
    }

    fn handle(&self) -> RunStatusHandle {
        RunStatusHandle {
            spinner: self.spinner.clone(),
        }
    }

    fn set(&self, message: impl Into<String>) {
        if !self.finished {
            self.spinner.set_message(message);
        }
    }

    /// Clear spinner and leave a clean line for the answer / footer.
    fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.spinner.finish_and_clear_with_newline();
    }
}

impl Drop for RunStatus {
    fn drop(&mut self) {
        self.finish();
    }
}

impl RunStatusHandle {
    fn set(&self, message: impl Into<String>) {
        self.spinner.set_message(message);
    }

    /// Stop spinner without forcing an extra blank line (stream formats already write).
    fn finish_quiet(&self) {
        self.spinner.finish_and_clear();
    }
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
async fn resolve_headless_turn(
    session: &super::session::CodingAgentSession,
    input: &str,
) -> Result<HeadlessTurn> {
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
        Some(SlashDispatch::PromptTemplate { name, args }) => {
            Ok(HeadlessTurn::PromptTemplate { name, args })
        }
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
                let available: Vec<_> = resources
                    .prompt_templates
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect();
                if !name.is_empty()
                    && !resources
                        .prompt_templates
                        .iter()
                        .any(|t| t.name == name)
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

/// Refresh the wait spinner from agent UI events (stderr only).
fn update_status_for_event(status: &RunStatusHandle, event: &AgentUiEvent, model: &str, mode: &str) {
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
            let label = if task_name.is_empty() { "subagent" } else { task_name.as_str() };
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
        AgentUiEvent::RunCompleted { .. } => {
            // Spinner is cleared by the main path after the turn; avoid a flash message.
        }
        AgentUiEvent::PlanConfirmationRequired(_) => {
            status.set("Waiting for plan confirmation…");
        }
        AgentUiEvent::ToolApprovalRequired(_) => {
            status.set("Waiting for tool approval…");
        }
        _ => {}
    }
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
        dim(line(
            "resume",
            &format!("elph run --session-id={} \"…\"", meta.session_id)
        ))
    );
    let _ = writeln!(err);
}

async fn collect_last_assistant_text(session: &super::session::CodingAgentSession) -> String {
    let entries = session.harness().session_entries().await;
    let mut text = String::new();
    for entry in entries.into_iter().rev() {
        if let elph_agent::SessionTreeEntry::Message { message, .. } = entry
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
        AgentUiEvent::RunCompleted { elapsed_secs } => json!({
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
        AgentUiEvent::RunCompleted { .. } => {
            if *msg_started {
                out.push(json!({"type": "content_block_stop", "index": 0}).to_string());
                out.push(json!({"type": "message_stop"}).to_string());
                *msg_started = false;
            }
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
    use elph_agent::{PromptTemplate, Skill};

    #[test]
    fn output_format_aliases() {
        assert_eq!(OutputFormat::parse("text").unwrap(), OutputFormat::Plain);
        assert_eq!(OutputFormat::parse("plain").unwrap(), OutputFormat::Plain);
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
