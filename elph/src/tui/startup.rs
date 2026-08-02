//! TUI startup bootstrap: staged agent session creation and deferred MCP discovery.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use elph_agent::{FileSystem, McpLoadReport, McpServerLoadProgress};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use chrono::{DateTime, Utc};

use crate::agent::{AgentUiEvent, CodingAgentSession, CreateSessionOptions, LoadResourcesResult};
use crate::agent::{create_coding_session_with_events, format_resource_conflict_notice, format_resource_load_warnings};
use crate::platform::{Paths, Settings};
use crate::tui::transcript::markdown::AssistantMarkdownBuffer;
use crate::tui::transcript::markdown::parse_markdown_on_worker;
use crate::tui::transcript::{TranscriptMessage, TranscriptStyle};

/// Middle-dot separator for startup copy (` · `).
pub const STARTUP_SEP: &str = " · ";
/// Unicode ellipsis for in-progress startup lines.
pub const STARTUP_ELLIPSIS: &str = "…";
/// Nest indent (cells) for per-server MCP rows under the section header.
/// Applied as whole-row `status_indent` so the status glyph stays tight to the label.
pub const STARTUP_MCP_INDENT_CELLS: u16 = 2;
/// Indent for dimmed configuration warnings under MCP summary.
pub const STARTUP_WARN_INDENT: &str = "    ";

pub const STARTUP_KEY_PHASE: &str = "startup:phase";
pub const STARTUP_KEY_MCP_LOAD: &str = "startup:mcp-load";

pub fn mcp_server_startup_key(name: &str) -> String {
    format!("startup:mcp:{name}")
}

/// Inputs for background agent bootstrap after the TUI shell is visible.
#[derive(Debug, Clone)]
pub struct TuiBootstrapConfig {
    pub paths: Paths,
    pub settings: Settings,
    pub resume_id: Option<String>,
    pub preloaded_resources: LoadResourcesResult,
}

/// Bootstrap phases surfaced in the status row and transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapPhase {
    Pending,
    Running,
    AgentReady,
    McpLoading,
    Done,
    Failed,
}

/// Status-row label for the current bootstrap step (short; matches transcript tone).
pub fn bootstrap_activity_label(phase: BootstrapPhase, detail: Option<&str>) -> String {
    match phase {
        BootstrapPhase::Pending => String::new(),
        BootstrapPhase::Running => detail.unwrap_or("Preparing agent").to_string(),
        BootstrapPhase::AgentReady => "Agent ready".to_string(), // detail lives on the transcript line
        BootstrapPhase::McpLoading => "Loading MCP".to_string(),
        BootstrapPhase::Done => String::new(),
        BootstrapPhase::Failed => "Startup failed".to_string(),
    }
}

/// Compact status-row label while MCP servers connect (spinner + elapsed).
pub fn mcp_server_status_label(progress: &McpServerLoadProgress) -> String {
    match progress {
        McpServerLoadProgress::Started { name, index, total } => {
            format!("MCP {index}/{total}{STARTUP_SEP}{name}")
        }
        McpServerLoadProgress::Finished {
            name,
            ok: true,
            tool_count,
            ..
        } => {
            format!("MCP{STARTUP_SEP}{name}{STARTUP_SEP}{tool_count} tools")
        }
        McpServerLoadProgress::Finished { name, ok: false, .. } => {
            format!("MCP{STARTUP_SEP}{name}{STARTUP_SEP}failed")
        }
    }
}

pub fn bootstrap_is_active(phase: BootstrapPhase) -> bool {
    matches!(
        phase,
        BootstrapPhase::Running | BootstrapPhase::AgentReady | BootstrapPhase::McpLoading
    )
}

fn upsert_startup_line(
    messages: &mut Vec<TranscriptMessage>,
    key: &str,
    content: impl Into<String>,
    style: TranscriptStyle,
) {
    let content = content.into();
    if let Some(row) = messages
        .iter_mut()
        .find(|message| message.startup_key.as_deref() == Some(key))
    {
        row.content = content;
        row.style = style;
        return;
    }
    messages.push(TranscriptMessage::startup_status(key, content, style));
}

