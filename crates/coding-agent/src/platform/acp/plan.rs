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
) -> anyhow::Result<()> {
    let options = vec![
        PermissionOption::new("implement", "Implement plan", PermissionOptionKind::AllowOnce),
        PermissionOption::new("fresh", "Implement in a fresh context", PermissionOptionKind::AllowOnce),
        PermissionOption::new("stay", "Stay in plan mode", PermissionOptionKind::RejectOnce),
    ];
    let request = RequestPermissionRequest::new(session_id.clone(), "Approve this plan?", options)
        .description(req.plan_text.clone());
    let choice = match connection.send_request(request).block_task().await {
        Ok(response) => match response.outcome {
            agent_client_protocol::schema::v2::RequestPermissionOutcome::Selected(selected) => {
                match selected.option_id.0.as_ref() {
                    "implement" => elph_agent::PlanConfirmationChoice::Implement,
                    "fresh" => elph_agent::PlanConfirmationChoice::ImplementFresh,
                    _ => elph_agent::PlanConfirmationChoice::StayInPlan,
                }
            }
            _ => elph_agent::PlanConfirmationChoice::StayInPlan,
        },
        Err(err) => {
            log::warn!("plan permission request failed: {err}");
            elph_agent::PlanConfirmationChoice::StayInPlan
        }
    };
    session.resolve_plan(choice).await
}
