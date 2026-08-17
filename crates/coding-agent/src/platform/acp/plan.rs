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
    req: &PlanConfirmationRequest,
) -> anyhow::Result<()> {
    let options = vec![
        PermissionOption::new("allow", "Approve plan", PermissionOptionKind::AllowOnce),
        PermissionOption::new("reject", "Reject plan", PermissionOptionKind::RejectOnce),
    ];
    let request = RequestPermissionRequest::new(session_id.clone(), "Approve this plan?", options)
        .description(req.plan_text.clone());
    if let Err(err) = connection.send_request(request).block_task().await {
        log::warn!("plan permission request failed: {err}");
    }
    Ok(())
}