/// Opening transcript lines before async bootstrap begins.
pub fn initial_startup_messages(loaded: &LoadResourcesResult) -> Vec<TranscriptMessage> {
    let mut messages = vec![TranscriptMessage::startup_status(
        STARTUP_KEY_PHASE,
        format!("Preparing workspace{STARTUP_ELLIPSIS}"),
        TranscriptStyle::StatusRunning,
    )];
    if let Some(notice) = format_resource_conflict_notice(loaded) {
        let mut msg = TranscriptMessage::text(notice, TranscriptStyle::Meta);
        msg.sticky_meta = true;
        messages.push(msg);
    }
    if let Some(warn) = format_resource_load_warnings(loaded) {
        let mut msg = TranscriptMessage::text(warn, TranscriptStyle::Meta);
        msg.sticky_meta = true;
        messages.push(msg);
    }
    messages
}

pub fn begin_agent_startup(messages: &mut Vec<TranscriptMessage>) {
    upsert_startup_line(
        messages,
        STARTUP_KEY_PHASE,
        format!("Preparing agent{STARTUP_ELLIPSIS}"),
        TranscriptStyle::StatusRunning,
    );
}

/// Startup line when the agent session is ready, including the active model label.
///
/// - With model: `Agent ready (active model: provider/model-id)`
/// - Without: `Agent ready (active model: none)`
pub fn format_agent_ready_line(provider_id: Option<&str>, model_id: Option<&str>) -> String {
    let active = match (
        provider_id.map(str::trim).filter(|s| !s.is_empty()),
        model_id.map(str::trim).filter(|s| !s.is_empty()),
    ) {
        (Some(provider), Some(model)) => format!("{provider}/{model}"),
        _ => "none".to_string(),
    };
    format!("Agent ready (active model: {active})")
}

pub fn mark_agent_startup_ready(
    messages: &mut Vec<TranscriptMessage>,
    provider_id: Option<&str>,
    model_id: Option<&str>,
) {
    upsert_startup_line(
        messages,
        STARTUP_KEY_PHASE,
        format_agent_ready_line(provider_id, model_id),
        TranscriptStyle::StatusSuccess,
    );
}

pub fn mark_agent_startup_failed(messages: &mut Vec<TranscriptMessage>, err: &str) {
    upsert_startup_line(
        messages,
        STARTUP_KEY_PHASE,
        format!("Startup failed{STARTUP_SEP}{err}"),
        TranscriptStyle::StatusFailed,
    );
}

pub fn begin_mcp_startup(messages: &mut Vec<TranscriptMessage>, enabled_servers: usize) {
    upsert_startup_line(
        messages,
        STARTUP_KEY_MCP_LOAD,
        format_mcp_loading_header(enabled_servers),
        TranscriptStyle::StatusRunning,
    );
}

pub fn apply_mcp_startup_summary_line(messages: &mut Vec<TranscriptMessage>, summary: &str) {
    upsert_startup_line(messages, STARTUP_KEY_MCP_LOAD, summary, TranscriptStyle::StatusSuccess);
}

/// Append a dim configuration warning under the MCP block.
pub fn append_startup_warning(messages: &mut Vec<TranscriptMessage>, warning: &str) {
    let warning = warning.trim();
    if warning.is_empty() {
        return;
    }
    messages.push(TranscriptMessage::text(
        format!("{STARTUP_WARN_INDENT}{warning}"),
        TranscriptStyle::Meta,
    ));
}

/// Transcript section header while MCP servers load.
pub fn format_mcp_loading_header(enabled_servers: usize) -> String {
    if enabled_servers == 0 {
        format!("Loading MCP{STARTUP_SEP}none configured")
    } else {
        let noun = if enabled_servers == 1 { "server" } else { "servers" };
        format!("Loading MCP{STARTUP_SEP}{enabled_servers} {noun}")
    }
}

fn format_mcp_server_line(name: &str, detail: &str) -> String {
    // No leading spaces — nesting is `status_indent` on the transcript row.
    format!("MCP server \"{name}\"{STARTUP_SEP}{detail}")
}

