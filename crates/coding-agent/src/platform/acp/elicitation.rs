//! `ask_user_question` via elicitation forms when the client supports them,
//! otherwise `session/request_permission`.

use std::collections::BTreeMap;

use agent_client_protocol::schema::v2::{
    CreateElicitationRequest, ElicitationAction, ElicitationFormMode, ElicitationMode, ElicitationSchema,
    ElicitationScope, ElicitationSessionScope, PermissionOption, PermissionOptionKind, RequestPermissionRequest,
    SessionId,
};
use agent_client_protocol::{Client, ConnectionTo};

use crate::agent::{UserQuestionOption, UserQuestionRequest, UserQuestionStep};
use crate::platform::acp::permission::send_permission;

pub async fn ask_user(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    req: UserQuestionRequest,
    prefer_form: bool,
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> anyhow::Result<()> {
    if prefer_form && let Some(answer) = try_form(connection, session_id, &req.steps, cancel.clone()).await {
        let _ = req.response_tx.send(answer);
        return Ok(());
    }
    let mut collected = BTreeMap::new();
    let total = req.steps.len().max(1);
    for (index, step) in req.steps.iter().enumerate() {
        let title = if total > 1 {
            format!("({}/{}) {}", index + 1, total, step.question)
        } else {
            step.question.clone()
        };
        let answer = ask_step(connection, session_id, &title, step, cancel.clone()).await;
        if answer.is_none() && step.required {
            let _ = req.response_tx.send(String::new());
            return Ok(());
        }
        collected.insert(step.id.clone(), answer.unwrap_or_default());
    }
    let _ = req.response_tx.send(finalize_answers(&req.steps, &collected));
    Ok(())
}

async fn try_form(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    steps: &[UserQuestionStep],
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> Option<String> {
    let mut schema = ElicitationSchema::new().title("Questions");
    for step in steps {
        schema = schema.string(&step.id, step.required);
    }
    let message = steps
        .iter()
        .map(|s| format!("{}: {}", s.id, s.question))
        .collect::<Vec<_>>()
        .join("\n");
    let mode = ElicitationMode::Form(ElicitationFormMode::new(
        ElicitationScope::Session(ElicitationSessionScope::new(session_id.clone())),
        schema,
    ));
    let request = CreateElicitationRequest::new(mode, message);
    let pending = connection.send_request(request).block_task();
    tokio::pin!(pending);
    let response = if let Some(cancel) = cancel {
        tokio::select! {
            result = &mut pending => result.ok()?,
            _ = cancel.notified() => return Some(String::new()),
        }
    } else {
        pending.await.ok()?
    };
    match response.action {
        ElicitationAction::Accept(accept) => {
            let mut collected = BTreeMap::new();
            if let Some(content) = accept.content {
                for step in steps {
                    if let Some(value) = content.get(&step.id) {
                        collected.insert(step.id.clone(), elicitation_to_string(value));
                    }
                }
            }
            Some(finalize_answers(steps, &collected))
        }
        ElicitationAction::Decline | ElicitationAction::Cancel | _ => Some(String::new()),
    }
}

fn elicitation_to_string(value: &agent_client_protocol::schema::v2::ElicitationContentValue) -> String {
    use agent_client_protocol::schema::v2::ElicitationContentValue;
    match value {
        ElicitationContentValue::String(s) => s.clone(),
        ElicitationContentValue::Boolean(b) => b.to_string(),
        ElicitationContentValue::Integer(n) => n.to_string(),
        ElicitationContentValue::Number(n) => n.to_string(),
        ElicitationContentValue::StringArray(items) => serde_json::to_string(items).unwrap_or_default(),
        _ => String::new(),
    }
}

async fn ask_step(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    title: &str,
    step: &UserQuestionStep,
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> Option<String> {
    if step.allow_multiple {
        return ask_multi(connection, session_id, title, step, cancel).await;
    }
    if is_confirm(step) {
        return ask_confirm(connection, session_id, title, step, cancel).await;
    }
    if let Some(options) = step.options.as_ref().filter(|opts| !opts.is_empty()) {
        return ask_select(connection, session_id, title, step, options, cancel).await;
    }
    ask_text_fallback(connection, session_id, title, step, cancel).await
}

async fn ask_confirm(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    title: &str,
    step: &UserQuestionStep,
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> Option<String> {
    let mut options = vec![
        PermissionOption::new("true", "Yes", PermissionOptionKind::AllowOnce),
        PermissionOption::new("false", "No", PermissionOptionKind::RejectOnce),
    ];
    if !step.required {
        options.push(PermissionOption::new("skip", "Skip", PermissionOptionKind::RejectOnce));
    }
    match send_choice(connection, session_id, title, step_description(step), options, cancel).await {
        Some(id) if id != "skip" => Some(id),
        Some(_) => Some(String::new()),
        None => None,
    }
}

async fn ask_select(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    title: &str,
    step: &UserQuestionStep,
    choices: &[UserQuestionOption],
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> Option<String> {
    let mut options: Vec<PermissionOption> = choices
        .iter()
        .map(|opt| {
            let label = match &opt.hint {
                Some(hint) if !hint.is_empty() => format!("{} — {hint}", opt.label),
                _ => opt.label.clone(),
            };
            PermissionOption::new(opt.value.clone(), label, PermissionOptionKind::AllowOnce)
        })
        .collect();
    if step.allow_custom
        && let Some(default) = step.default.as_ref().filter(|d| !d.is_empty())
    {
        options.push(PermissionOption::new(
            default.clone(),
            format!("{} ({default})", step.custom_label),
            PermissionOptionKind::AllowOnce,
        ));
    }
    if !step.required {
        options.push(PermissionOption::new("skip", "Skip", PermissionOptionKind::RejectOnce));
    }
    match send_choice(connection, session_id, title, step_description(step), options, cancel).await {
        Some(id) if id != "skip" => Some(id),
        Some(_) => Some(String::new()),
        None => None,
    }
}

async fn ask_multi(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    title: &str,
    step: &UserQuestionStep,
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> Option<String> {
    let Some(choices) = step.options.as_ref().filter(|opts| !opts.is_empty()) else {
        return ask_text_fallback(connection, session_id, title, step, cancel).await;
    };
    let mut selected = Vec::new();
    for (i, opt) in choices.iter().enumerate() {
        let opt_title = format!("{title} — include {}?", opt.label);
        let options = vec![
            PermissionOption::new("yes", format!("Include {}", opt.label), PermissionOptionKind::AllowOnce),
            PermissionOption::new("no", "Do not include", PermissionOptionKind::RejectOnce),
        ];
        let desc = format!("Option {}/{}: {}", i + 1, choices.len(), opt.hint.clone().unwrap_or_default());
        if send_choice(connection, session_id, &opt_title, desc, options, cancel.clone())
            .await
            .as_deref()
            == Some("yes")
        {
            selected.push(opt.value.clone());
        }
    }
    if selected.is_empty() && step.required {
        return None;
    }
    Some(serde_json::to_string(&selected).unwrap_or_default())
}

async fn ask_text_fallback(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    title: &str,
    step: &UserQuestionStep,
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> Option<String> {
    let mut options = Vec::new();
    if let Some(default) = step.default.as_ref().filter(|d| !d.is_empty()) {
        options.push(PermissionOption::new(
            default.clone(),
            format!("Use default ({default})"),
            PermissionOptionKind::AllowOnce,
        ));
    }
    options.push(PermissionOption::new(
        "ok",
        "Continue (reply in the next message if you need to type)",
        PermissionOptionKind::AllowOnce,
    ));
    if !step.required {
        options.push(PermissionOption::new("skip", "Skip", PermissionOptionKind::RejectOnce));
    }
    let desc = format!(
        "{}\nACP clients can only pick options here; type a follow-up message for free text.",
        step_description(step)
    );
    match send_choice(connection, session_id, title, desc, options, cancel).await {
        Some(id) if id == "ok" => Some(step.default.clone().unwrap_or_default()),
        Some(id) if id != "skip" => Some(id),
        Some(_) => Some(String::new()),
        None => None,
    }
}

async fn send_choice(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    title: &str,
    description: String,
    options: Vec<PermissionOption>,
    cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> Option<String> {
    if options.is_empty() {
        return None;
    }
    let request =
        RequestPermissionRequest::new(session_id.clone(), title.to_string(), options).description(description);
    send_permission(connection, request, cancel).await
}

fn is_confirm(step: &UserQuestionStep) -> bool {
    step.options.is_none()
        && step
            .default
            .as_ref()
            .is_some_and(|value| value == "true" || value == "false")
}

fn step_description(step: &UserQuestionStep) -> String {
    let mut parts = Vec::new();
    if let Some(tab) = &step.tab_label {
        parts.push(tab.clone());
    }
    if step.allow_custom {
        parts.push(format!("Custom answers: {}", step.custom_label));
    }
    if let Some(min) = step.min_length {
        parts.push(format!("min length {min}"));
    }
    parts.join(" · ")
}

fn finalize_answers(steps: &[UserQuestionStep], collected: &BTreeMap<String, String>) -> String {
    if steps.len() == 1 {
        let step = &steps[0];
        let answer = collected.get(&step.id).cloned().unwrap_or_default();
        if !step.allow_multiple {
            return answer;
        }
    }
    serde_json::to_string(collected).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, multiple: bool) -> UserQuestionStep {
        UserQuestionStep {
            id: id.into(),
            question: "Q".into(),
            options: None,
            allow_multiple: multiple,
            allow_custom: false,
            custom_label: "Other".into(),
            default: None,
            required: true,
            min_length: None,
            pattern: None,
            tab_label: None,
        }
    }

    #[test]
    fn single_step_returns_plain_answer() {
        let steps = vec![step("a", false)];
        let mut collected = BTreeMap::new();
        collected.insert("a".into(), "yes".into());
        assert_eq!(finalize_answers(&steps, &collected), "yes");
    }

    #[test]
    fn multi_step_returns_json() {
        let steps = vec![step("a", false), step("b", false)];
        let mut collected = BTreeMap::new();
        collected.insert("a".into(), "1".into());
        collected.insert("b".into(), "2".into());
        let json = finalize_answers(&steps, &collected);
        assert!(json.contains("\"a\":\"1\""));
        assert!(json.contains("\"b\":\"2\""));
    }
}
