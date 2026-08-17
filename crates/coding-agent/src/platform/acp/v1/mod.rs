//! ACP v1 (stable) stdio agent.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AgentCapabilities, AvailableCommand, AvailableCommandsUpdate, CancelNotification, CloseSessionRequest,
    ContentBlock, ContentChunk, CurrentModeUpdate, DeleteSessionRequest, Implementation, InitializeRequest,
    InitializeResponse, ListSessionsRequest, ListSessionsResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind, Plan, PlanEntry, PlanEntryPriority,
    PlanEntryStatus, PromptCapabilities, PromptRequest, PromptResponse, RequestPermissionOutcome,
    RequestPermissionRequest, ResumeSessionRequest, ResumeSessionResponse, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOption, SessionId, SessionInfo, SessionListCapabilities,
    SessionMode, SessionModeState, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse, StopReason, TextContent, ToolCall,
    ToolCallStatus, ToolCallUpdate, ToolKind,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Result as AcpResult, Stdio};
use parking_lot::Mutex;

use crate::agent::AgentUiEvent;
use crate::platform::acp::config::{ConfigCategory, apply_config_value, parse_thought, session_config};
use crate::platform::acp::session::{close_by_id, list_session_rows, open_or_create};
use crate::platform::acp::state::{AcpAgentState, lookup_session};
use crate::platform::{Paths, Settings};