/// Transcript text for one MCP server progress event.
pub fn format_mcp_server_transcript(progress: &McpServerLoadProgress) -> String {
    match progress {
        McpServerLoadProgress::Started { name, .. } => {
            format_mcp_server_line(name, &format!("connecting{STARTUP_ELLIPSIS}"))
        }
        McpServerLoadProgress::Finished {
            name,
            ok: true,
            transport,
            tool_count,
            ..
        } => {
            let tool_label = if *tool_count == 1 { "tool" } else { "tools" };
            format_mcp_server_line(name, &format!("{tool_count} {tool_label}{STARTUP_SEP}{transport}"))
        }
        McpServerLoadProgress::Finished {
            name,
            ok: false,
            message,
            ..
        } => format_mcp_server_line(name, message),
    }
}

pub fn mcp_server_transcript_style(progress: &McpServerLoadProgress) -> TranscriptStyle {
    match progress {
        McpServerLoadProgress::Started { .. } => TranscriptStyle::StatusRunning,
        McpServerLoadProgress::Finished { ok: true, .. } => TranscriptStyle::StatusSuccess,
        McpServerLoadProgress::Finished { ok: false, .. } => TranscriptStyle::StatusFailed,
    }
}

/// One-line totals after per-server MCP lines complete.
pub fn format_mcp_load_summary(report: &McpLoadReport) -> String {
    if report.servers_ok == 0 && report.servers_failed == 0 && report.tools_loaded == 0 {
        format!("MCP ready{STARTUP_SEP}none configured")
    } else {
        let connected = if report.servers_ok == 1 {
            "1 connected".to_string()
        } else {
            format!("{} connected", report.servers_ok)
        };
        let failed = if report.servers_failed == 1 {
            "1 failed".to_string()
        } else {
            format!("{} failed", report.servers_failed)
        };
        let tools = if report.tools_loaded == 1 {
            "1 tool".to_string()
        } else {
            format!("{} tools", report.tools_loaded)
        };
        format!("MCP ready{STARTUP_SEP}{connected}{STARTUP_SEP}{failed}{STARTUP_SEP}{tools}")
    }
}

/// Footer lines after per-server progress (summary upsert + config warnings).
pub fn format_mcp_load_footer(report: &McpLoadReport, config_warnings: &[String]) -> Vec<String> {
    let mut lines = Vec::with_capacity(1 + config_warnings.len());
    lines.push(format_mcp_load_summary(report));
    lines.extend(config_warnings.iter().cloned());
    lines
}

/// Upsert a colored MCP status row (connecting → connected/failed on the same line).
pub fn apply_mcp_server_progress(messages: &mut Vec<TranscriptMessage>, progress: &McpServerLoadProgress) {
    let name = match progress {
        McpServerLoadProgress::Started { name, .. } | McpServerLoadProgress::Finished { name, .. } => name,
    };
    let key = mcp_server_startup_key(name);
    let content = format_mcp_server_transcript(progress);
    let style = mcp_server_transcript_style(progress);
    upsert_startup_line(messages, &key, content, style);
    if let Some(row) = messages
        .iter_mut()
        .find(|message| message.startup_key.as_deref() == Some(key.as_str()))
    {
        // Indent glyph+label together under the MCP summary header.
        row.status_indent = STARTUP_MCP_INDENT_CELLS;
    }
}

/// Classify a line emitted from [`format_mcp_load_footer`] after the summary row.
pub fn classify_mcp_footer_line(line: &str) -> McpFooterLineKind {
    if line.trim_start().starts_with(STARTUP_WARN_INDENT) {
        McpFooterLineKind::Warning(line.trim_start().to_string())
    } else if line.starts_with("MCP ready") || line.starts_with("MCP failed") {
        McpFooterLineKind::Summary(line.to_string())
    } else {
        McpFooterLineKind::Warning(line.to_string())
    }
}

pub enum McpFooterLineKind {
    Summary(String),
    Warning(String),
}

pub struct AgentBootstrap {
    pub session: Arc<CodingAgentSession>,
    pub ui_rx: Arc<Mutex<UnboundedReceiver<AgentUiEvent>>>,
    pub session_id: String,
    /// Pre-populated transcript messages from the persisted session branch (for --resume).
    /// Empty for a brand-new session.
    pub history_messages: Vec<TranscriptMessage>,
}

