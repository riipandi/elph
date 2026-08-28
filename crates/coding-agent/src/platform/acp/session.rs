//! session/new, resume, list, close, delete.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use agent_client_protocol::schema::v2::{
    CloseSessionRequest, CloseSessionResponse, DeleteSessionRequest, DeleteSessionResponse, ListSessionsRequest,
    ListSessionsResponse, NewSessionRequest, NewSessionResponse, ReplayFrom, ResumeSessionRequest,
    ResumeSessionResponse, SessionId, SessionInfo, SessionInfoUpdate, SessionListCursor, SessionUpdate,
};
use agent_client_protocol::{Client, ConnectionTo};
use parking_lot::Mutex;

use crate::agent::{CreateSessionOptions, create_coding_session_with_events};
use crate::platform::acp::commands;
use crate::platform::acp::config;
use crate::platform::acp::mcp;
use crate::platform::acp::replay;
use crate::platform::acp::state::{AcpAgentState, AcpSessionState, MessageIds, lookup_session, session_key};
use crate::platform::acp::updates::send_update;

/// A non-absolute workspace root is a caller mistake: report `invalid_params`
/// (-32602), not `internal_error`.
pub fn require_absolute_cwd(cwd: &std::path::Path) -> Result<(), agent_client_protocol::Error> {
    if cwd.is_absolute() {
        return Ok(());
    }
    Err(agent_client_protocol::Error::invalid_params().data(serde_json::json!(format!(
        "cwd must be an absolute path, got `{}`",
        cwd.display()
    ))))
}

pub async fn create_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &NewSessionRequest,
    _connection: &ConnectionTo<Client>,
) -> anyhow::Result<NewSessionResponse> {
    let cwd = request.cwd.0.clone();
    let additional: Vec<PathBuf> = request.additional_directories.iter().map(|p| p.0.clone()).collect();

    let session_id = open_or_create(state, &cwd, additional, None).await?;
    let sid = SessionId::from(session_id.clone());
    let (session, _, _) = lookup_session(state, &session_id)?;
    let settings = state.lock().settings.clone();
    Ok(NewSessionResponse::new(sid).config_options(config::config_options_initial(&session, &settings).await))
}

/// Side effects after `session/new` has been answered (must not run on the I/O task).
pub async fn finish_create_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &NewSessionRequest,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
) {
    // Advertise slash commands *after* session/new is answered. Clients such as Zed
    // ignore `available_commands_update` for a session id they have not yet created.
    if let Err(error) = advertise_commands(state, connection, session_id).await {
        log::warn!("ACP available_commands after session/new: {error:#}");
    }
    attach_session_mcp(state, session_id, mcp::map_servers(&request.mcp_servers)).await;
    if let Err(error) = after_open(state, connection, session_id).await {
        log::warn!("ACP after session/new: {error:#}");
    }
    emit_full_config(state, connection, session_id).await;
}

async fn emit_full_config(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
) {
    let settings = state.lock().settings.clone();
    let Ok((session, _, _)) = lookup_session(state, session_id.0.as_ref()) else {
        return;
    };
    let options = config::config_options(&session, &settings).await;
    let _ = send_update(
        connection,
        session_id,
        SessionUpdate::ConfigOptionUpdate(agent_client_protocol::schema::v2::ConfigOptionUpdate::new(options)),
    );
}

async fn attach_session_mcp(
    state: &Arc<Mutex<AcpAgentState>>,
    session_id: &SessionId,
    client_mcp: Vec<(String, elph_agent::mcp::McpServerConfig)>,
) {
    let paths = state.lock().paths.clone();
    if let Ok((session, _, _)) = lookup_session(state, session_id.0.as_ref())
        && let Err(error) = mcp::attach_client_servers(&session, &paths, client_mcp).await
    {
        log::warn!("ACP mcpServers attach: {error:#}");
    }
}