pub async fn run(paths: Paths, settings: Settings) -> AcpResult<()> {
    let state = Arc::new(Mutex::new(AcpAgentState {
        sessions: HashMap::new(),
        paths,
        settings,
    }));

    Agent
        .builder()
        .name("elph")
        .on_receive_request(
            async move |_initialize: InitializeRequest, responder, _connection| {
                let _ = responder.respond(
                    InitializeResponse::new(ProtocolVersion::V1)
                        .agent_capabilities(v1_capabilities())
                        .agent_info(Implementation::new("elph", env!("CARGO_PKG_VERSION")).title("Elph")),
                );
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: NewSessionRequest, responder, connection| {
                    match open_or_create(&state, &request.cwd, request.additional_directories.clone(), None).await {
                        Ok(id) => {
                            let (modes, options) = advertise_v1(&state, &connection, &id).await;
                            let _ = responder.respond(
                                NewSessionResponse::new(SessionId::from(id))
                                    .modes(modes)
                                    .config_options(options),
                            );
                        }
                        Err(error) => {
                            let _ = responder
                                .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
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
                async move |request: LoadSessionRequest, responder, connection| {
                    match open_or_create(
                        &state,
                        &request.cwd,
                        request.additional_directories.clone(),
                        Some(request.session_id.0.as_ref()),
                    )
                    .await
                    {
                        Ok(id) => {
                            if let Ok((session, _, _)) = lookup_session(&state, &id) {
                                let _ = replay_v1(&connection, &id, &session).await;
                            }
                            let (modes, options) = advertise_v1(&state, &connection, &id).await;
                            let _ = responder.respond(LoadSessionResponse::new().modes(modes).config_options(options));
                        }
                        Err(error) => {
                            let _ = responder
                                .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
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
                async move |request: ResumeSessionRequest, responder, connection| {
                    match open_or_create(
                        &state,
                        &request.cwd,
                        request.additional_directories.clone(),
                        Some(request.session_id.0.as_ref()),
                    )
                    .await
                    {
                        Ok(id) => {
                            let (modes, options) = advertise_v1(&state, &connection, &id).await;
                            let _ =
                                responder.respond(ResumeSessionResponse::new().modes(modes).config_options(options));
                        }
                        Err(error) => {
                            let _ = responder
                                .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
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
                async move |request: ListSessionsRequest, responder, _connection| {
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
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: CloseSessionRequest, responder, _connection| match close_by_id(
                    &state,
                    request.session_id.0.as_ref(),
                )
                .await
                {
                    Ok(()) => {
                        let _ = responder.respond(agent_client_protocol::schema::v1::CloseSessionResponse::new());
                        Ok(())
                    }
                    Err(error) => {
                        let _ = responder
                            .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                        Ok(())
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: DeleteSessionRequest, responder, _connection| {
                    let key = request.session_id.0.as_ref().to_string();
                    let _ = close_by_id(&state, &key).await;
                    let paths = state.lock().paths.clone();
                    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
                    if let Ok(manager) = crate::agent::SessionManager::new(&paths, &cwd) {
                        let _ = manager.delete_by_id(&key).await;
                    }
                    let _ = responder.respond(agent_client_protocol::schema::v1::DeleteSessionResponse::new());
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: SetSessionModeRequest, responder, connection| match set_mode(
                    &state,
                    &connection,
                    &request,
                )
                .await
                {
                    Ok(response) => {
                        let _ = responder.respond(response);
                        Ok(())
                    }
                    Err(error) => {
                        let _ = responder
                            .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                        Ok(())
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: SetSessionConfigOptionRequest, responder, connection| {
                    match set_config_v1(&state, &connection, &request).await {
                        Ok(response) => {
                            let _ = responder.respond(response);
                        }
                        Err(error) => {
                            let _ = responder
                                .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
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
                async move |request: PromptRequest, responder, connection| match run_prompt(
                    &state,
                    &connection,
                    &request,
                )
                .await
                {
                    Ok(reason) => {
                        let _ = responder.respond(PromptResponse::new(reason));
                        Ok(())
                    }
                    Err(error) => {
                        let _ = responder
                            .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                        Ok(())
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let state = Arc::clone(&state);
                async move |notification: CancelNotification, _connection| {
                    let key = notification.session_id.0.as_ref().to_string();
                    if let Ok((session, _, _)) = lookup_session(&state, &key) {
                        if let Some(entry) = state.lock().sessions.get(&key) {
                            entry.cancelled.store(true, Ordering::Relaxed);
                        }
                        let _ = session.abort().await;
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(Stdio::new())
        .await
}

async fn advertise_v1(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &str,
) -> (SessionModeState, Vec<SessionConfigOption>) {
    let settings = state.lock().settings.clone();
    if let Ok((session, _, _)) = lookup_session(state, session_id) {
        let _ = send_v1_commands(connection, session_id, &session).await;
        let snapshot = session_config(&session, &settings).await;
        let modes = v1_thought_modes(&snapshot);
        let options = snapshot.into_iter().map(to_v1_option).collect();
        return (modes, options);
    }
    (v1_thought_modes_fallback(), Vec::new())
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
        .load_session(true)
        .prompt_capabilities(PromptCapabilities::new().embedded_context(true))
        .session_capabilities(
            agent_client_protocol::schema::v1::SessionCapabilities::new()
                .list(SessionListCapabilities::new())
                .resume(agent_client_protocol::schema::v1::SessionResumeCapabilities::new())
                .close(agent_client_protocol::schema::v1::SessionCloseCapabilities::new()),
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

async fn run_prompt(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    request: &PromptRequest,
) -> anyhow::Result<StopReason> {
    let key = request.session_id.0.as_ref().to_string();
    let (session, ui_rx, _) = lookup_session(state, &key)?;
    let text = extract_text(&request.prompt)?;
    let trimmed = text.trim();
    if crate::platform::acp::commands::is_slash(trimmed) {
        return run_slash_v1(state, connection, &key, trimmed).await;
    }
    session.submit_prompt(text, false).await?;
    stream_v1(state, connection, &key, &ui_rx).await
}

async fn run_slash_v1(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    key: &str,
    input: &str,
) -> anyhow::Result<StopReason> {
    match crate::platform::acp::commands::resolve_slash(state, key, input).await? {
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
            session
                .submit_prompt(crate::agent::RETRY_CONTINUE_PROMPT.to_string(), false)
                .await?;
            stream_v1(state, connection, key, &ui_rx).await
        }
        crate::platform::acp::commands::SlashOutcome::SubmitPrompt => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            session.submit_prompt(input.to_string(), false).await?;
            stream_v1(state, connection, key, &ui_rx).await
        }
    }
}

fn v1_tool_update(id: String, status: ToolCallStatus, output: String) -> ToolCallUpdate {
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

async fn stream_v1(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    key: &str,
    ui_rx: &Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<StopReason> {
    let mut rx = ui_rx.lock().await;
    while let Some(event) = rx.recv().await {
        if state
            .lock()
            .sessions
            .get(key)
            .is_some_and(|s| s.cancelled.load(Ordering::Relaxed))
        {
            return Ok(StopReason::Cancelled);
        }
        match event {
            AgentUiEvent::TextDelta(text) if !text.is_empty() => {
                notify(
                    connection,
                    key,
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text)))),
                )?;
            }
            AgentUiEvent::ThinkingDelta(text) if !text.is_empty() => {
                notify(
                    connection,
                    key,
                    SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text)))),
                )?;
            }
            AgentUiEvent::ToolStart {
                id, name, args_summary, ..
            } => {
                let call = ToolCall::new(id, name.clone())
                    .kind(map_kind(&name))
                    .status(ToolCallStatus::InProgress)
                    .raw_input(serde_json::json!({ "summary": args_summary }));
                notify(connection, key, SessionUpdate::ToolCall(call))?;
            }
            AgentUiEvent::ToolUpdate { id, output } => {
                notify(
                    connection,
                    key,
                    SessionUpdate::ToolCallUpdate(v1_tool_update(id, ToolCallStatus::InProgress, output)),
                )?;
            }
            AgentUiEvent::ToolEnd {
                id, is_error, output, ..
            } => {
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
            AgentUiEvent::RunCompleted { .. } => return Ok(StopReason::EndTurn),
            AgentUiEvent::ToolApprovalRequired(req) => {
                let choice = request_v1_tool_approval(connection, key, &req).await;
                let _ = req.response_tx.send(choice);
            }
            AgentUiEvent::UserQuestionRequired(req) => {
                let _ = req.response_tx.send(String::new());
            }
            AgentUiEvent::ModeChangeRequired(req) => {
                let approved = request_v1_mode_change(connection, key, &req).await;
                let _ = req.response_tx.send(if approved {
                    req.target_mode.clone()
                } else {
                    String::new()
                });
            }
            AgentUiEvent::PlanConfirmationRequired(_) => {}
            _ => {}
        }
    }
    Ok(StopReason::EndTurn)
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

async fn request_v1_tool_approval(
    connection: &ConnectionTo<Client>,
    session_id: &str,
    req: &crate::agent::ToolApprovalRequest,
) -> crate::agent::ToolApprovalChoice {
    let options = vec![
        PermissionOption::new("allow-once", "Allow once", PermissionOptionKind::AllowOnce),
        PermissionOption::new("allow-session", "Allow for session", PermissionOptionKind::AllowAlways),
        PermissionOption::new("allow-all", "Allow all tools", PermissionOptionKind::AllowAlways),
        PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
    ];
    let mut fields = agent_client_protocol::schema::v1::ToolCallUpdateFields::new();
    fields.title = Some(req.tool_name.clone());
    let request = RequestPermissionRequest::new(
        SessionId::from(session_id.to_string()),
        ToolCallUpdate::new(req.tool_call_id.clone(), fields),
        options,
    );
    match send_v1_permission(connection, request).await.as_deref() {
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
) -> bool {
    let options = vec![
        PermissionOption::new(
            "allow",
            format!("Switch to {}", req.target_mode),
            PermissionOptionKind::AllowOnce,
        ),
        PermissionOption::new("reject", "Stay in current mode", PermissionOptionKind::RejectOnce),
    ];
    let request = RequestPermissionRequest::new(
        SessionId::from(session_id.to_string()),
        ToolCallUpdate::new("mode_change", agent_client_protocol::schema::v1::ToolCallUpdateFields::new()),
        options,
    );
    matches!(send_v1_permission(connection, request).await.as_deref(), Some("allow"))
}

async fn send_v1_permission(connection: &ConnectionTo<Client>, request: RequestPermissionRequest) -> Option<String> {
    let response = connection.send_request(request).block_task().await.ok()?;
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
    let options = session_config(&session, &settings)
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
