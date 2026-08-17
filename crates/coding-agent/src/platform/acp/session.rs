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

pub async fn create_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &NewSessionRequest,
    connection: &ConnectionTo<Client>,
) -> anyhow::Result<NewSessionResponse> {
    let cwd = request.cwd.0.clone();
    if !cwd.is_absolute() {
        anyhow::bail!("cwd must be an absolute path");
    }
    let additional: Vec<PathBuf> = request.additional_directories.iter().map(|p| p.0.clone()).collect();
    let _ = mcp::map_servers(&request.mcp_servers);

    let session_id = open_or_create(state, &cwd, additional, None).await?;
    after_open(state, connection, &session_id).await?;
    let (session, _, _) = lookup_session(state, session_id.0.as_ref())?;
    Ok(NewSessionResponse::new(session_id).config_options(config::config_options(&session)))
}

pub async fn resume_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &ResumeSessionRequest,
    connection: &ConnectionTo<Client>,
) -> anyhow::Result<ResumeSessionResponse> {
    let cwd = request.cwd.0.clone();
    if !cwd.is_absolute() {
        anyhow::bail!("cwd must be an absolute path");
    }
    let additional: Vec<PathBuf> = request.additional_directories.iter().map(|p| p.0.clone()).collect();
    let _ = mcp::map_servers(&request.mcp_servers);
    let resume_id = request.session_id.0.to_string();
    let session_id = open_or_create(state, &cwd, additional, Some(&resume_id)).await?;

    if matches!(request.replay_from, Some(ReplayFrom::Start(_))) {
        let (session, _, _) = lookup_session(state, session_id.0.as_ref())?;
        replay::replay_from_start(connection, &session_id, &session).await?;
    }

    after_open(state, connection, &session_id).await?;
    let (session, _, _) = lookup_session(state, session_id.0.as_ref())?;
    Ok(ResumeSessionResponse::new().config_options(config::config_options(&session)))
}

pub async fn list_sessions(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &ListSessionsRequest,
) -> anyhow::Result<ListSessionsResponse> {
    let (paths, settings) = {
        let guard = state.lock();
        (guard.paths.clone(), guard.settings.clone())
    };
    let cwd = request
        .cwd
        .as_ref()
        .map(|p| p.0.clone())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    let manager = crate::agent::SessionManager::new(&paths, &cwd)?;
    let mut sessions = manager.list().await?;
    if let Some(filter) = request.cwd.as_ref() {
        let filter = filter.0.to_string_lossy().to_string();
        sessions.retain(|s| s.cwd == filter);
    }
    let offset = request
        .cursor
        .as_ref()
        .and_then(|c| c.0.parse::<usize>().ok())
        .unwrap_or(0);
    let page_size = 50;
    let next = if offset + page_size < sessions.len() {
        Some(SessionListCursor::new((offset + page_size).to_string()))
    } else {
        None
    };
    let page = sessions
        .into_iter()
        .skip(offset)
        .take(page_size)
        .map(|meta| {
            let mut info = SessionInfo::new(meta.id, PathBuf::from(meta.cwd));
            if let Some(title) = meta.name {
                info = info.title(title);
            }
            if !meta.updated_at.is_empty() {
                info = info.updated_at(meta.updated_at);
            }
            info
        })
        .collect();
    let mut response = ListSessionsResponse::new(page);
    if let Some(cursor) = next {
        response = response.next_cursor(cursor);
    }
    let _ = settings;
    Ok(response)
}

pub async fn close_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &CloseSessionRequest,
) -> anyhow::Result<CloseSessionResponse> {
    let key = session_key(&request.session_id);
    if let Ok((session, _, _)) = lookup_session(state, &key) {
        let _ = session.abort().await;
        let _ = session
            .session_manager()
            .release_session_lease(session.session_id())
            .await;
    }
    state.lock().sessions.remove(&key);
    Ok(CloseSessionResponse::new())
}

pub async fn delete_session(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &DeleteSessionRequest,
) -> anyhow::Result<DeleteSessionResponse> {
    let key = session_key(&request.session_id);
    let _ = close_session(state, &CloseSessionRequest::new(request.session_id.clone())).await;
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

async fn open_or_create(
    state: &Arc<Mutex<AcpAgentState>>,
    cwd: &PathBuf,
    additional: Vec<PathBuf>,
    resume_id: Option<&str>,
) -> anyhow::Result<SessionId> {
    let (paths, settings) = {
        let guard = state.lock();
        (guard.paths.clone(), guard.settings.clone())
    };

    let (session, ui_rx) = create_coding_session_with_events(CreateSessionOptions {
        paths: &paths,
        settings: &settings,
        cwd,
        resume_id,
        create_if_missing: false,
        session_name: None,
        provider_override: None,
        model_override: None,
        agent_mode: None,
        system_prompt_override: None,
        preloaded_resources: None,
        defer_mcp_load: false,
        headless: false,
    })
    .await?;

    let session_id = SessionId::from(session.session_id().to_string());
    let key = session.session_id().to_string();
    let session = Arc::new(session);
    session.start_worker_inbox_poller();

    state.lock().sessions.insert(
        key,
        AcpSessionState {
            session,
            ui_rx: Arc::new(tokio::sync::Mutex::new(ui_rx)),
            cwd: cwd.clone(),
            additional_directories: additional,
            running: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
            ids: MessageIds::new(),
        },
    );
    Ok(session_id)
}

async fn after_open(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
) -> anyhow::Result<()> {
    commands::send_available_commands(connection, session_id)?;
    let (session, _, _) = lookup_session(state, session_id.0.as_ref())?;
    if let Ok(title) = crate::agent::session_title_for_rename(Some(&session)) {
        send_update(
            connection,
            session_id,
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new().title(title)),
        )?;
    }
    Ok(())
}