pub async fn resume_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &ResumeSessionRequest,
    _connection: &ConnectionTo<Client>,
) -> anyhow::Result<ResumeSessionResponse> {
    let cwd = request.cwd.0.clone();
    let additional: Vec<PathBuf> = request.additional_directories.iter().map(|p| p.0.clone()).collect();
    let resume_id = request.session_id.0.to_string();
    let session_id = open_or_create(state, &cwd, additional, Some(&resume_id)).await?;
    let (session, _, _) = lookup_session(state, &session_id)?;
    let settings = state.lock().settings.clone();
    Ok(ResumeSessionResponse::new().config_options(config::config_options_initial(&session, &settings).await))
}

pub async fn finish_resume_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &ResumeSessionRequest,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
) {
    if matches!(request.replay_from, Some(ReplayFrom::Start(_)))
        && let Ok((session, _, _)) = lookup_session(state, session_id.0.as_ref())
        && let Err(error) = replay::replay_from_start(connection, session_id, &session).await
    {
        log::warn!("ACP resume replay: {error:#}");
    }
    if let Err(error) = advertise_commands(state, connection, session_id).await {
        log::warn!("ACP available_commands after session/resume: {error:#}");
    }
    attach_session_mcp(state, session_id, mcp::map_servers(&request.mcp_servers)).await;
    if let Err(error) = after_open(state, connection, session_id).await {
        log::warn!("ACP after session/resume: {error:#}");
    }
    emit_full_config(state, connection, session_id).await;
}

pub struct ListedSession {
    pub id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub updated_at: String,
}

pub async fn list_session_rows(
    state: &Arc<Mutex<AcpAgentState>>,
    cwd_filter: Option<PathBuf>,
    cursor: Option<&str>,
) -> anyhow::Result<(Vec<ListedSession>, Option<String>)> {
    let paths = state.lock().paths.clone();
    let cwd = cwd_filter
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    let manager = crate::agent::SessionManager::new(&paths, &cwd)?;
    let mut sessions = manager.list().await?;
    if let Some(filter) = cwd_filter {
        let filter = filter.to_string_lossy().to_string();
        sessions.retain(|s| s.cwd == filter);
    }
    let offset = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);
    let page_size = 50;
    let next = if offset + page_size < sessions.len() {
        Some((offset + page_size).to_string())
    } else {
        None
    };
    let page = sessions
        .into_iter()
        .skip(offset)
        .take(page_size)
        .map(|meta| ListedSession {
            id: meta.id,
            cwd: meta.cwd,
            title: meta.name,
            updated_at: meta.updated_at,
        })
        .collect();
    Ok((page, next))
}

pub async fn list_sessions(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &ListSessionsRequest,
) -> anyhow::Result<ListSessionsResponse> {
    let filter = request.cwd.as_ref().map(|p| p.0.clone());
    let cursor = request.cursor.as_ref().map(|c| c.0.as_ref().to_string());
    let (rows, next) = list_session_rows(state, filter, cursor.as_deref()).await?;
    let page = rows
        .into_iter()
        .map(|meta| {
            let extra = state
                .lock()
                .sessions
                .get(&meta.id)
                .map(|s| s.additional_directories.clone())
                .unwrap_or_default();
            let mut info = SessionInfo::new(meta.id, PathBuf::from(meta.cwd));
            if let Some(title) = meta.title {
                info = info.title(title);
            }
            if !meta.updated_at.is_empty() {
                info = info.updated_at(meta.updated_at);
            }
            if !extra.is_empty() {
                info = info.additional_directories(extra);
            }
            info
        })
        .collect();
    let mut response = ListSessionsResponse::new(page);
    if let Some(cursor) = next {
        response = response.next_cursor(SessionListCursor::new(cursor));
    }
    Ok(response)
}

pub async fn close_by_id(state: &Arc<Mutex<AcpAgentState>>, key: &str) -> anyhow::Result<()> {
    crate::platform::acp::state::mark_session_cancelled(state, key);
    if let Ok((session, _, _)) = lookup_session(state, key) {
        let _ = session.abort().await;
        let _ = session
            .session_manager()
            .release_session_lease(session.session_id())
            .await;
    }
    state.lock().sessions.remove(key);
    Ok(())
}

pub async fn close_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &CloseSessionRequest,
) -> anyhow::Result<CloseSessionResponse> {
    close_by_id(state, &session_key(&request.session_id)).await?;
    Ok(CloseSessionResponse::new())
}