/// Create the agent session without blocking on MCP discovery.
pub async fn bootstrap_agent_session(config: &TuiBootstrapConfig) -> Result<AgentBootstrap> {
    // Match SessionManager / `--continue` listing: always the resolved project dir.
    let cwd = config.paths.project_dir().clone();

    let (session, ui_rx) = create_coding_session_with_events(CreateSessionOptions {
        paths: &config.paths,
        settings: &config.settings,
        cwd: &cwd,
        resume_id: config.resume_id.as_deref(),
        provider_override: None,
        model_override: None,
        agent_mode: None,
        preloaded_resources: Some(config.preloaded_resources.clone()),
        defer_mcp_load: true,
    })
    .await?;

    let session = Arc::new(session);
    let session_id = session.session_id().to_string();
    let is_resume = config.resume_id.is_some();

    // Human-friendly title only for brand-new sessions. On resume/continue, keep the
    // stored name so we don't rewrite metadata (or scramble "latest" ordering).
    if !is_resume && let Ok(memorable_id) = memorable_ids::generate(memorable_ids::GenerateOptions::default()) {
        let _ = session.harness().set_session_name(&memorable_id).await;
    }

    // Load persisted chat history from the session branch (for --resume / --continue).
    let history_messages = load_chat_history(session.as_ref()).await;
    if is_resume && history_messages.is_empty() {
        log::warn!(
            "resumed session {session_id} has no reconstructable transcript entries (empty tree or missing snapshot)"
        );
    } else if is_resume {
        log::info!(
            "restored {} transcript message(s) for session {session_id}",
            history_messages.len()
        );
    }

    Ok(AgentBootstrap {
        session,
        ui_rx: Arc::new(Mutex::new(ui_rx)),
        session_id,
        history_messages,
    })
}

/// Load persisted chat history from the session's branch entries and convert them
/// to transcript messages for display on resume.
///
/// Prefer the latest `elph.transcript.snapshot` custom entry (exact live TUI state).
/// Fall back to reconstructing cards from LLM messages + tool results when no snapshot
/// exists (e.g. interrupted turn before snapshot was written).
async fn load_chat_history(session: &CodingAgentSession) -> Vec<TranscriptMessage> {
    let Ok(entries) = session.branch_entries().await else {
        return Vec::new();
    };

    if let Some(messages) = load_transcript_snapshot_from_entries(&entries) {
        return messages;
    }

    let cwd = session.harness().env().cwd().to_string();
    reconstruct_transcript_from_llm_entries(&entries, &cwd)
}

/// Latest full transcript snapshot written after each completed turn.
fn load_transcript_snapshot_from_entries(entries: &[elph_agent::SessionTreeEntry]) -> Option<Vec<TranscriptMessage>> {
    use crate::tui::transcript::{TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE, messages_from_snapshot_data};

    let mut latest: Option<&serde_json::Value> = None;
    for entry in entries {
        if let elph_agent::SessionTreeEntry::Custom { custom_type, data, .. } = entry
            && custom_type == TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE
            && let Some(data) = data
        {
            latest = Some(data);
        }
    }
    latest.and_then(messages_from_snapshot_data)
}

