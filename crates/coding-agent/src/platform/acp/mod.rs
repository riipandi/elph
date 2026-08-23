//! ACP agent server over stdio (v1 stable, v2 experimental).

mod auth;
mod capabilities;
mod commands;
mod config;
mod content;
mod elicitation;
mod limits;
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
    ListSessionsRequest, LoginAuthRequest, LoginAuthResponse, LogoutAuthRequest, LogoutAuthResponse, NewSessionRequest,
    PromptRequest, ResumeSessionRequest, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
};
use agent_client_protocol::{Agent, ConnectTo, Result as AcpResult, Stdio};
use parking_lot::Mutex;

use crate::platform::acp::state::{AcpAgentState, lookup_session, session_key};
use crate::platform::{Paths, Settings};

/// Run Elph as an ACP agent on stdio.
pub async fn run_agent_stdio(paths: Paths, settings: Settings, mode: AcpMode) -> AcpResult<()> {
    run_agent_on(paths, settings, mode, Stdio::new()).await
}

/// Run the ACP agent on an arbitrary transport (stdio or in-process streams).
pub async fn run_agent_on<T>(paths: Paths, settings: Settings, mode: AcpMode, transport: T) -> AcpResult<()>
where
    T: ConnectTo<Agent> + 'static,
{
    match mode {
        AcpMode::V1 => v1::run_with(paths, settings, transport).await,
        AcpMode::V2 => run_v2_with(paths, settings, transport).await,
    }
}

async fn run_v2_with<T>(paths: Paths, settings: Settings, transport: T) -> AcpResult<()>
where
    T: ConnectTo<Agent> + 'static,
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
        .v2()
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |initialize: InitializeRequest, responder, _connection| {
                    {
                        let mut guard = state.lock();
                        guard.client_elicitation_form = initialize
                            .capabilities
                            .elicitation
                            .as_ref()
                            .is_some_and(|e| e.form.is_some());
                    }
                    let _ = responder.respond(
                        InitializeResponse::new(ProtocolVersion::V2, capabilities::implementation())
                            .capabilities(capabilities::agent_capabilities())
                            .auth_methods(auth::v2_auth_methods()),
                    );
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: LoginAuthRequest, responder, _connection| {
                    match auth::login(&state, request.method_id.0.as_ref()) {
                        Ok(()) => {
                            let _ = responder.respond(LoginAuthResponse::new());
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
                async move |_request: LogoutAuthRequest, responder, connection| {
                    let state = Arc::clone(&state);
                    if let Err(error) = connection.spawn(async move {
                        auth::logout(&state).await;
                        let _ = responder.respond(LogoutAuthResponse::new());
                        Ok(())
                    }) {
                        log::warn!("ACP logout spawn failed: {error}");
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
                    if let Err(error) = auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    if let Err(error) = session::require_absolute_cwd(&request.cwd.0) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        // Isolate panics so they cannot tear down the JSON-RPC stdio loop
                        // (client then reports `incoming_transport_closed` on session/new).
                        let opened = tokio::spawn({
                            let state = Arc::clone(&state);
                            let request = request.clone();
                            let conn = conn.clone();
                            async move { session::create_session(&state, &request, &conn).await }
                        })
                        .await;
                        match opened {
                            Ok(Ok(response)) => {
                                let sid = response.session_id.clone();
                                let _ = responder.respond(response);
                                session::finish_create_session(&state, &request, &conn, &sid).await;
                            }
                            Ok(Err(error)) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                            Err(join) => {
                                log::error!("ACP session/new panicked: {join}");
                                let _ = responder.respond_with_error(agent_client_protocol::util::internal_error(
                                    format!("session/new panicked: {join}"),
                                ));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP session/new spawn failed: {error}");
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
                    if let Err(error) = auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    if let Err(error) = session::require_absolute_cwd(&request.cwd.0) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        match session::resume_session(&state, &request, &conn).await {
                            Ok(response) => {
                                let sid = request.session_id.clone();
                                let _ = responder.respond(response);
                                session::finish_resume_session(&state, &request, &conn, &sid).await;
                            }
                            Err(error) => {
                                let _ = responder
                                    .respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
                            }
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP session/resume spawn failed: {error}");
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
                    }) {
                        log::warn!("ACP session/list spawn failed: {error}");
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
                    }) {
                        log::warn!("ACP session/close spawn failed: {error}");
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
                    }) {
                        log::warn!("ACP session/delete spawn failed: {error}");
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
                    if let Err(error) = auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        match set_config(&state, &request, &conn).await {
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
                        log::warn!("ACP set_config spawn failed: {error}");
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
                    if let Err(error) = auth::require(&state) {
                        let _ = responder.respond_with_error(error);
                        return Ok(());
                    }
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
                    crate::platform::acp::state::mark_session_cancelled(&state, &session_key(&notification.session_id));
                    let state = Arc::clone(&state);
                    let conn = connection.clone();
                    if let Err(error) = connection.spawn(async move {
                        if let Err(error) = prompt::handle_cancel(&state, &conn, notification).await {
                            log::warn!("ACP cancel failed: {error:#}");
                        }
                        Ok(())
                    }) {
                        log::warn!("ACP cancel spawn failed: {error}");
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(transport)
        .await
}

async fn set_config(
    state: &Arc<Mutex<AcpAgentState>>,
    request: &SetSessionConfigOptionRequest,
    connection: &agent_client_protocol::ConnectionTo<agent_client_protocol::Client>,
) -> anyhow::Result<SetSessionConfigOptionResponse> {
    let key = session_key(&request.session_id);
    let (session, _, _) = lookup_session(state, &key)?;
    let settings = state.lock().settings.clone();
    let options = config::set_config_option(&session, &settings, request.config_id.0.as_ref(), &request.value).await?;
    let _ = updates::send_update(
        connection,
        &request.session_id,
        agent_client_protocol::schema::v2::SessionUpdate::ConfigOptionUpdate(
            agent_client_protocol::schema::v2::ConfigOptionUpdate::new(options.clone()),
        ),
    );
    Ok(SetSessionConfigOptionResponse::new(options))
}
