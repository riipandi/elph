//! `session/request_permission` bridge.

use agent_client_protocol::schema::v2::{
    PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionSubject, SessionId, ToolCallUpdate,
};
use agent_client_protocol::{Client, ConnectionTo};

use crate::agent::{ModeChangeRequest, ToolApprovalChoice, ToolApprovalRequest};
use crate::platform::acp::tools;

pub async fn request_tool_approval(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    req: &ToolApprovalRequest,
) -> ToolApprovalChoice {
    let options = vec![
        PermissionOption::new("allow-once", "Allow once", PermissionOptionKind::AllowOnce),
        PermissionOption::new("allow-session", "Allow for session", PermissionOptionKind::AllowAlways),
        PermissionOption::new("allow-all", "Allow all tools", PermissionOptionKind::AllowAlways),
        PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
    ];
    let subject = RequestPermissionSubject::from(
        ToolCallUpdate::new(req.tool_call_id.clone())
            .title(req.tool_name.clone())
            .kind(tools::kind_for_tool(&req.tool_name)),
    );
    let request = RequestPermissionRequest::new(session_id.clone(), format!("Allow {}?", req.tool_name), options)
        .description(req.args_summary.clone())
        .subject(subject);

    map_choice(send_permission(connection, request).await)
}

pub async fn request_mode_change(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    req: ModeChangeRequest,
) -> anyhow::Result<()> {
    let options = vec![
        PermissionOption::new(
            "allow",
            format!("Switch to {}", req.target_mode),
            PermissionOptionKind::AllowOnce,
        ),
        PermissionOption::new("reject", "Stay in current mode", PermissionOptionKind::RejectOnce),
    ];
    let request =
        RequestPermissionRequest::new(session_id.clone(), format!("Switch to {} mode?", req.target_mode), options)
            .description(req.reason.clone());
    let approved = matches!(
        send_permission(connection, request).await,
        Some(id) if id == "allow"
    );
    let reply = if approved {
        req.target_mode.clone()
    } else {
        String::new()
    };
    let _ = req.response_tx.send(reply);
    Ok(())
}

async fn send_permission(connection: &ConnectionTo<Client>, request: RequestPermissionRequest) -> Option<String> {
    let response = connection.send_request(request).block_task().await.ok()?;
    match response.outcome {
        RequestPermissionOutcome::Selected(selected) => Some(selected.option_id.0.to_string()),
        RequestPermissionOutcome::Cancelled | RequestPermissionOutcome::Other(_) | _ => None,
    }
}

fn map_choice(option_id: Option<String>) -> ToolApprovalChoice {
    match option_id.as_deref() {
        Some("allow-once") => ToolApprovalChoice::Approve,
        Some("allow-session") => ToolApprovalChoice::AllowSession,
        Some("allow-all") => ToolApprovalChoice::AllowAllTools,
        _ => ToolApprovalChoice::Reject,
    }
}
