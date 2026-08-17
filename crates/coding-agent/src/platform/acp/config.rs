//! Session config options (mode / model / thought_level).

use agent_client_protocol::schema::v2::{
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigOptionValue, SessionConfigSelectOption,
};
use anyhow::Context;

use crate::agent::CodingAgentSession;
use crate::platform::acp::state::current_mode;
use crate::types::{AgentMode, ThinkingLevel};

pub fn config_options(session: &CodingAgentSession) -> Vec<SessionConfigOption> {
    let mode = current_mode(session);
    let mode_option = SessionConfigOption::select(
        "mode",
        "Session Mode",
        mode.footer_label(),
        vec![
            SessionConfigSelectOption::new("ask", "Ask"),
            SessionConfigSelectOption::new("plan", "Plan"),
            SessionConfigSelectOption::new("build", "Build"),
            SessionConfigSelectOption::new("brave", "Brave"),
        ],
    )
    .category(SessionConfigOptionCategory::Mode)
    .description("Controls tool permission and planning behavior");

    let model_id = format!("{}/{}", session.model_provider(), session.model_id());
    let model_option = SessionConfigOption::select(
        "model",
        "Model",
        model_id,
        vec![SessionConfigSelectOption::new(
            format!("{}/{}", session.model_provider(), session.model_id()),
            session.model_display(),
        )],
    )
    .category(SessionConfigOptionCategory::Model);

    vec![mode_option, model_option]
}

pub async fn set_config_option(
    session: &CodingAgentSession,
    config_id: &str,
    value: &SessionConfigOptionValue,
) -> anyhow::Result<Vec<SessionConfigOption>> {
    match config_id {
        "mode" => {
            let SessionConfigOptionValue::Id { value } = value else {
                anyhow::bail!("mode expects type id");
            };
            let mode = parse_mode(value.0.as_ref()).context("unknown mode")?;
            session.set_agent_mode(mode).await?;
        }
        "model" => {
            let SessionConfigOptionValue::Id { value } = value else {
                anyhow::bail!("model expects type id");
            };
            session.set_model_from_value(value.0.as_ref()).await?;
        }
        "thought_level" => {
            let SessionConfigOptionValue::Id { value } = value else {
                anyhow::bail!("thought_level expects type id");
            };
            let level = parse_thought(value.0.as_ref()).context("unknown thought level")?;
            session.set_thinking_level(level).await?;
        }
        other => anyhow::bail!("unknown config option: {other}"),
    }
    Ok(config_options(session))
}

fn parse_mode(value: &str) -> Option<AgentMode> {
    match value {
        "ask" => Some(AgentMode::Ask),
        "plan" => Some(AgentMode::Plan),
        "build" => Some(AgentMode::Build),
        "brave" => Some(AgentMode::Brave),
        _ => None,
    }
}

fn parse_thought(value: &str) -> Option<ThinkingLevel> {
    match value {
        "off" => Some(ThinkingLevel::Off),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::Xhigh),
        "max" => Some(ThinkingLevel::Max),
        _ => None,
    }
}
