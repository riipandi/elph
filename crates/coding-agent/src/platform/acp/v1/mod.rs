//! ACP v1 (stable) stdio agent.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AgentAuthCapabilities, AgentCapabilities, AuthenticateRequest, AuthenticateResponse, AvailableCommand,
    AvailableCommandsUpdate, CancelNotification, CloseSessionRequest, ContentBlock, ContentChunk, CurrentModeUpdate,
    DeleteSessionRequest, Implementation, InitializeRequest, InitializeResponse, ListSessionsRequest,
    ListSessionsResponse, LoadSessionRequest, LoadSessionResponse, LogoutCapabilities, LogoutRequest, LogoutResponse,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind, Plan, PlanEntry, PlanEntryPriority,
    PlanEntryStatus, PromptCapabilities, PromptRequest, PromptResponse, RequestPermissionOutcome,
    RequestPermissionRequest, ResumeSessionRequest, ResumeSessionResponse, SessionAdditionalDirectoriesCapabilities,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption, SessionDeleteCapabilities, SessionId,
    SessionInfo, SessionListCapabilities, SessionMode, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse,
    StopReason, TextContent, ToolCall, ToolCallStatus, ToolCallUpdate, ToolKind,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Result as AcpResult};
use parking_lot::Mutex;

use crate::agent::AgentUiEvent;
use crate::platform::acp::config::{ConfigCategory, apply_config_value, parse_thought, session_config};
use crate::platform::acp::mcp;
use crate::platform::acp::session::{close_by_id, list_session_rows, open_or_create};
use crate::platform::acp::state::{AcpAgentState, lookup_session};
use crate::platform::{Paths, Settings};

