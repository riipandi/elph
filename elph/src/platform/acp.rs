//! ACP (Agent Client Protocol) agent server over stdio.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse, NewSessionRequest,
    NewSessionResponse, PromptRequest, PromptResponse, SessionId, SessionNotification, SessionUpdate, StopReason,
    TextContent,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Dispatch, Result as AcpResult, Stdio};
use anyhow::Context;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::agent::{AgentUiEvent, CodingAgentSession, CreateSessionOptions, create_coding_session_with_events};
use crate::platform::{Paths, Settings};

struct AcpSessionState {
    session: Arc<CodingAgentSession>,
    ui_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
}

struct AcpAgentState {
    sessions: HashMap<String, AcpSessionState>,
    paths: Paths,
    settings: Settings,
}

/// Run Elph as an ACP agent on stdio (for IDE / CLI clients).
pub async fn run_agent_stdio(paths: Paths, settings: Settings) -> AcpResult<()> {
    let state = Arc::new(Mutex::new(AcpAgentState {
        sessions: HashMap::new(),
        paths,
        settings,
    }));

    Agent
        .builder()
        .name("elph")
        .on_receive_request(
            async move |initialize: InitializeRequest, responder, _connection| {
                responder.respond(
                    InitializeResponse::new(initialize.protocol_version).agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: NewSessionRequest, responder, _connection| match create_acp_session(
                    &state,
                    &request.cwd,
                )
                .await
                {
                    Ok(session_id) => responder.respond(NewSessionResponse::new(session_id)),
                    Err(error) => {
                        responder.respond_with_error(agent_client_protocol::util::internal_error(error.to_string()))
                    }
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
                    &request.session_id,
                    &request,
                )
                .await
                {
                    Ok(()) => responder.respond(PromptResponse::new(StopReason::EndTurn)),
                    Err(error) => {
                        responder.respond_with_error(agent_client_protocol::util::internal_error(error.to_string()))
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_dispatch(
            async move |message: Dispatch, cx: ConnectionTo<Client>| {
                message.respond_with_error(agent_client_protocol::util::internal_error("unhandled ACP message"), cx)
            },
            agent_client_protocol::on_receive_dispatch!(),
        )
        .connect_to(Stdio::new())
        .await
}

async fn create_acp_session(state: &Arc<Mutex<AcpAgentState>>, cwd: &PathBuf) -> anyhow::Result<SessionId> {
    let (paths, settings) = {
        let guard = state.lock();
        (guard.paths.clone(), guard.settings.clone())
    };

    let (session, ui_rx) = create_coding_session_with_events(CreateSessionOptions {
        paths: &paths,
        settings: &settings,
        cwd,
        resume_id: None,
        provider_override: None,
        model_override: None,
    })
    .await?;

    let session_id = SessionId::from(session.session_id().to_string());
    state.lock().sessions.insert(
        session.session_id().to_string(),
        AcpSessionState {
            session: Arc::new(session),
            ui_rx: Arc::new(tokio::sync::Mutex::new(ui_rx)),
        },
    );
    Ok(session_id)
}

async fn run_prompt(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    request: &PromptRequest,
) -> anyhow::Result<()> {
    let key = session_id.0.to_string();
    let (session, ui_rx) = {
        let guard = state.lock();
        let entry = guard.sessions.get(&key).context("ACP session not found")?;
        (Arc::clone(&entry.session), entry.ui_rx.clone())
    };

    let prompt = extract_prompt_text(request);
    let mut rx = ui_rx.lock().await;
    session.submit_prompt(prompt, false).await?;

    while let Some(event) = rx.recv().await {
        match event {
            AgentUiEvent::TextDelta(text) if !text.is_empty() => {
                let notification = SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text)))),
                );
                connection.send_notification(notification)?;
            }
            AgentUiEvent::RunCompleted { .. } => break,
            _ => {}
        }
    }

    Ok(())
}

fn extract_prompt_text(request: &PromptRequest) -> String {
    request
        .prompt
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
