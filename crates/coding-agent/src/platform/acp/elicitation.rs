//! User questions via permission fallback.
//!
//! TODO(later): ACP elicitation forms (`session/elicitation`) — not advertised yet.
//! TODO(later): WASM extension slash commands in `available_commands_update`.

use agent_client_protocol::schema::v2::{PermissionOption, PermissionOptionKind, RequestPermissionRequest, SessionId};
use agent_client_protocol::{Client, ConnectionTo};

use crate::agent::UserQuestionRequest;

pub async fn ask_user(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    req: UserQuestionRequest,
) -> anyhow::Result<()> {
    let first = req.steps.first();
    let title = first.map(|s| s.question.clone()).unwrap_or_else(|| "Question".into());
    let mut options: Vec<PermissionOption> = first
        .and_then(|s| s.options.as_ref())
        .map(|opts| {
            opts.iter()
                .map(|opt| PermissionOption::new(opt.value.clone(), opt.label.clone(), PermissionOptionKind::AllowOnce))
                .collect()
        })
        .unwrap_or_default();
    if options.is_empty() {
        options.push(PermissionOption::new("ok", "OK", PermissionOptionKind::AllowOnce));
    }
    options.push(PermissionOption::new("skip", "Skip", PermissionOptionKind::RejectOnce));

    let request = RequestPermissionRequest::new(session_id.clone(), title, options);
    let response = connection
        .send_request(request)
        .block_task()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let answer = match response.outcome {
        agent_client_protocol::schema::v2::RequestPermissionOutcome::Selected(selected) => {
            selected.option_id.0.to_string()
        }
        agent_client_protocol::schema::v2::RequestPermissionOutcome::Cancelled
        | agent_client_protocol::schema::v2::RequestPermissionOutcome::Other(_)
        | _ => String::new(),
    };
    let _ = req.response_tx.send(answer);
    Ok(())
}
