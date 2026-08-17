//! Session config options (mode / model / thought_level), following pi-acp.

use agent_client_protocol::schema::v2::{
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigOptionValue, SessionConfigSelectOption,
};
use anyhow::Context;
use elph_ai::get_builtin_model;

use crate::agent::{CodingAgentSession, from_agent_thinking, list_model_select_items};
use crate::platform::Settings;
use crate::platform::acp::state::current_mode;
use crate::types::{AgentMode, ThinkingLevel};

pub async fn config_options(session: &CodingAgentSession, settings: &Settings) -> Vec<SessionConfigOption> {
    let snapshot = session_config(session, settings).await;
    snapshot.into_iter().map(to_v2_option).collect()
}

pub async fn set_config_option(
    session: &CodingAgentSession,
    settings: &Settings,
    config_id: &str,
    value: &SessionConfigOptionValue,
) -> anyhow::Result<Vec<SessionConfigOption>> {
    let raw = match value {
        SessionConfigOptionValue::Id { value } => value.0.as_ref().to_string(),
        _ => anyhow::bail!("{config_id} expects type id"),
    };
    apply_config_value(session, config_id, &raw).await?;
    Ok(config_options(session, settings).await)
}

pub async fn apply_config_value(session: &CodingAgentSession, config_id: &str, raw: &str) -> anyhow::Result<()> {
    match config_id {
        "mode" => {
            let mode = parse_mode(raw).context("unknown mode")?;
            session.set_agent_mode(mode).await?;
        }
        "model" => {
            session.set_model_from_value(raw).await?;
        }
        "thought_level" => {
            let level = parse_thought(raw).context("unknown thought level")?;
            session.set_thinking_level(level).await?;
        }
        other => anyhow::bail!("unknown config option: {other}"),
    }
    Ok(())
}

pub struct ConfigChoice {
    pub id: String,
    pub name: String,
}

pub struct ConfigSelect {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: ConfigCategory,
    pub current: String,
    pub options: Vec<ConfigChoice>,
}

#[derive(Clone, Copy)]
pub enum ConfigCategory {
    Mode,
    Model,
    ThoughtLevel,
}

pub async fn session_config(session: &CodingAgentSession, settings: &Settings) -> Vec<ConfigSelect> {
    let mode = current_mode(session);
    let mode_option = ConfigSelect {
        id: "mode",
        name: "Session Mode",
        description: "Controls tool permission and planning behavior",
        category: ConfigCategory::Mode,
        current: mode.footer_label().to_string(),
        options: vec![
            choice("ask", "Ask"),
            choice("plan", "Plan"),
            choice("build", "Build"),
            choice("brave", "Brave"),
        ],
    };

    let current_model = format!("{}/{}", session.model_provider(), session.model_id());
    let model_option = ConfigSelect {
        id: "model",
        name: "Model",
        description: "Select the model for this session",
        category: ConfigCategory::Model,
        current: current_model.clone(),
        options: advertised_models(session, settings),
    };

    let thought = current_thought(session).await;
    let thought_option = ConfigSelect {
        id: "thought_level",
        name: "Thinking",
        description: "Set the reasoning effort for this session",
        category: ConfigCategory::ThoughtLevel,
        current: thought.label().to_string(),
        options: advertised_thought_levels(session, thought),
    };

    vec![model_option, thought_option, mode_option]
}

async fn current_thought(session: &CodingAgentSession) -> ThinkingLevel {
    from_agent_thinking(session.harness().get_thinking_level().await)
}

fn advertised_models(session: &CodingAgentSession, settings: &Settings) -> Vec<ConfigChoice> {
    let current = format!("{}/{}", session.model_provider(), session.model_id());
    let mut options = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (id, name) in session.list_acp_models() {
        if seen.insert(id.clone()) {
            options.push(ConfigChoice { id, name });
        }
    }
    for item in list_model_select_items() {
        if seen.insert(item.value.clone()) {
            options.push(ConfigChoice {
                id: item.value,
                name: item.label,
            });
        }
    }
    for value in &settings.models.scoped_models {
        if seen.insert(value.clone()) {
            options.push(model_choice(value));
        }
    }
    if seen.insert(current.clone()) {
        options.insert(
            0,
            ConfigChoice {
                id: current.clone(),
                name: session.model_display(),
            },
        );
    } else if let Some(pos) = options.iter().position(|c| c.id == current) {
        let current_choice = options.remove(pos);
        options.insert(0, current_choice);
    }
    options
}

fn model_choice(value: &str) -> ConfigChoice {
    let name = get_builtin_model_label(value).unwrap_or_else(|| value.to_string());
    ConfigChoice {
        id: value.to_string(),
        name,
    }
}

fn get_builtin_model_label(value: &str) -> Option<String> {
    let (provider, id) = value.split_once('/')?;
    get_builtin_model(provider, id).map(|m| format!("{provider}/{}", m.name))
}

fn advertised_thought_levels(session: &CodingAgentSession, current: ThinkingLevel) -> Vec<ConfigChoice> {
    let model = get_builtin_model(&session.model_provider(), &session.model_id());
    let mut levels = match model.as_ref() {
        Some(model) => ThinkingLevel::cycle_for_model(model),
        None => vec![
            ThinkingLevel::Off,
            ThinkingLevel::Minimal,
            ThinkingLevel::Low,
            ThinkingLevel::Medium,
            ThinkingLevel::High,
            ThinkingLevel::Xhigh,
            ThinkingLevel::Max,
        ],
    };
    if !levels.contains(&current) {
        levels.insert(1.min(levels.len()), current);
    }
    levels
        .into_iter()
        .map(|level| ConfigChoice {
            id: level.label().to_string(),
            name: format!("Thinking: {}", level.label()),
        })
        .collect()
}

fn to_v2_option(select: ConfigSelect) -> SessionConfigOption {
    let options: Vec<SessionConfigSelectOption> = select
        .options
        .into_iter()
        .map(|c| SessionConfigSelectOption::new(c.id, c.name))
        .collect();
    SessionConfigOption::select(select.id, select.name, select.current, options)
        .category(match select.category {
            ConfigCategory::Mode => SessionConfigOptionCategory::Mode,
            ConfigCategory::Model => SessionConfigOptionCategory::Model,
            ConfigCategory::ThoughtLevel => SessionConfigOptionCategory::ThoughtLevel,
        })
        .description(select.description)
}

fn choice(id: &str, name: &str) -> ConfigChoice {
    ConfigChoice {
        id: id.to_string(),
        name: name.to_string(),
    }
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

pub fn parse_thought(value: &str) -> Option<ThinkingLevel> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_thought_levels() {
        assert_eq!(parse_thought("high"), Some(ThinkingLevel::High));
        assert_eq!(parse_thought("xhigh"), Some(ThinkingLevel::Xhigh));
        assert_eq!(parse_thought("nope"), None);
    }
}
