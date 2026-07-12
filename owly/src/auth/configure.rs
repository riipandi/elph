use anyhow::Result;
use chrono::Utc;

use crate::connectors::{self, default_connector_config, io};
use crate::onboarding_config::{self, OnboardingSourceConfig, OnboardingSourceInstanceConfig, save_onboarding_config};

#[derive(Debug, Clone)]
pub struct ConfigureResult {
    pub config_path: std::path::PathBuf,
    pub status: &'static str,
    pub next_steps: Vec<String>,
}

pub fn configure_connector(id: connectors::ConnectorId, force: bool) -> Result<ConfigureResult> {
    let path = io::config_path(id);
    if path.exists() && !force {
        return Ok(ConfigureResult {
            config_path: path,
            status: "exists",
            next_steps: next_steps(id, false),
        });
    }

    let config = default_connector_config(id);
    io::write_connector_config(id, &config)?;
    register_source_instance(id)?;

    Ok(ConfigureResult {
        config_path: path,
        status: if force { "updated" } else { "created" },
        next_steps: next_steps(id, true),
    })
}

fn register_source_instance(id: connectors::ConnectorId) -> Result<()> {
    let mut config = onboarding_config::read_onboarding_config().unwrap_or_default();
    let instance_id = id.as_str().to_string();
    if config.source_instances.iter().any(|s| s.id == instance_id) {
        return Ok(());
    }
    config.source_instances.push(OnboardingSourceInstanceConfig {
        connector_id: id.as_str().to_string(),
        id: instance_id,
        source: OnboardingSourceConfig {
            connected_at: Some(Utc::now().to_rfc3339()),
            ..OnboardingSourceConfig::default()
        },
        name: Some(id.display_name().to_string()),
    });
    config.sources.insert(
        id.as_str().to_string(),
        OnboardingSourceConfig {
            connected_at: Some(Utc::now().to_rfc3339()),
            ..OnboardingSourceConfig::default()
        },
    );
    save_onboarding_config(&config)
}

fn next_steps(id: connectors::ConnectorId, created: bool) -> Vec<String> {
    let path = io::config_path(id);
    let mut steps = vec![format!("Edit {}", path.display())];
    match id {
        connectors::ConnectorId::GitRepo => {
            steps.push("Add local repositories under \"repos\".".into());
        }
        connectors::ConnectorId::WebSearch => {
            steps.push(format!("Set {} and add search queries.", connectors::TAVILY_API_KEY_ENV));
        }
        connectors::ConnectorId::HackerNews => {
            steps.push("Set enabled=true and optional queries.".into());
        }
        connectors::ConnectorId::X => {
            steps.push(format!(
                "Set {} in ~/.owly/.env and set enabled=true.",
                connectors::X_ACCESS_TOKEN_ENV
            ));
        }
    }
    if created {
        steps.push("Run `owly ingest all` to refresh the personal wiki.".into());
    }
    steps
}
