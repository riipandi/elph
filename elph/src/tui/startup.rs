//! TUI startup bootstrap: staged agent session creation and deferred MCP discovery.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use elph_agent::{McpLoadReport, McpServerLoadProgress};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::agent::SkillConflict;
use crate::agent::mcp_bootstrap::{discover_mcp_registry_with_progress, wire_mcp_into_session};
use crate::agent::{AgentUiEvent, CodingAgentSession, CreateSessionOptions, LoadResourcesResult};
use crate::agent::{create_coding_session_with_events, format_skill_conflict_notice};
use crate::platform::{Paths, Settings};
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
pub fn initial_startup_messages(skill_conflicts: &[SkillConflict]) -> Vec<TranscriptMessage> {
    let mut messages = vec![TranscriptMessage::startup_status(
        STARTUP_KEY_PHASE,
        format!("Preparing workspace{STARTUP_ELLIPSIS}"),
        TranscriptStyle::StatusRunning,
    )];
    if let Some(notice) = format_skill_conflict_notice(skill_conflicts) {
        messages.push(TranscriptMessage::text(notice, TranscriptStyle::Meta));
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

pub fn mark_mcp_startup_failed(messages: &mut Vec<TranscriptMessage>, err: &str) {
    upsert_startup_line(
        messages,
        STARTUP_KEY_MCP_LOAD,
        format!("MCP failed{STARTUP_SEP}{err}"),
        TranscriptStyle::StatusFailed,
    );
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
}

/// Create the agent session without blocking on MCP discovery.
pub async fn bootstrap_agent_session(config: &TuiBootstrapConfig) -> Result<AgentBootstrap> {
    let cwd = std::env::current_dir().map_err(|e| anyhow::anyhow!("{e}"))?;

    let (session, ui_rx) = create_coding_session_with_events(CreateSessionOptions {
        paths: &config.paths,
        settings: &config.settings,
        cwd: &cwd,
        resume_id: config.resume_id.as_deref(),
        provider_override: None,
        model_override: None,
        preloaded_resources: Some(config.preloaded_resources.clone()),
        defer_mcp_load: true,
    })
    .await?;

    let session = Arc::new(session);
    let session_id = session.session_id().to_string();
    Ok(AgentBootstrap {
        session,
        ui_rx: Arc::new(Mutex::new(ui_rx)),
        session_id,
    })
}

/// MCP bootstrap UI update (per-server progress or final transcript line).
#[derive(Debug, Clone)]
pub enum McpBootstrapUpdate {
    Server(McpServerLoadProgress),
    TranscriptLine(String),
}

/// Discover MCP servers and attach tools to a running session (after the TUI is visible).
pub async fn bootstrap_mcp_for_session(
    session: &CodingAgentSession,
    paths: &Paths,
    mut on_update: impl FnMut(McpBootstrapUpdate),
) -> Result<()> {
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
    let paths = paths.clone();
    let load = tokio::spawn(async move { discover_mcp_registry_with_progress(&paths, Some(progress_tx)).await });

    while let Some(event) = progress_rx.recv().await {
        on_update(McpBootstrapUpdate::Server(event));
    }

    let (registry, config_warnings) = load.await.map_err(|e| anyhow::anyhow!("{e}"))?;
    for line in format_mcp_load_footer(&registry.load_report(), &config_warnings) {
        on_update(McpBootstrapUpdate::TranscriptLine(line));
    }
    wire_mcp_into_session(session, registry, config_warnings).await?;
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
    McpFailed(String),
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
    match bootstrap_mcp_for_session(session.as_ref(), &paths, move |update| {
        let event = match update {
            McpBootstrapUpdate::Server(progress) => BootstrapUiEvent::McpServer(progress),
            McpBootstrapUpdate::TranscriptLine(line) => BootstrapUiEvent::McpTranscriptLine(line),
        };
        let _ = tx_progress.send(event);
    })
    .await
    {
        Ok(()) => {
            let _ = tx.send(BootstrapUiEvent::McpComplete);
        }
        Err(err) => {
            let _ = tx.send(BootstrapUiEvent::McpFailed(err.to_string()));
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
        let mut messages = initial_startup_messages(&[]);
        assert_eq!(messages[0].content, "Preparing workspace…");
        begin_agent_startup(&mut messages);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Preparing agent…");
        mark_agent_startup_ready(&mut messages, Some("anthropic"), Some("claude-sonnet-4"));
        assert_eq!(
            messages[0].content,
            "Agent ready (active model: anthropic/claude-sonnet-4)"
        );
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
}