pub async fn run_with<T>(paths: Paths, settings: Settings, transport: T) -> AcpResult<()>
where
    T: agent_client_protocol::ConnectTo<Agent> + 'static,
{
    let state = Arc::new(Mutex::new(AcpAgentState {
        sessions: HashMap::new(),
        paths,
        settings,
        client_fs_read: false,
        client_elicitation_form: false,
        auth: crate::platform::acp::state::ConnectionAuth::Anonymous,
    }));

    Agent
        .builder()
        .name("elph")
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |initialize: InitializeRequest, responder, _connection| {
                    state.lock().client_fs_read = initialize.client_capabilities.fs.read_text_file;
                    let _ = responder.respond(
                        InitializeResponse::new(ProtocolVersion::V1)
                            .agent_capabilities(v1_capabilities())
                            .auth_methods(crate::platform::acp::auth::v1_auth_methods())
                            .agent_info(Implementation::new("elph", env!("CARGO_PKG_VERSION")).title("Elph")),
                    );
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: AuthenticateRequest, responder, _connection| {
                    match crate::platform::acp::auth::login(&state, request.method_id.0.as_ref()) {
                        Ok(()) => {
                            let _ = responder.respond(AuthenticateResponse::new());
                        }
                        Err(error) => {
                            let _ = responder.respond_with_error(error);
                        }
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |_request: LogoutRequest, responder, connection| {
                    let state = Arc::clone(&state);
                    if let Err(error) = connection.spawn(async move {
                        crate::platform::acp::auth::logout(&state).await;
                        let _ = responder.respond(LogoutResponse::new());
                        Ok(())
                    }) {
                        log::warn!("ACP v1 logout spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: NewSessionRequest, responder, connection| {
                    if let Err(error) = crate::platform::acp::auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    if !request.cwd.is_absolute() {
                        let _ = responder.respond_with_error(agent_client_protocol::util::internal_error(
                            "cwd must be an absolute path",
                        ));
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        let opened = tokio::spawn({
                            let state = Arc::clone(&state);
                            let cwd = request.cwd.clone();
                            let extra = request.additional_directories.clone();
                            async move { open_or_create(&state, &cwd, extra, None).await }
                        })
                        .await;
                        match opened {
                            Ok(Ok(id)) => {
                                let (modes, options) = v1_config_extras(&state, &id).await;
                                let _ = responder.respond(
                                    NewSessionResponse::new(SessionId::from(id.clone()))
                                        .modes(modes)
                                        .config_options(options),
                                );
                                v1_after_open(&state, &conn, &id, mcp::map_v1_servers(&request.mcp_servers)).await;
                            }
                            Ok(Err(error)) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                            Err(join) => {
                                log::error!("ACP v1 session/new panicked: {join}");
                                let _ = responder.respond_with_error(agent_client_protocol::util::internal_error(
                                    format!("session/new panicked: {join}"),
                                ));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 session/new spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: LoadSessionRequest, responder, connection| {
                    if let Err(error) = crate::platform::acp::auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        match open_or_create(
                            &state,
                            &request.cwd,
                            request.additional_directories.clone(),
                            Some(request.session_id.0.as_ref()),
                        )
                        .await
                        {
                            Ok(id) => {
                                let (modes, options) = v1_config_extras(&state, &id).await;
                                let _ =
                                    responder.respond(LoadSessionResponse::new().modes(modes).config_options(options));
                                if let Ok((session, _, _)) = lookup_session(&state, &id) {
                                    let _ = replay_v1(&conn, &id, &session).await;
                                }
                                v1_after_open(&state, &conn, &id, mcp::map_v1_servers(&request.mcp_servers)).await;
                            }
                            Err(error) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 session/load spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: ResumeSessionRequest, responder, connection| {
                    if let Err(error) = crate::platform::acp::auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        match open_or_create(
                            &state,
                            &request.cwd,
                            request.additional_directories.clone(),
                            Some(request.session_id.0.as_ref()),
                        )
                        .await
                        {
                            Ok(id) => {
                                let (modes, options) = v1_config_extras(&state, &id).await;
                                let _ = responder
                                    .respond(ResumeSessionResponse::new().modes(modes).config_options(options));
                                v1_after_open(&state, &conn, &id, mcp::map_v1_servers(&request.mcp_servers)).await;
                            }
                            Err(error) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 session/resume spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: ListSessionsRequest, responder, connection| {
                    let state = Arc::clone(&state);
                    if let Err(error) = connection.spawn(async move {
                        let filter = request.cwd.clone();
                        let cursor = request.cursor.clone();
                        match list_session_rows(&state, filter, cursor.as_deref()).await {
                            Ok((rows, _)) => {
                                let sessions = rows
                                    .into_iter()
                                    .map(|row| {
                                        let mut info = SessionInfo::new(row.id, PathBuf::from(row.cwd));
                                        if let Some(title) = row.title {
                                            info = info.title(title);
                                        }
                                        info
                                    })
                                    .collect();
                                let _ = responder.respond(ListSessionsResponse::new(sessions));
                            }
                            Err(error) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 session/list spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: CloseSessionRequest, responder, connection| {
                    let state = Arc::clone(&state);
                    if let Err(error) = connection.spawn(async move {
                        match close_by_id(&state, request.session_id.0.as_ref()).await {
                            Ok(()) => {
                                let _ =
                                    responder.respond(agent_client_protocol::schema::v1::CloseSessionResponse::new());
                            }
                            Err(error) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 session/close spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: DeleteSessionRequest, responder, connection| {
                    let state = Arc::clone(&state);
                    if let Err(error) = connection.spawn(async move {
                        let key = request.session_id.0.as_ref().to_string();
                        let _ = close_by_id(&state, &key).await;
                        let paths = state.lock().paths.clone();
                        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
                        if let Ok(manager) = crate::agent::SessionManager::new(&paths, &cwd) {
                            let _ = manager.delete_by_id(&key).await;
                        }
                        let _ = responder.respond(agent_client_protocol::schema::v1::DeleteSessionResponse::new());
                        Ok(())
                    }) {
                        log::warn!("ACP v1 session/delete spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: SetSessionModeRequest, responder, connection| {
                    if let Err(error) = crate::platform::acp::auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        match set_mode(&state, &conn, &request).await {
                            Ok(response) => {
                                let _ = responder.respond(response);
                            }
                            Err(error) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 set_mode spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: SetSessionConfigOptionRequest, responder, connection| {
                    if let Err(error) = crate::platform::acp::auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        match set_config_v1(&state, &conn, &request).await {
                            Ok(response) => {
                                let _ = responder.respond(response);
                            }
                            Err(error) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 set_config spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: PromptRequest, responder, connection| {
                    if let Err(error) = crate::platform::acp::auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        match run_prompt(&state, &conn, &request).await {
                            Ok(reason) => {
                                let _ = responder.respond(PromptResponse::new(reason));
                            }
                            Err(error) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 session/prompt spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let state = Arc::clone(&state);
                async move |notification: CancelNotification, connection| {
                    let key = notification.session_id.0.as_ref().to_string();
                    crate::platform::acp::state::mark_session_cancelled(&state, &key);
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        if let Ok((session, _, _)) = lookup_session(&state, &key) {
                            let _ = session.abort().await;
                            let _ = cancel_v1_open_tools(&state, &conn, &key);
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP v1 cancel spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(transport)
        .await
}

async fn attach_v1_mcp(
    state: &Arc<Mutex<AcpAgentState>>,
    session_id: &str,
    servers: Vec<(String, elph_agent::mcp::McpServerConfig)>,
) {
    if servers.is_empty() {
        return;
    }
    let paths = state.lock().paths.clone();
    if let Ok((session, _, _)) = lookup_session(state, session_id)
        && let Err(error) = mcp::attach_client_servers(&session, &paths, servers).await
    {
        log::warn!("ACP v1 mcpServers attach: {error:#}");
    }
}

fn cancel_v1_open_tools(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &str,
) -> anyhow::Result<()> {
    let ids = crate::platform::acp::tools::take_open_tools(state, session_id);
    for id in ids {
        notify(
            connection,
            session_id,
            SessionUpdate::ToolCallUpdate(v1_tool_update(id, ToolCallStatus::Failed, "cancelled".into())),
        )?;
    }
    Ok(())
}

async fn v1_config_extras(
    state: &Arc<Mutex<AcpAgentState>>,
    session_id: &str,
) -> (SessionModeState, Vec<SessionConfigOption>) {
    let settings = state.lock().settings.clone();
    if let Ok((session, _, _)) = lookup_session(state, session_id) {
        let snapshot = session_config(&session, &settings, false).await;
        let modes = v1_thought_modes(&snapshot);
        let options = snapshot.into_iter().map(to_v1_option).collect();
        return (modes, options);
    }
    (v1_thought_modes_fallback(), Vec::new())
}

async fn v1_after_open(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &str,
    servers: Vec<(String, elph_agent::mcp::McpServerConfig)>,
) {
    if let Ok((session, _, _)) = lookup_session(state, session_id)
        && let Err(error) = send_v1_commands(connection, session_id, &session).await
    {
        log::warn!("ACP v1 available commands: {error:#}");
    }
    attach_v1_mcp(state, session_id, servers).await;
    if let Ok((session, _, _)) = lookup_session(state, session_id) {
        session.ensure_mcp_tools_ready().await;
        if let Err(error) = session.reconcile_tool_surface().await {
            log::warn!("ACP v1 tool catalog refresh: {error:#}");
        }
        if let Err(error) = send_v1_commands(connection, session_id, &session).await {
            log::warn!("ACP v1 available commands: {error:#}");
        }
        let settings = state.lock().settings.clone();
        let options = session_config(&session, &settings, true)
            .await
            .into_iter()
            .map(to_v1_option)
            .collect();
        let _ = notify(
            connection,
            session_id,
            SessionUpdate::ConfigOptionUpdate(agent_client_protocol::schema::v1::ConfigOptionUpdate::new(options)),
        );
    }
}

fn v1_thought_modes(snapshot: &[crate::platform::acp::config::ConfigSelect]) -> SessionModeState {
    snapshot
        .iter()
        .find(|s| s.id == "thought_level")
        .map(|s| {
            SessionModeState::new(
                s.current.clone(),
                s.options
                    .iter()
                    .map(|c| SessionMode::new(c.id.clone(), c.name.clone()))
                    .collect(),
            )
        })
        .unwrap_or_else(v1_thought_modes_fallback)
}

fn v1_thought_modes_fallback() -> SessionModeState {
    SessionModeState::new(
        "off",
        vec![
            SessionMode::new("off", "Thinking: off"),
            SessionMode::new("minimal", "Thinking: minimal"),
            SessionMode::new("low", "Thinking: low"),
            SessionMode::new("medium", "Thinking: medium"),
            SessionMode::new("high", "Thinking: high"),
            SessionMode::new("xhigh", "Thinking: xhigh"),
            SessionMode::new("max", "Thinking: max"),
        ],
    )
}

fn to_v1_option(select: crate::platform::acp::config::ConfigSelect) -> SessionConfigOption {
    let options: Vec<SessionConfigSelectOption> = select
        .options
        .into_iter()
        .map(|c| SessionConfigSelectOption::new(c.id, c.name))
        .collect();
    SessionConfigOption::select(select.id, select.name, select.current, options)
        .category(match select.category {
            ConfigCategory::Mode => SessionConfigOptionCategory::Mode,
            ConfigCategory::Model => SessionConfigOptionCategory::Model,
            ConfigCategory::ThoughtLevel => SessionConfigOptionCategory::ThoughtLevel,
        })
        .description(select.description)
}

fn v1_capabilities() -> AgentCapabilities {
    AgentCapabilities::new()
        .auth(AgentAuthCapabilities::new().logout(LogoutCapabilities::new()))
        .load_session(true)
        .prompt_capabilities(PromptCapabilities::new().embedded_context(true).image(true))
        .mcp_capabilities(
            agent_client_protocol::schema::v1::McpCapabilities::new()
                .http(true)
                .sse(true),
        )
        .session_capabilities(
            agent_client_protocol::schema::v1::SessionCapabilities::new()
                .list(SessionListCapabilities::new())
                .resume(agent_client_protocol::schema::v1::SessionResumeCapabilities::new())
                .close(agent_client_protocol::schema::v1::SessionCloseCapabilities::new())
                .delete(SessionDeleteCapabilities::new())
                .additional_directories(SessionAdditionalDirectoriesCapabilities::new()),
        )
}

async fn send_v1_commands(
    connection: &ConnectionTo<Client>,
    session_id: &str,
    session: &crate::agent::CodingAgentSession,
) -> anyhow::Result<()> {
    let commands = crate::platform::acp::commands::slash_catalog(session)
        .await
        .into_iter()
        .map(|c| {
            let mut cmd = AvailableCommand::new(c.name, c.description);
            if let Some(hint) = c.hint {
                cmd = cmd.input(agent_client_protocol::schema::v1::AvailableCommandInput::Unstructured(
                    agent_client_protocol::schema::v1::UnstructuredCommandInput::new(hint),
                ));
            }
            cmd
        })
        .collect();
    notify(
        connection,
        session_id,
        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands)),
    )
}

fn notify(connection: &ConnectionTo<Client>, session_id: &str, update: SessionUpdate) -> anyhow::Result<()> {
    connection
        .send_notification(SessionNotification::new(SessionId::from(session_id.to_string()), update))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn extract_text(blocks: &[ContentBlock]) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(t) => parts.push(t.text.clone()),
            ContentBlock::ResourceLink(link) => parts.push(format!("[resource {}]({})", link.name, link.uri)),
            ContentBlock::Audio(_) => anyhow::bail!("audio content is not supported"),
            _ => {}
        }
    }
    Ok(parts.join("\n"))
}

fn extra_roots_v1(state: &Arc<Mutex<AcpAgentState>>, key: &str) -> String {
    let dirs = state
        .lock()
        .sessions
        .get(key)
        .map(|s| s.additional_directories.clone())
        .unwrap_or_default();
    if dirs.is_empty() {
        return String::new();
    }
    let mut lines = vec!["Additional workspace directories:".to_string()];
    for dir in dirs {
        lines.push(format!("- {}", dir.display()));
    }
    lines.join("\n")
}

async fn hydrate_v1_files(connection: &ConnectionTo<Client>, session_id: &str, text: String) -> String {
    let mut out = text.clone();
    for token in text.split_whitespace() {
        let Some(raw) = token.strip_prefix("(file://").or_else(|| token.strip_prefix("file://")) else {
            continue;
        };
        let path = raw.trim_end_matches(')');
        let request = agent_client_protocol::schema::v1::ReadTextFileRequest::new(
            SessionId::from(session_id.to_string()),
            std::path::PathBuf::from(path),
        );
        if let Ok(response) = connection.send_request(request).block_task().await {
            let excerpt: String = response.content.chars().take(8_000).collect();
            out.push_str(&format!("\n\n<resource uri=\"file://{path}\">\n{excerpt}\n</resource>"));
        } else if let Ok(body) = std::fs::read_to_string(path) {
            let excerpt: String = body.chars().take(8_000).collect();
            out.push_str(&format!("\n\n<resource uri=\"file://{path}\">\n{excerpt}\n</resource>"));
        }
    }
    out
}

async fn run_prompt(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    request: &PromptRequest,
) -> anyhow::Result<StopReason> {
    let key = request.session_id.0.as_ref().to_string();
    let (session, ui_rx, _) = lookup_session(state, &key)?;
    let mut text = match extract_text(&request.prompt) {
        Ok(text) => text,
        Err(error) => {
            notify(
                connection,
                &key,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(format!(
                    "Could not read prompt: {error:#}"
                ))))),
            )?;
            return Ok(StopReason::EndTurn);
        }
    };
    if state.lock().client_fs_read {
        text = hydrate_v1_files(connection, &key, text).await;
    }
    let extra = extra_roots_v1(state, &key);
    if !extra.is_empty() {
        text = format!("{extra}\n\n{text}");
    }
    let trimmed = text.trim();
    if crate::platform::acp::commands::is_slash(trimmed) {
        return run_slash_v1(state, connection, &key, trimmed).await;
    }
    submit_and_stream_v1(state, connection, &key, session, text, &ui_rx).await
}

async fn submit_and_stream_v1(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    key: &str,
    session: std::sync::Arc<crate::agent::CodingAgentSession>,
    text: String,
    ui_rx: &Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<StopReason> {
    let steer = crate::platform::acp::updates::is_running(
        state,
        &agent_client_protocol::schema::v2::SessionId::from(key.to_string()),
    );
    race_v1(
        state,
        connection,
        key,
        async move { session.submit_prompt(text, steer).await },
        ui_rx,
    )
    .await
}

async fn race_v1<F>(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    key: &str,
    submit: F,
    ui_rx: &Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<StopReason>
where
    F: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let cancel = crate::platform::acp::state::session_cancel_notify(state, key);
    let gate = crate::platform::acp::state::session_stream_gate(state, key);
    let mut submit = tokio::spawn(submit);
    let Some(gate) = gate else {
        return v1_finish_submit_only(connection, key, submit).await;
    };
    let Ok(_permit) = gate.try_lock() else {
        return v1_finish_submit_only(connection, key, submit).await;
    };

    let mut submit_done = false;
    let mut submit_err = None;
    let mut pending = std::collections::VecDeque::new();

    loop {
        if state
            .lock()
            .sessions
            .get(key)
            .is_some_and(|s| s.cancelled.load(Ordering::Relaxed))
        {
            return Ok(StopReason::Cancelled);
        }
        let next = if let Some(event) = pending.pop_front() {
            Some(event)
        } else {
            let mut rx = ui_rx.lock().await;
            crate::platform::acp::updates::drain_stale(&mut rx, &mut pending);
            if let Some(event) = pending.pop_front() {
                Some(event)
            } else {
                tokio::select! {
                    biased;
                    event = rx.recv() => event,
                    result = &mut submit, if !submit_done => {
                        submit_done = true;
                        match result {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                let _ = notify(
                                    connection,
                                    key,
                                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                                        TextContent::new(format!("Prompt failed: {error:#}")),
                                    ))),
                                );
                                submit_err = Some(error);
                            }
                            Err(error) => submit_err = Some(anyhow::anyhow!("{error}")),
                        }
                        None
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(400)), if submit_done => {
                        break;
                    }
                    _ = async {
                        if let Some(cancel) = &cancel {
                            cancel.notified().await;
                        } else {
                            std::future::pending::<()>().await;
                        }
                    } => {
                        return Ok(StopReason::Cancelled);
                    }
                }
            }
        };
        if let Some(event) = next
            && let Err(error) = apply_v1_event(state, connection, key, event, cancel.clone()).await
        {
            log::warn!("ACP v1 session update: {error:#}");
        }
    }

    if let Some(error) = submit_err {
        let _ = notify(
            connection,
            key,
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(format!(
                "Error: {error:#}"
            ))))),
        );
        return Ok(StopReason::EndTurn);
    }
    Ok(StopReason::EndTurn)
}

async fn v1_finish_submit_only(
    connection: &ConnectionTo<Client>,
    key: &str,
    submit: tokio::task::JoinHandle<anyhow::Result<()>>,
) -> anyhow::Result<StopReason> {
    let message = match submit.await {
        Ok(Ok(())) => "Turn finished, but output could not be streamed.".to_string(),
        Ok(Err(error)) => format!("Error: {error:#}"),
        Err(join) => format!("Turn panicked: {join}"),
    };
    let _ = notify(
        connection,
        key,
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(message)))),
    );
    Ok(StopReason::EndTurn)
}

async fn run_slash_v1(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    key: &str,
    input: &str,
) -> anyhow::Result<StopReason> {
    let outcome = match crate::platform::acp::commands::resolve_slash(state, key, input).await {
        Ok(outcome) => outcome,
        Err(error) => {
            notify(
                connection,
                key,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(format!(
                    "Slash command failed: {error:#}"
                ))))),
            )?;
            return Ok(StopReason::EndTurn);
        }
    };
    match outcome {
        crate::platform::acp::commands::SlashOutcome::Text(text) => {
            notify(
                connection,
                key,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text)))),
            )?;
            Ok(StopReason::EndTurn)
        }
        crate::platform::acp::commands::SlashOutcome::Continue => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            submit_and_stream_v1(
                state,
                connection,
                key,
                session,
                crate::agent::RETRY_CONTINUE_PROMPT.to_string(),
                &ui_rx,
            )
            .await
        }
        crate::platform::acp::commands::SlashOutcome::SubmitPrompt => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            submit_and_stream_v1(state, connection, key, session, input.to_string(), &ui_rx).await
        }
        crate::platform::acp::commands::SlashOutcome::Skill { name, args } => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            match race_v1(
                state,
                connection,
                key,
                {
                    let name = name.clone();
                    async move { session.invoke_skill(&name, &args).await }
                },
                &ui_rx,
            )
            .await
            {
                Ok(reason) => Ok(reason),
                Err(error) => {
                    notify(
                        connection,
                        key,
                        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
                            format!("Skill `/{name}` failed: {error:#}"),
                        )))),
                    )?;
                    Ok(StopReason::EndTurn)
                }
            }
        }
        crate::platform::acp::commands::SlashOutcome::PromptTemplate { name, args } => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            match race_v1(
                state,
                connection,
                key,
                {
                    let name = name.clone();
                    async move { session.prompt_from_template(&name, &args).await }
                },
                &ui_rx,
            )
            .await
            {
                Ok(reason) => Ok(reason),
                Err(error) => {
                    notify(
                        connection,
                        key,
                        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
                            format!("Template `/{name}` failed: {error:#}"),
                        )))),
                    )?;
                    Ok(StopReason::EndTurn)
                }
            }
        }
        crate::platform::acp::commands::SlashOutcome::Reloaded(text) => {
            if let Ok((session, _, _)) = lookup_session(state, key) {
                let _ = send_v1_commands(connection, key, &session).await;
            }
            notify(
                connection,
                key,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text)))),
            )?;
            Ok(StopReason::EndTurn)
        }
    }
}

