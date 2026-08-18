//! Agent plan updates from Elph todos.

use agent_client_protocol::schema::v2::{
    PermissionOption, PermissionOptionKind, PlanEntry, PlanEntryPriority, PlanEntryStatus, PlanItems, PlanUpdate,
    PlanUpdateContent, RequestPermissionRequest, SessionId, SessionUpdate,
};
use agent_client_protocol::{Client, ConnectionTo};
use elph_agent::{TodoItem, TodoStatus};

use crate::agent::PlanConfirmationRequest;
use crate::platform::acp::updates::send_update;

pub fn on_todos(connection: &ConnectionTo<Client>, session_id: &SessionId, items: &[TodoItem]) -> anyhow::Result<()> {
    let entries = items
        .iter()
        .map(|item| PlanEntry::new(item.content.clone(), PlanEntryPriority::Medium, map_status(item.status)))
        .collect();
    let plan = PlanUpdateContent::Items(PlanItems::new("session-plan", entries));
    send_update(connection, session_id, SessionUpdate::PlanUpdate(PlanUpdate::new(plan)))
}

fn map_status(status: TodoStatus) -> PlanEntryStatus {
    match status {
        TodoStatus::Pending => PlanEntryStatus::Pending,
        TodoStatus::InProgress => PlanEntryStatus::InProgress,
        TodoStatus::Completed => PlanEntryStatus::Completed,
        TodoStatus::Cancelled => PlanEntryStatus::Other("cancelled".into()),
    }
}

pub async fn confirm_plan(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: &crate::agent::CodingAgentSession,
    req: &PlanConfirmationRequest,
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> anyhow::Result<()> {
    let options = vec![
        PermissionOption::new("implement", "Implement plan", PermissionOptionKind::AllowOnce),
        PermissionOption::new("fresh", "Implement in a fresh context", PermissionOptionKind::AllowOnce),
        PermissionOption::new("stay", "Stay in plan mode", PermissionOptionKind::RejectOnce),
        PermissionOption::new("revise", "Request changes", PermissionOptionKind::RejectOnce),
        PermissionOption::new("quit", "Leave plan mode", PermissionOptionKind::RejectOnce),
    ];
    let request = RequestPermissionRequest::new(session_id.clone(), "Approve this plan?", options)
        .description(req.plan_text.clone());
    let choice = match crate::platform::acp::permission::send_permission(connection, request, cancel)
        .await
        .as_deref()
    {
        Some("implement") => elph_agent::PlanConfirmationChoice::Implement,
        Some("fresh") => elph_agent::PlanConfirmationChoice::ImplementFresh,
        Some("quit") => {
            session.clear_pending_plan().await?;
            return session.set_agent_mode(crate::types::AgentMode::Build).await;
        }
        Some("revise") => {
            return session.clear_pending_plan().await;
        }
        _ => elph_agent::PlanConfirmationChoice::StayInPlan,
    };
    session.resolve_plan(choice).await
}