pub async fn delete_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &DeleteSessionRequest,
) -> anyhow::Result<DeleteSessionResponse> {
    let key = session_key(&request.session_id);
    let _ = close_by_id(state, &key).await;
    let (paths, cwd) = {
        let guard = state.lock();
        (
            guard.paths.clone(),
            guard
                .sessions
                .values()
                .next()
                .map(|s| s.cwd.clone())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))),
        )
    };
    let manager = crate::agent::SessionManager::new(&paths, &cwd)?;
    let _ = manager.delete_by_id(&key).await;
    Ok(DeleteSessionResponse::new())
}

pub(super) async fn open_or_create(
    state: &Arc<Mutex<AcpAgentState>>,
    cwd: &PathBuf,
    additional: Vec<PathBuf>,
    resume_id: Option<&str>,
) -> anyhow::Result<String> {
    let (paths, mut settings) = {
        let guard = state.lock();
        (guard.paths.clone(), guard.settings.clone())
    };
    // GC on open can lock the store for a long time and the client will kill stdio
    // (`incoming_transport_closed` on `session/new`). Run retention from the TUI / cron instead.
    settings.session.gc_on_open = false;

    if let Some(id) = resume_id {
        let manager = crate::agent::SessionManager::new(&paths, cwd)?;
        if let Some(meta) = manager.find_metadata(id).await? {
            let stored = PathBuf::from(&meta.cwd);
            if stored != *cwd && !stored.as_os_str().is_empty() {
                anyhow::bail!("session cwd mismatch: stored {} vs request {}", stored.display(), cwd.display());
            }
        }
    }

    let created = tokio::time::timeout(
        std::time::Duration::from_secs(25),
        create_coding_session_with_events(CreateSessionOptions {
            paths: &paths,
            settings: &settings,
            cwd,
            resume_id,
            create_if_missing: false,
            session_name: None,
            provider_override: None,
            model_override: None,
            thinking_override: None,
            agent_mode: None,
            system_prompt_override: None,
            preloaded_resources: None,
            defer_mcp_load: true,
            defer_session_gc: false,
            defer_memory_warm: false,
            headless: true,
            extension_host: None,
        }),
    )
    .await
    .map_err(|_| anyhow::anyhow!("session open timed out after 25s"))?;
    let (session, ui_rx) = created?;

    let key = session.session_id().to_string();
    let session = Arc::new(session);
    session.start_worker_inbox_poller();

    state.lock().sessions.insert(
        key.clone(),
        AcpSessionState {
            session,
            ui_rx: Arc::new(tokio::sync::Mutex::new(ui_rx)),
            cwd: cwd.clone(),
            additional_directories: additional,
            running: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
            cancel_notify: Arc::new(tokio::sync::Notify::new()),
            stream_gate: Arc::new(tokio::sync::Mutex::new(())),
            ids: MessageIds::new(),
            open_tools: Arc::new(Mutex::new(std::collections::HashSet::new())),
            tool_outputs: Arc::new(Mutex::new(std::collections::HashMap::new())),
            terminal_sent: Arc::new(Mutex::new(std::collections::HashMap::new())),
            open_shells: Arc::new(Mutex::new(std::collections::HashSet::new())),
            idle_emitted: Arc::new(AtomicBool::new(false)),
        },
    );
    Ok(key)
}

async fn advertise_commands(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
) -> anyhow::Result<()> {
    let (session, _, _) = lookup_session(state, session_id.0.as_ref())?;
    commands::send_available_commands(connection, session_id, &session).await
}

async fn after_open(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
) -> anyhow::Result<()> {
    let (session, _, _) = lookup_session(state, session_id.0.as_ref())?;
    session.ensure_mcp_tools_ready().await;
    if let Err(error) = session.reconcile_tool_surface().await {
        log::warn!("ACP tool catalog refresh: {error:#}");
    }
    // Skills / MCP tools may have appeared; refresh the slash catalog.
    commands::send_available_commands(connection, session_id, &session).await?;
    if let Ok(title) = crate::agent::session_title_for_rename(Some(&session)) {
        send_update(
            connection,
            session_id,
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().title(title)),
        )?;
    }
    Ok(())
}