/// Reconstruct transcript cards from the LLM session tree (fallback path).
fn reconstruct_transcript_from_llm_entries(
    entries: &[elph_agent::SessionTreeEntry],
    cwd: &str,
) -> Vec<TranscriptMessage> {
    use elph_ai::{AssistantContentBlock, ContentBlock, Message, UserContent};
    use std::collections::HashMap;

    // --- first pass: index tool results by tool_call_id ---
    let mut tool_results: HashMap<String, ToolResultInfo> = HashMap::new();
    for entry in entries {
        let elph_agent::SessionTreeEntry::Message { message, .. } = entry else {
            continue;
        };
        let Some(llm) = message.as_llm() else {
            continue;
        };
        if let Message::ToolResult {
            tool_call_id,
            content,
            is_error,
            details,
            timestamp,
            ..
        } = llm
        {
            let output: String = content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            tool_results.insert(
                tool_call_id.clone(),
                ToolResultInfo {
                    output,
                    is_error: *is_error,
                    details: details.clone(),
                    timestamp_ms: *timestamp,
                },
            );
        }
    }

    // --- second pass: build transcript messages in order ---
    let mut messages: Vec<TranscriptMessage> = Vec::new();
    // Previous event wall time (ms since epoch) for duration reconstruction.
    let mut last_event_ms: Option<i64> = None;

    for entry in entries {
        let elph_agent::SessionTreeEntry::Message {
            message,
            timestamp,
            prompt_title,
            prompt_kind,
            ..
        } = entry
        else {
            continue;
        };
        let entry_ts = parse_iso_timestamp(timestamp);

        let Some(llm) = message.as_llm() else {
            continue;
        };
        match llm {
            Message::User {
                content,
                timestamp: user_ms,
            } => {
                // Prefer stored slash title (skill/template) over expanded invocation body.
                if let Some(msg) = prompt_card_from_session_meta(
                    prompt_title,
                    prompt_kind,
                    entry_ts.or_else(|| datetime_from_millis(*user_ms)),
                ) {
                    messages.push(msg);
                    last_event_ms = Some(*user_ms);
                    continue;
                }

                let text = match content {
                    UserContent::Text(t) => t.clone(),
                    UserContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                };
                if text.is_empty() {
                    continue;
                }

                let mut msg = TranscriptMessage::text(text, TranscriptStyle::User);
                msg.submitted_at = entry_ts.or_else(|| datetime_from_millis(*user_ms));
                msg.detail_expanded = false;
                messages.push(msg);
                last_event_ms = Some(*user_ms);
            }
            Message::Assistant(assistant) => {
                let assist_ms = assistant.timestamp;
                // Wall time for this LLM completion (generation after previous event).
                let generation_secs = last_event_ms.and_then(|prev| secs_between_ms(prev, assist_ms));

                let mut thinking_assigned_duration = false;
                let mut text_assigned_duration = false;
                let mut has_visible = false;

                for block in &assistant.content {
                    match block {
                        AssistantContentBlock::Thinking(t) => {
                            let thinking = t.thinking.trim();
                            if thinking.is_empty() {
                                continue;
                            }
                            has_visible = true;
                            let mut msg = TranscriptMessage::text(t.thinking.clone(), TranscriptStyle::Thinking);
                            // Attribute generation wall time to the first thinking block.
                            if !thinking_assigned_duration {
                                msg.duration_secs = generation_secs;
                                thinking_assigned_duration = true;
                            }
                            // Match live finalize_thinking default (collapsed when settled).
                            msg.detail_expanded = false;
                            messages.push(msg);
                        }
                        AssistantContentBlock::ToolCall(tc) => {
                            has_visible = true;
                            // Same raw JSON args as live ToolStart (not pretty-formatted).
                            let mut args_summary = serde_json::to_string(&tc.arguments).unwrap_or_default();
                            if tc.name == "shell_exec" {
                                args_summary = elph_agent::normalize_shell_exec_args(&args_summary, cwd);
                            }

                            let mut msg = TranscriptMessage::tool_call(
                                tc.name.clone(),
                                args_summary,
                                TranscriptStyle::ToolSuccess,
                            );

                            if let Some(result) = tool_results.get(&tc.id) {
                                if let Some(tool) = msg.tool.as_mut() {
                                    tool.output = result.output.clone();
                                    if let Some(details) = &result.details {
                                        let _ = tool.apply_tool_result_details(details);
                                        if let Some(secs) = crate::tui::transcript::duration_from_tool_details(details)
                                        {
                                            msg.duration_secs = Some(secs);
                                        }
                                    }
                                }
                                if result.is_error {
                                    msg.style = TranscriptStyle::ToolFailed;
                                }
                                // Fallback duration: tool_result wall clock − assistant message time.
                                if msg.duration_secs.is_none() {
                                    msg.duration_secs = secs_between_ms(assist_ms, result.timestamp_ms);
                                }
                                last_event_ms = Some(result.timestamp_ms.max(assist_ms));
                            }

                            msg.detail_expanded = msg.tool.as_ref().is_some_and(|t| t.has_inline_diff());
                            messages.push(msg);
                        }
                        AssistantContentBlock::Text(t) => {
                            let text = t.text.trim();
                            if text.is_empty() {
                                continue;
                            }
                            has_visible = true;

                            let mut msg = TranscriptMessage::text(t.text.clone(), TranscriptStyle::Assistant);
                            if !text_assigned_duration {
                                // Prefer remaining generation time when thinking already took it;
                                // if this assistant message is text-only, use full generation_secs.
                                if thinking_assigned_duration {
                                    msg.duration_secs = last_event_ms
                                        .and_then(|prev| secs_between_ms(prev, assist_ms))
                                        .or(generation_secs);
                                } else {
                                    msg.duration_secs = generation_secs;
                                }
                                text_assigned_duration = true;
                            }

                            let mut md = AssistantMarkdownBuffer::new();
                            md.mark_stream_complete();
                            md.refresh_stable(&msg.content, 100);
                            if let Some(part) = md.parts.first() {
                                let hash = part.source_hash;
                                let document = parse_markdown_on_worker(&msg.content);
                                md.apply_document(hash, document);
                            }
                            msg.markdown = Some(md);
                            msg.detail_expanded = true;
                            messages.push(msg);
                        }
                    }
                }

                if has_visible {
                    last_event_ms = Some(assist_ms.max(last_event_ms.unwrap_or(0)));
                }
            }
            Message::ToolResult { timestamp, .. } => {
                // Already merged into tool cards; advance clock for subsequent segments.
                last_event_ms = Some((*timestamp).max(last_event_ms.unwrap_or(0)));
            }
        }
    }

    messages
}

