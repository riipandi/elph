//! ACP agent server over stdio (v1 stable, v2 experimental).

mod capabilities;
mod commands;
mod config;
mod content;
mod elicitation;
mod mcp;
mod permission;
mod plan;
mod prompt;
mod replay;
mod session;
mod state;
mod terminals;
mod tools;
mod updates;
mod v1;

/// Which ACP protocol this process speaks (one version per process).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpMode {
    /// ACP v1 (stable). `elph acp --stdio`
    V1,
    /// ACP v2 draft. `elph acp --stdio --experimental`
    V2,
}

use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v2::{
    CancelSessionNotification, CloseSessionRequest, DeleteSessionRequest, InitializeRequest, InitializeResponse,
    ListSessionsRequest, NewSessionRequest, PromptRequest, ResumeSessionRequest, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse,
};
use agent_client_protocol::{Agent, Result as AcpResult, Stdio};
use parking_lot::Mutex;

use crate::platform::acp::state::{AcpAgentState, lookup_session, session_key};
use crate::platform::{Paths, Settings};

/// Run Elph as an ACP agent on stdio.
pub async fn run_agent_stdio(paths: Paths, settings: Settings, mode: AcpMode) -> AcpResult<()> {
    match mode {
        AcpMode::V1 => v1::run(paths, settings).await,
        AcpMode::V2 => run_v2(paths, settings).await,
    }
}

async fn run_v2(paths: Paths, settings: Settings) -> AcpResult<()> {
    let state = Arc::new(Mutex::new(AcpAgentState {
        sessions: HashMap::new(),
        paths,
        settings,
    }));

    Agent
        .v2()
        .on_receive_request(
            async move |initialize: InitializeRequest, responder, _connection| {
                let _ = initialize;
                let _ = responder.respond(
                    InitializeResponse::new(ProtocolVersion::V2, capabilities::implementation())
                        .capabilities(capabilities::agent_capabilities()),
                );
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: NewSessionRequest, responder, connection| {
                    match session::create_session(&state, &request, &connection).await {
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
                async move |request: ResumeSessionRequest, responder, connection| {
                    match session::resume_session(&state, &request, &connection).await {
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
                async move |request: ListSessionsRequest, responder, _connection| {
                    match session::list_sessions(&state, &request).await {
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
                async move |request: CloseSessionRequest, responder, _connection| {
                    match session::close_session(&state, &request).await {
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
                async move |request: DeleteSessionRequest, responder, _connection| {
                    match session::delete_session(&state, &request).await {
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
                async move |request: SetSessionConfigOptionRequest, responder, connection| {
                    match set_config(&state, &request, &connection).await {
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
                async move |request: PromptRequest, responder, connection| {
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        if let Err(error) = prompt::handle_prompt(state, conn, request, responder).await {
                            log::warn!("ACP prompt failed: {error:#}");
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP prompt spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let state = Arc::clone(&state);
                async move |notification: CancelSessionNotification, connection| {
                    if let Err(error) = prompt::handle_cancel(&state, &connection, notification).await {
                        log::warn!("ACP cancel failed: {error:#}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(Stdio::new())
        .await
}

async fn set_config(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &SetSessionConfigOptionRequest,
    connection: &agent_client_protocol::ConnectionTo<agent_client_protocol::Client>,
) -> anyhow::Result<SetSessionConfigOptionResponse> {
    let key = session_key(&request.session_id);
    let (session, _, _) = lookup_session(state, &key)?;
    let options = config::set_config_option(&session, request.config_id.0.as_ref(), &request.value).await?;
    let _ = updates::send_update(
        connection,
        &request.session_id,
        agent_client_protocol::schema::v2::SessionUpdate::ConfigOptionUpdate(
            agent_client_protocol::schema::v2::ConfigOptionUpdate::new(options.clone()),
        ),
    );
    Ok(SetSessionConfigOptionResponse::new(options))
}
