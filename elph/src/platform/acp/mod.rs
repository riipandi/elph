//! ACP (Agent Client Protocol) agent server over stdio.

mod handler;
mod util;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse, PromptRequest,
    PromptResponse, SessionId, StopReason,
};
use agent_client_protocol::{Agent, Result as AcpResult, Stdio};
use anyhow::Context;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::agent::{AgentUiEvent, CodingAgentSession, CreateSessionOptions, create_coding_session_with_events};
use crate::platform::{Paths, Settings};

/// Handle + receiver + working directory extracted from an active session.
type SessionContext = (
    Arc<CodingAgentSession>,
    Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
    PathBuf,
);

/// Per-session runtime state kept for the duration of the ACP connection.
struct AcpSessionState {
    session: Arc<CodingAgentSession>,
    ui_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
    cwd: PathBuf,
}

/// Shared mutable state across all ACP sessions.
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
                async move |request: PromptRequest, responder, connection| match self::handler::run_prompt(
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
        .connect_to(Stdio::new())
        .await
}

/// Create a new agent session and register it in shared state.
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
        preloaded_resources: None,
        defer_mcp_load: false,
    })
    .await?;

    let session_id = SessionId::from(session.session_id().to_string());
    let key = session.session_id().to_string();

    state.lock().sessions.insert(
        key,
        AcpSessionState {
            session: Arc::new(session),
            ui_rx: Arc::new(tokio::sync::Mutex::new(ui_rx)),
            cwd: cwd.clone(),
        },
    );
    Ok(session_id)
}

/// Look up session state by its string key.
///
/// Returns the session handle, its UI event receiver (for streaming), and the
/// working directory it was created with (for `/reload` and similar commands).
fn lookup_session(state: &Arc<Mutex<AcpAgentState>>, key: &str) -> anyhow::Result<SessionContext> {
    let guard = state.lock();
    let entry = guard.sessions.get(key).context("ACP session not found")?;
    Ok((Arc::clone(&entry.session), entry.ui_rx.clone(), entry.cwd.clone()))
}