/// Indexed tool-result data used when matching `ToolCall` blocks to their output.
struct ToolResultInfo {
    output: String,
    is_error: bool,
    /// Structured tool details (e.g. edit_file old/new content for DiffView).
    details: Option<serde_json::Value>,
    timestamp_ms: i64,
}

fn datetime_from_millis(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms)
}

fn secs_between_ms(start_ms: i64, end_ms: i64) -> Option<f64> {
    let delta = end_ms.saturating_sub(start_ms);
    if delta <= 0 {
        return None;
    }
    Some(delta as f64 / 1000.0)
}

/// Build the transcript prompt card for a skill/template slash turn.
pub(crate) fn prompt_card_from_session_meta(
    prompt_title: &str,
    prompt_kind: &str,
    submitted_at: Option<DateTime<Utc>>,
) -> Option<TranscriptMessage> {
    if prompt_title.is_empty() {
        return None;
    }
    let style = if prompt_kind == "skill" {
        TranscriptStyle::SkillPrompt
    } else {
        TranscriptStyle::User
    };
    let mut msg = TranscriptMessage::text(prompt_title.to_string(), style);
    msg.submitted_at = submitted_at;
    msg.detail_expanded = false;
    Some(msg)
}

/// Parse an ISO 8601 / RFC 3339 timestamp string into `DateTime<Utc>`.
fn parse_iso_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    // Try RFC 3339 first, then basic ISO 8601.
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.fZ")
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        })
        .or_else(|| {
            // Fallback: try chrono's flexible ISO 8601 parser
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
        })
}

/// MCP bootstrap UI update (per-server progress or final transcript line).
#[derive(Debug, Clone)]
pub enum McpBootstrapUpdate {
    Server(McpServerLoadProgress),
    TranscriptLine(String),
}

/// Discover MCP servers and attach tools to a running session (after the TUI is visible).
///
/// Always attaches tools even if some servers fail — graceful degradation.
pub async fn bootstrap_mcp_for_session(
    session: &CodingAgentSession,
    _paths: &Paths,
    mut on_update: impl FnMut(McpBootstrapUpdate),
) -> Result<()> {
    let registry = session
        .mcp_registry()
        .ok_or_else(|| anyhow::anyhow!("MCP registry not available"))?;

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
    let registry_for_discovery = Arc::clone(&registry);
    let load = tokio::spawn(async move {
        registry_for_discovery
            .discover_tools_with_progress(Some(progress_tx))
            .await
    });

    while let Some(event) = progress_rx.recv().await {
        on_update(McpBootstrapUpdate::Server(event));
    }

    // Always attach tools even if discovery had errors — partial results are better than none.
    match load.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::warn!("MCP discovery partial failure (tools already attached): {e}"),
        Err(e) => log::warn!("MCP discovery task panicked: {e}"),
    }
    let report = registry.load_report();
    for line in format_mcp_load_footer(&report, &[]) {
        on_update(McpBootstrapUpdate::TranscriptLine(line));
    }
    // Attach whatever tools we have (even if discovery partially failed).
    session.attach_mcp_registry(registry).await?;
    Ok(())
}