fn v1_tool_update(id: String, status: ToolCallStatus, output: String) -> ToolCallUpdate {
    let output = crate::platform::acp::limits::truncate_text(&output);
    let mut fields = agent_client_protocol::schema::v1::ToolCallUpdateFields::new();
    fields.status = Some(status);
    fields.content = Some(vec![agent_client_protocol::schema::v1::ToolCallContent::from(
        ContentBlock::Text(TextContent::new(output)),
    )]);
    ToolCallUpdate::new(id, fields)
}

fn map_kind(name: &str) -> ToolKind {
    match crate::platform::acp::tools::kind_for_tool(name) {
        agent_client_protocol::schema::v2::ToolKind::Read => ToolKind::Read,
        agent_client_protocol::schema::v2::ToolKind::Edit => ToolKind::Edit,
        agent_client_protocol::schema::v2::ToolKind::Delete => ToolKind::Delete,
        agent_client_protocol::schema::v2::ToolKind::Move => ToolKind::Move,
        agent_client_protocol::schema::v2::ToolKind::Search => ToolKind::Search,
        agent_client_protocol::schema::v2::ToolKind::Execute => ToolKind::Execute,
        agent_client_protocol::schema::v2::ToolKind::Think => ToolKind::Think,
        agent_client_protocol::schema::v2::ToolKind::Fetch => ToolKind::Fetch,
        _ => ToolKind::Other,
    }
}

