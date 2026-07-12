//! Personal-mode onboarding config (`~/.owly/onboarding.json`).

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::credentials;
use crate::mode::{RunMode, owly_home_dir};

pub const ONBOARDING_FILE: &str = "onboarding.json";
pub const PERSONAL_INSTRUCTIONS_FILE: &str = "INSTRUCTIONS.md";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnboardingSourceScheduleConfig {
    pub description: String,
    pub expression: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_agent_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<String>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnboardingSourceConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_config: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingestion_goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<OnboardingSourceScheduleConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnboardingSourceInstanceConfig {
    pub connector_id: String,
    pub id: String,
    #[serde(flatten)]
    pub source: OnboardingSourceConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnboardingConfig {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingestion_schedule: Option<OnboardingSourceScheduleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_name: Option<String>,
    #[serde(default)]
    pub source_instances: Vec<OnboardingSourceInstanceConfig>,
    #[serde(default)]
    pub sources: std::collections::HashMap<String, OnboardingSourceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_name: Option<String>,
    /// Transient: loaded from `INSTRUCTIONS.md`, not persisted in JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_goal: Option<String>,
}

impl Default for OnboardingConfig {
    fn default() -> Self {
        Self {
            version: 1,
            completed_at: None,
            ingestion_schedule: None,
            mode_id: None,
            mode_name: None,
            source_instances: Vec::new(),
            sources: std::collections::HashMap::new(),
            template_id: None,
            template_name: None,
            wiki_goal: None,
        }
    }
}

pub fn onboarding_path() -> PathBuf {
    owly_home_dir().join(ONBOARDING_FILE)
}

pub fn personal_instructions_path() -> PathBuf {
    owly_home_dir().join(PERSONAL_INSTRUCTIONS_FILE)
}

pub fn read_onboarding_config() -> Result<OnboardingConfig> {
    let path = onboarding_path();
    if !path.exists() {
        let wiki_goal = read_personal_instructions()?;
        return Ok(OnboardingConfig {
            wiki_goal,
            ..OnboardingConfig::default()
        });
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut config: OnboardingConfig = serde_json::from_str(&raw).context("parse onboarding.json")?;
    config.wiki_goal = read_personal_instructions()?;
    Ok(config)
}

pub fn save_onboarding_config(config: &OnboardingConfig) -> Result<()> {
    crate::mode::ensure_personal_home()?;
    let path = onboarding_path();
    let wiki_goal = config.wiki_goal.clone();
    let mut persistable = config.clone();
    persistable.wiki_goal = None;
    let body = serde_json::to_string_pretty(&persistable)?;
    std::fs::write(&path, format!("{body}\n")).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    if let Some(goal) = wiki_goal.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        save_personal_instructions(goal)?;
    }
    credentials::secure_env_dir()?;
    Ok(())
}

pub fn read_personal_instructions() -> Result<Option<String>> {
    let path = personal_instructions_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

pub fn save_personal_instructions(wiki_goal: &str) -> Result<()> {
    let trimmed = wiki_goal.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Wiki brief cannot be empty.");
    }
    crate::mode::ensure_personal_home()?;
    let path = personal_instructions_path();
    std::fs::write(&path, format!("{trimmed}\n")).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Mark personal onboarding complete for the given mode id.
pub fn complete_personal_onboarding(wiki_goal: &str) -> Result<()> {
    let mut config = read_onboarding_config().unwrap_or_default();
    config.mode_id = Some(RunMode::Personal.as_str().to_string());
    config.mode_name = Some("Personal".to_string());
    config.template_id = Some("personal".to_string());
    config.template_name = Some("Personal brain".to_string());
    config.completed_at = Some(chrono::Utc::now().to_rfc3339());
    config.wiki_goal = Some(wiki_goal.trim().to_string());
    save_onboarding_config(&config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_version_is_one() {
        assert_eq!(OnboardingConfig::default().version, 1);
    }
}