/// Background bootstrap events delivered to the shell tick loop (non-blocking).
pub enum BootstrapUiEvent {
    AgentReady(AgentBootstrap),
    AgentFailed(String),
    McpHeader { enabled_servers: usize },
    McpServer(McpServerLoadProgress),
    McpTranscriptLine(String),
    McpComplete,
}

/// Run agent + MCP bootstrap off the UI thread; progress arrives on the returned channel.
pub fn spawn_bootstrap_worker(config: TuiBootstrapConfig, paths: Paths) -> UnboundedReceiver<BootstrapUiEvent> {
    let (tx, rx) = unbounded_channel();
    tokio::spawn(async move {
        run_bootstrap_worker(config, paths, tx).await;
    });
    rx
}

async fn run_bootstrap_worker(config: TuiBootstrapConfig, paths: Paths, tx: UnboundedSender<BootstrapUiEvent>) {
    let bootstrap = match bootstrap_agent_session(&config).await {
        Ok(bootstrap) => bootstrap,
        Err(err) => {
            let _ = tx.send(BootstrapUiEvent::AgentFailed(err.to_string()));
            return;
        }
    };

    let session = Arc::clone(&bootstrap.session);
    if tx.send(BootstrapUiEvent::AgentReady(bootstrap)).is_err() {
        return;
    }

    let enabled_servers = crate::platform::mcp::load_config_best_effort(&paths)
        .0
        .enabled_servers()
        .count();
    if tx.send(BootstrapUiEvent::McpHeader { enabled_servers }).is_err() {
        return;
    }

    let tx_progress = tx.clone();
    let mcp_result = bootstrap_mcp_for_session(session.as_ref(), &paths, move |update| {
        let event = match update {
            McpBootstrapUpdate::Server(progress) => BootstrapUiEvent::McpServer(progress),
            McpBootstrapUpdate::TranscriptLine(line) => BootstrapUiEvent::McpTranscriptLine(line),
        };
        let _ = tx_progress.send(event);
    })
    .await;

    // Always complete the bootstrap — tools were already attached even on partial failure.
    match mcp_result {
        Ok(()) => {
            let _ = tx.send(BootstrapUiEvent::McpComplete);
        }
        Err(err) => {
            log::warn!("MCP bootstrap partial failure (tools already attached): {err}");
            let _ = tx.send(BootstrapUiEvent::McpComplete);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_status_label_is_compact() {
        let label = mcp_server_status_label(&McpServerLoadProgress::Started {
            name: "context7".into(),
            index: 2,
            total: 5,
        });
        assert_eq!(label, "MCP 2/5 · context7");
    }

    #[test]
    fn format_mcp_load_summary_uses_consistent_separators() {
        let report = McpLoadReport {
            tools_loaded: 5,
            servers_ok: 2,
            servers_failed: 1,
            ..Default::default()
        };
        let summary = format_mcp_load_summary(&report);
        assert_eq!(summary, "MCP ready · 2 connected · 1 failed · 5 tools");

        let mut messages = Vec::new();
        begin_mcp_startup(&mut messages, 3);
        apply_mcp_startup_summary_line(&mut messages, &summary);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, summary);
        assert_eq!(messages[0].style, TranscriptStyle::StatusSuccess);
    }

    #[test]
    fn format_mcp_server_transcript_matches_expected_copy() {
        let started = format_mcp_server_transcript(&McpServerLoadProgress::Started {
            name: "code-review-graph".into(),
            index: 1,
            total: 3,
        });
        assert_eq!(started, "MCP server \"code-review-graph\" · connecting…");

        let ok = format_mcp_server_transcript(&McpServerLoadProgress::Finished {
            name: "deepwiki".into(),
            ok: true,
            transport: "http".into(),
            tool_count: 3,
            message: "discovered 3 tools".into(),
        });
        assert_eq!(ok, "MCP server \"deepwiki\" · 3 tools · http");

        let fail = format_mcp_server_transcript(&McpServerLoadProgress::Finished {
            name: "lightpanda".into(),
            ok: false,
            transport: "stdio".into(),
            tool_count: 0,
            message: "MCP error - Connection closed".into(),
        });
        assert_eq!(fail, "MCP server \"lightpanda\" · MCP error - Connection closed");
    }

    #[test]
    fn apply_mcp_server_progress_upserts_connecting_to_connected() {
        let mut messages = Vec::new();
        apply_mcp_server_progress(
            &mut messages,
            &McpServerLoadProgress::Started {
                name: "context7".into(),
                index: 1,
                total: 2,
            },
        );
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].style, TranscriptStyle::StatusRunning);
        assert_eq!(messages[0].startup_key.as_deref(), Some("startup:mcp:context7"));
        assert_eq!(messages[0].status_indent, STARTUP_MCP_INDENT_CELLS);
        assert!(!messages[0].content.starts_with(' '));

        apply_mcp_server_progress(
            &mut messages,
            &McpServerLoadProgress::Finished {
                name: "context7".into(),
                ok: true,
                transport: "http".into(),
                tool_count: 2,
                message: "discovered 2 tools".into(),
            },
        );
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].style, TranscriptStyle::StatusSuccess);
        assert_eq!(messages[0].content, "MCP server \"context7\" · 2 tools · http");
        assert_eq!(messages[0].status_indent, STARTUP_MCP_INDENT_CELLS);
    }

    #[test]
    fn phase_line_upserts_in_place() {
        let mut messages = initial_startup_messages(&LoadResourcesResult::default());
        assert_eq!(messages[0].content, "Preparing workspace…");
        begin_agent_startup(&mut messages);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Preparing agent…");
        mark_agent_startup_ready(&mut messages, Some("anthropic"), Some("claude-sonnet-4"));
        assert_eq!(messages[0].content, "Agent ready (active model: anthropic/claude-sonnet-4)");
        assert_eq!(messages[0].style, TranscriptStyle::StatusSuccess);

        mark_agent_startup_ready(&mut messages, None, None);
        assert_eq!(messages[0].content, "Agent ready (active model: none)");
        mark_agent_startup_ready(&mut messages, Some(""), Some("  "));
        assert_eq!(messages[0].content, "Agent ready (active model: none)");
    }

    #[test]
    fn format_agent_ready_line_handles_missing_model() {
        assert_eq!(
            format_agent_ready_line(Some("openai"), Some("gpt-5")),
            "Agent ready (active model: openai/gpt-5)"
        );
        assert_eq!(
            format_agent_ready_line(Some("openai"), None),
            "Agent ready (active model: none)"
        );
        assert_eq!(format_agent_ready_line(None, None), "Agent ready (active model: none)");
    }

    #[test]
    fn prompt_card_from_session_meta_skill_and_template() {
        let skill = prompt_card_from_session_meta("/tui-design layout", "skill", None).expect("skill");
        assert_eq!(skill.style, TranscriptStyle::SkillPrompt);
        assert_eq!(skill.content, "/tui-design layout");
        assert!(skill.style.is_user_input_card());

        let template = prompt_card_from_session_meta("/review-pr 42", "template", None).expect("template");
        assert_eq!(template.style, TranscriptStyle::User);
        assert_eq!(template.content, "/review-pr 42");

        assert!(prompt_card_from_session_meta("", "skill", None).is_none());
    }

    #[test]
    fn snapshot_preserves_skill_and_template_prompt_cards() {
        use crate::tui::transcript::{build_snapshot_data, messages_from_snapshot_data};

        let skill = prompt_card_from_session_meta("/code-review fix tests", "skill", None).unwrap();
        let template = prompt_card_from_session_meta("/summarize --short", "template", None).unwrap();
        let data = build_snapshot_data(&[skill, template]);
        let restored = messages_from_snapshot_data(&data).expect("parse");
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].style, TranscriptStyle::SkillPrompt);
        assert_eq!(restored[0].content, "/code-review fix tests");
        assert_eq!(restored[1].style, TranscriptStyle::User);
        assert_eq!(restored[1].content, "/summarize --short");
    }
}
