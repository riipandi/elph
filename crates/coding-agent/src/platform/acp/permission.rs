//! `session/request_permission` for tools and agent mode changes.

use std::sync::Arc;

use agent_client_protocol::schema::v2::{
    PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionSubject, SessionId, ToolCallUpdate,
};
use agent_client_protocol::{Client, ConnectionTo};
use tokio::sync::Notify;

use crate::agent::{CodingAgentSession, ModeChangeRequest, ToolApprovalChoice, ToolApprovalRequest};
use crate::platform::acp::tools;

pub async fn request_tool_approval(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    req: &ToolApprovalRequest,
    cancel: Option<Arc<Notify>>,
) -> ToolApprovalChoice {
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
    let subject = RequestPermissionSubject::from(
        ToolCallUpdate::new(req.tool_call_id.clone())
            .title(req.tool_name.clone())
            .kind(tools::kind_for_tool(&req.tool_name)),
    );
    let request = RequestPermissionRequest::new(session_id.clone(), format!("Allow {}?", req.tool_name), options)
        .description(req.args_summary.clone())
        .subject(subject);

    map_choice(send_permission(connection, request, cancel).await)
}

pub async fn request_mode_change(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: &CodingAgentSession,
    req: ModeChangeRequest,
    cancel: Option<Arc<Notify>>,
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
        send_permission(connection, request, cancel).await,
        Some(id) if id == "allow"
    );
    if approved {
        let mode = crate::agent::agent_mode_from_setting(&req.target_mode);
        session.invalidate_system_prompt_cache();
        session.try_set_mode_sync(mode);
        if let Err(error) = session.set_agent_mode(mode).await {
            log::warn!("ACP mode change apply failed: {error:#}");
            let _ = req.response_tx.send("false".into());
            return Ok(());
        }
    }
    let _ = req.response_tx.send(if approved { "true" } else { "false" }.into());
    Ok(())
}

pub async fn send_permission(
    connection: &ConnectionTo<Client>,
    request: RequestPermissionRequest,
    cancel: Option<Arc<Notify>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_permission_option_ids() {
        assert_eq!(map_choice(Some("allow-once".into())), ToolApprovalChoice::Approve);
        assert_eq!(map_choice(Some("allow-session".into())), ToolApprovalChoice::AllowSession);
        assert_eq!(map_choice(Some("allow-all".into())), ToolApprovalChoice::AllowAllTools);
        assert_eq!(map_choice(Some("reject".into())), ToolApprovalChoice::Reject);
        assert_eq!(map_choice(None), ToolApprovalChoice::Reject);
    }
}
