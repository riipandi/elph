//! Non-interactive `elph run` execution.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::events::AgentUiEvent;
use super::runtime::CreateSessionOptions;
use super::runtime::create_coding_session_with_events;
use crate::platform::{Paths, Settings};
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
    let (session, mut ui_rx) = create_coding_session_with_events(CreateSessionOptions {
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
    .await?;
    let session = Arc::new(session);
    session.start_worker_inbox_poller();

    if let Some(level) = options.effort {
        session.set_thinking_level(level).await?;
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
    let plain_streamed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let plain_streamed_w = Arc::clone(&plain_streamed);
    let harness_for_abort = session.harness();

    // Stream UI events to stdout for streaming formats / plain deltas.
    let stream_task = tokio::spawn(async move {
        let mut msg_started = false;
        while let Some(event) = ui_rx.recv().await {
            match format {
                OutputFormat::Plain => {
                    if let AgentUiEvent::TextDelta(text) = &event {
                        print!("{text}");
                        let _ = std::io::stdout().flush();
                        plain_streamed_w.store(true, Ordering::Relaxed);
                    }
                }
                OutputFormat::StreamJson => {
                    if let Some(line) = stream_json_line(&event) {
                        println!("{line}");
                        let _ = std::io::stdout().flush();
                    }
                }
                OutputFormat::StreamMessageJson => {
                    for line in stream_message_json_lines(&event, &mut msg_started) {
                        println!("{line}");
                        let _ = std::io::stdout().flush();
                    }
                }
                OutputFormat::Json => {
                    // Collect only via final transcript; ignore live events.
                }
            }
            if let AgentUiEvent::ToolStart { .. } = &event {
                let n = tool_starts_watch.fetch_add(1, Ordering::Relaxed) + 1;
                if let Some(max) = max_turns
                    && n > max
                {
                    log::warn!("max-turns exceeded ({n} > {max}); aborting run");
                    let _ = harness_for_abort.abort().await;
                }
            }
            if matches!(event, AgentUiEvent::RunCompleted { .. }) {
                break;
            }
        }
    });

    let prompt_result = session.submit_prompt(options.prompt.to_string(), false).await;
    // Allow stream task to drain RunCompleted.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), stream_task).await;

    let assistant_text = collect_last_assistant_text(&session).await;
    let session_name = session.harness().session_name().await;

    if let Err(err) = prompt_result {
        if format == OutputFormat::Json {
            let body = json!({
                "ok": false,
                "error": err.to_string(),
                "session": session_info_json(&session_id, session_name.as_deref(), options.cwd, &model_label, options.mode),
                "result": assistant_text,
            });
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        // Still clean up ephemeral sessions.
        if options.no_session {
            let _ = session.session_manager().delete_by_id(&session_id).await;
        } else {
            emit_session_trailer(format, &session_id, session_name.as_deref());
        }
        return Err(err);
    }

    if format == OutputFormat::Plain {
        if plain_streamed.load(Ordering::Relaxed) {
            println!();
        } else if !assistant_text.is_empty() {
            println!("{assistant_text}");
        }
    } else if format == OutputFormat::Json {
        let body = json!({
            "ok": true,
            "session": session_info_json(&session_id, session_name.as_deref(), options.cwd, &model_label, options.mode),
            "result": assistant_text,
        });
        println!("{}", serde_json::to_string_pretty(&body)?);
    } else if format == OutputFormat::StreamJson {
        println!(
            "{}",
            json!({
                "type": "result",
                "ok": true,
                "session_id": session_id,
                "text": assistant_text,
            })
        );
    } else if format == OutputFormat::StreamMessageJson {
        println!(
            "{}",
            json!({
                "type": "result",
                "session_id": session_id,
            })
        );
    }

    if options.no_session {
        let _ = session.session_manager().delete_by_id(&session_id).await;
    } else {
        emit_session_trailer(format, &session_id, session_name.as_deref());
    }

    Ok(RunModeResult {
        session_id,
        session_name,
        assistant_text,
    })
}

fn session_info_json(session_id: &str, name: Option<&str>, cwd: &Path, model: &str, mode: AgentMode) -> Value {
    json!({
        "id": session_id,
        "name": name,
        "cwd": cwd.display().to_string(),
        "model": model,
        "mode": mode.footer_label(),
    })
}

fn emit_session_trailer(format: OutputFormat, session_id: &str, name: Option<&str>) {
    // Trailer on stderr so stdout stays machine-parseable for json/stream formats.
    let mut err = std::io::stderr().lock();
    let name_part = name.map(|n| format!(" name={n}")).unwrap_or_default();
    let _ = writeln!(err, "elph: session_id={session_id}{name_part}");
    let _ = writeln!(err, "elph: resume: elph run --session-id={session_id} \"…\"");
    if matches!(format, OutputFormat::Plain) {
        // Already on stderr; nothing extra for plain.
    }
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
}