async fn apply_v1_event(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    key: &str,
    event: AgentUiEvent,
    cancel: Option<Arc<tokio::sync::Notify>>,
) -> anyhow::Result<()> {
    match event {
        AgentUiEvent::TextDelta(text) if !text.is_empty() => {
            notify(
                connection,
                key,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
                    crate::platform::acp::limits::truncate_text(&text),
                )))),
            )?;
        }
        AgentUiEvent::ThinkingDelta(text) if !text.is_empty() => {
            notify(
                connection,
                key,
                SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
                    crate::platform::acp::limits::truncate_text(&text),
                )))),
            )?;
        }
        AgentUiEvent::Retrying { attempt } => {
            notify(
                connection,
                key,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(format!(
                    "Provider is rate-limited or busy — retrying (attempt {attempt})…"
                ))))),
            )?;
        }
        AgentUiEvent::Status(line) if !line.is_empty() => {
            notify(
                connection,
                key,
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
                    crate::platform::acp::updates::acp_status_text(&line),
                )))),
            )?;
        }
        AgentUiEvent::Status(_) => {}
        AgentUiEvent::ToolStart {
            id, name, args_summary, ..
        } => {
            let sid = agent_client_protocol::schema::v2::SessionId::from(key.to_string());
            let already = crate::platform::acp::tools::is_open_tool(state, &sid, &id);
            if !already {
                let call = ToolCall::new(id.clone(), name.clone())
                    .kind(map_kind(&name))
                    .status(ToolCallStatus::Pending)
                    .raw_input(serde_json::json!({ "summary": args_summary }));
                notify(connection, key, SessionUpdate::ToolCall(call))?;
            }
            crate::platform::acp::tools::track_tool_start(state, &sid, &id, &name);
            let mut fields = agent_client_protocol::schema::v1::ToolCallUpdateFields::new();
            fields.status = Some(ToolCallStatus::InProgress);
            notify(connection, key, SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id, fields)))?;
        }
        AgentUiEvent::ToolUpdate { id, output } => {
            let sid = agent_client_protocol::schema::v2::SessionId::from(key.to_string());
            if !crate::platform::acp::tools::is_open_tool(state, &sid, &id) {
                notify(
                    connection,
                    key,
                    SessionUpdate::ToolCall(ToolCall::new(id.clone(), "tool").status(ToolCallStatus::InProgress)),
                )?;
                crate::platform::acp::tools::track_tool_start(state, &sid, &id, "tool");
            }
            notify(
                connection,
                key,
                SessionUpdate::ToolCallUpdate(v1_tool_update(id, ToolCallStatus::InProgress, output)),
            )?;
        }
        AgentUiEvent::ToolEnd {
            id, is_error, output, ..
        } => {
            crate::platform::acp::tools::track_tool_end(
                state,
                &agent_client_protocol::schema::v2::SessionId::from(key.to_string()),
                &id,
            );
            let status = if is_error {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            };
            notify(
                connection,
                key,
                SessionUpdate::ToolCallUpdate(v1_tool_update(id, status, output)),
            )?;
        }
        AgentUiEvent::TodoUpdated { items } => {
            let entries = items
                .iter()
                .map(|item| {
                    PlanEntry::new(
                        item.content.clone(),
                        PlanEntryPriority::Medium,
                        match item.status {
                            elph_agent::TodoStatus::Completed => PlanEntryStatus::Completed,
                            elph_agent::TodoStatus::InProgress => PlanEntryStatus::InProgress,
                            _ => PlanEntryStatus::Pending,
                        },
                    )
                })
                .collect();
            notify(connection, key, SessionUpdate::Plan(Plan::new(entries)))?;
        }
        AgentUiEvent::RunCompleted { .. } => {}
        AgentUiEvent::ToolApprovalRequired(req) => {
            crate::platform::acp::tools::track_tool_start(
                state,
                &agent_client_protocol::schema::v2::SessionId::from(key.to_string()),
                &req.tool_call_id,
                &req.tool_name,
            );
            let call = ToolCall::new(req.tool_call_id.clone(), req.tool_name.clone())
                .kind(map_kind(&req.tool_name))
                .status(ToolCallStatus::Pending)
                .raw_input(serde_json::json!({ "summary": req.args_summary }));
            notify(connection, key, SessionUpdate::ToolCall(call))?;
            let choice = request_v1_tool_approval(connection, key, &req, cancel).await;
            let _ = req.response_tx.send(choice);
        }
        AgentUiEvent::UserQuestionRequired(req) => {
            ask_user_v1(connection, key, req, cancel).await?;
        }
        AgentUiEvent::ModeChangeRequired(req) => {
            let approved = request_v1_mode_change(connection, key, &req, cancel).await;
            if approved && let Ok((session, _, _)) = lookup_session(state, key) {
                let mode = crate::agent::agent_mode_from_setting(&req.target_mode);
                session.invalidate_system_prompt_cache();
                session.try_set_mode_sync(mode);
                if let Err(error) = session.set_agent_mode(mode).await {
                    log::warn!("ACP v1 mode change apply failed: {error:#}");
                    let _ = req.response_tx.send("false".into());
                    return Ok(());
                }
            }
            let _ = req.response_tx.send(if approved { "true" } else { "false" }.into());
        }
        AgentUiEvent::PlanConfirmationRequired(req) => {
            if let Ok((session, _, _)) = lookup_session(state, key) {
                let options = vec![
                    PermissionOption::new("implement", "Implement plan", PermissionOptionKind::AllowOnce),
                    PermissionOption::new("fresh", "Implement in a fresh context", PermissionOptionKind::AllowOnce),
                    PermissionOption::new("stay", "Stay in plan mode", PermissionOptionKind::RejectOnce),
                    PermissionOption::new("revise", "Request changes", PermissionOptionKind::RejectOnce),
                    PermissionOption::new("quit", "Leave plan mode", PermissionOptionKind::RejectOnce),
                ];
                let mut fields = agent_client_protocol::schema::v1::ToolCallUpdateFields::new();
                fields.title = Some("Approve plan".into());
                let _ = req.plan_text;
                let request = RequestPermissionRequest::new(
                    SessionId::from(key.to_string()),
                    ToolCallUpdate::new("plan_confirm", fields),
                    options,
                );
                match send_v1_permission(connection, request, cancel.clone()).await.as_deref() {
                    Some("implement") => {
                        let _ = session
                            .resolve_plan(elph_agent::PlanConfirmationChoice::Implement)
                            .await;
                    }
                    Some("fresh") => {
                        let _ = session
                            .resolve_plan(elph_agent::PlanConfirmationChoice::ImplementFresh)
                            .await;
                    }
                    Some("revise") => {
                        let _ = session.clear_pending_plan().await;
                    }
                    Some("quit") => {
                        let _ = session.clear_pending_plan().await;
                        let _ = session.set_agent_mode(crate::types::AgentMode::Build).await;
                    }
                    _ => {
                        let _ = session
                            .resolve_plan(elph_agent::PlanConfirmationChoice::StayInPlan)
                            .await;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

async fn replay_v1(
    connection: &ConnectionTo<Client>,
    session_id: &str,
    session: &crate::agent::CodingAgentSession,
) -> anyhow::Result<()> {
    for (is_user, text) in crate::platform::acp::replay::history_texts(session).await {
        let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)));
        let update = if is_user {
            SessionUpdate::UserMessageChunk(chunk)
        } else {
            SessionUpdate::AgentMessageChunk(chunk)
        };
        notify(connection, session_id, update)?;
    }
    Ok(())
}

async fn ask_user_v1(
    connection: &ConnectionTo<Client>,
    session_id: &str,
    req: crate::agent::UserQuestionRequest,
    cancel: Option<Arc<tokio::sync::Notify>>,
) -> anyhow::Result<()> {
    let mut collected = std::collections::BTreeMap::new();
    let total = req.steps.len().max(1);
    for (index, step) in req.steps.iter().enumerate() {
        let title = if total > 1 {
            format!("({}/{}) {}", index + 1, total, step.question)
        } else {
            step.question.clone()
        };
        let mut options: Vec<PermissionOption> = step
            .options
            .as_ref()
            .into_iter()
            .flatten()
            .map(|opt| PermissionOption::new(opt.value.clone(), opt.label.clone(), PermissionOptionKind::AllowOnce))
            .collect();
        if options.is_empty() {
            if step
                .default
                .as_ref()
                .is_some_and(|value| value == "true" || value == "false")
            {
                options.push(PermissionOption::new("true", "Yes", PermissionOptionKind::AllowOnce));
                options.push(PermissionOption::new("false", "No", PermissionOptionKind::RejectOnce));
            } else if let Some(default) = step.default.as_ref().filter(|d| !d.is_empty()) {
                options.push(PermissionOption::new(
                    default.clone(),
                    format!("Use default ({default})"),
                    PermissionOptionKind::AllowOnce,
                ));
            } else {
                options.push(PermissionOption::new("ok", "OK", PermissionOptionKind::AllowOnce));
            }
        }
        if !step.required {
            options.push(PermissionOption::new("skip", "Skip", PermissionOptionKind::RejectOnce));
        }
        let mut fields = agent_client_protocol::schema::v1::ToolCallUpdateFields::new();
        fields.title = Some(title);
        let request = RequestPermissionRequest::new(
            SessionId::from(session_id.to_string()),
            ToolCallUpdate::new(format!("ask_{}", step.id), fields),
            options,
        );
        match send_v1_permission(connection, request, cancel.clone()).await.as_deref() {
            Some("skip") => {
                collected.insert(step.id.clone(), String::new());
            }
            Some("ok") => {
                collected.insert(step.id.clone(), step.default.clone().unwrap_or_default());
            }
            Some(id) => {
                collected.insert(step.id.clone(), id.to_string());
            }
            None if step.required => {
                let _ = req.response_tx.send(String::new());
                return Ok(());
            }
            None => {
                collected.insert(step.id.clone(), String::new());
            }
        }
    }
    let response = if req.steps.len() == 1 && !req.steps[0].allow_multiple {
        collected.get(&req.steps[0].id).cloned().unwrap_or_default()
    } else {
        serde_json::to_string(&collected).unwrap_or_default()
    };
    let _ = req.response_tx.send(response);
    Ok(())
}

async fn request_v1_tool_approval(
    connection: &ConnectionTo<Client>,
    session_id: &str,
    req: &crate::agent::ToolApprovalRequest,
    cancel: Option<Arc<tokio::sync::Notify>>,
) -> crate::agent::ToolApprovalChoice {
    let options = if req.once_only {
        vec![
            PermissionOption::new("allow-once", "Allow once", PermissionOptionKind::AllowOnce),
            PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
        ]
    } else {
        vec![
            PermissionOption::new("allow-once", "Allow once", PermissionOptionKind::AllowOnce),
            PermissionOption::new("allow-session", "Allow for session", PermissionOptionKind::AllowAlways),
            PermissionOption::new("allow-all", "Allow all tools", PermissionOptionKind::AllowAlways),
            PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
        ]
    };
    let mut fields = agent_client_protocol::schema::v1::ToolCallUpdateFields::new();
    fields.title = Some(req.tool_name.clone());
    let request = RequestPermissionRequest::new(
        SessionId::from(session_id.to_string()),
        ToolCallUpdate::new(req.tool_call_id.clone(), fields),
        options,
    );
    match send_v1_permission(connection, request, cancel.clone()).await.as_deref() {
        Some("allow-once") => crate::agent::ToolApprovalChoice::Approve,
        Some("allow-session") => crate::agent::ToolApprovalChoice::AllowSession,
        Some("allow-all") => crate::agent::ToolApprovalChoice::AllowAllTools,
        _ => crate::agent::ToolApprovalChoice::Reject,
    }
}

async fn request_v1_mode_change(
    connection: &ConnectionTo<Client>,
    session_id: &str,
    req: &crate::agent::ModeChangeRequest,
    cancel: Option<Arc<tokio::sync::Notify>>,
) -> bool {
    let options = vec![
        PermissionOption::new(
            "allow",
            format!("Switch to {}", req.target_mode),
            PermissionOptionKind::AllowOnce,
        ),
        PermissionOption::new("reject", "Stay in current mode", PermissionOptionKind::RejectOnce),
    ];
    let mut fields = agent_client_protocol::schema::v1::ToolCallUpdateFields::new();
    fields.title = Some(format!("Switch to {} mode", req.target_mode));
    let request = RequestPermissionRequest::new(
        SessionId::from(session_id.to_string()),
        ToolCallUpdate::new("mode_change", fields),
        options,
    );
    matches!(send_v1_permission(connection, request, cancel).await.as_deref(), Some("allow"))
}

async fn send_v1_permission(
    connection: &ConnectionTo<Client>,
    request: RequestPermissionRequest,
    cancel: Option<Arc<tokio::sync::Notify>>,
) -> Option<String> {
    let pending = connection.send_request(request).block_task();
    tokio::pin!(pending);
    let response = if let Some(cancel) = cancel {
        tokio::select! {
            result = &mut pending => result.ok()?,
            _ = cancel.notified() => return None,
        }
    } else {
        pending.await.ok()?
    };
    match response.outcome {
        RequestPermissionOutcome::Selected(selected) => Some(selected.option_id.0.to_string()),
        RequestPermissionOutcome::Cancelled | _ => None,
    }
}

async fn set_mode(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    request: &SetSessionModeRequest,
) -> anyhow::Result<SetSessionModeResponse> {
    let key = request.session_id.0.as_ref().to_string();
    let (session, _, _) = lookup_session(state, &key)?;
    let level = parse_thought(request.mode_id.0.as_ref())
        .ok_or_else(|| anyhow::anyhow!("unknown thinking level {}", request.mode_id.0))?;
    session.set_thinking_level(level).await?;
    let _ = notify(
        connection,
        &key,
        SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(request.mode_id.clone())),
    );
    Ok(SetSessionModeResponse::new())
}

async fn set_config_v1(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    request: &SetSessionConfigOptionRequest,
) -> anyhow::Result<SetSessionConfigOptionResponse> {
    let key = request.session_id.0.as_ref().to_string();
    let (session, _, _) = lookup_session(state, &key)?;
    let settings = state.lock().settings.clone();
    let raw = v1_config_raw(&request.value)?;
    apply_config_value(&session, request.config_id.0.as_ref(), &raw).await?;
    if request.config_id.0.as_ref() == "thought_level" {
        let _ = notify(
            connection,
            &key,
            SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(raw.clone())),
        );
    }
    let options = session_config(&session, &settings, true)
        .await
        .into_iter()
        .map(to_v1_option)
        .collect();
    Ok(SetSessionConfigOptionResponse::new(options))
}

fn v1_config_raw(value: &agent_client_protocol::schema::v1::SessionConfigOptionValue) -> anyhow::Result<String> {
    match value {
        agent_client_protocol::schema::v1::SessionConfigOptionValue::ValueId { value } => Ok(value.0.to_string()),
        _ => anyhow::bail!("config option expects an id value"),
    }
}
