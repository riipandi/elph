//! Connector ingestion + personal wiki update runs.

use anyhow::{Context, Result, bail};

use crate::config::Config;
use crate::connectors::{
    self, ConnectorId, ConnectorIngestOptions, ConnectorIngestResult, is_connector_id, is_safe_source_instance_id,
};
use crate::mode::WikiContext;
use crate::onboarding_config::{self, OnboardingSourceInstanceConfig};

#[derive(Debug, Clone)]
pub enum IngestionTarget {
    All,
    Connector(ConnectorId),
    SourceInstance(String),
}

pub fn parse_ingestion_target(value: &str) -> Option<IngestionTarget> {
    if value == "all" {
        return Some(IngestionTarget::All);
    }
    if let Some(id) = ConnectorId::parse(value) {
        return Some(IngestionTarget::Connector(id));
    }
    if is_safe_source_instance_id(value) {
        return Some(IngestionTarget::SourceInstance(value.to_string()));
    }
    None
}

pub async fn run_ingestion(
    target: IngestionTarget,
    model_override: Option<&str>,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    let repo_cwd = std::env::current_dir()?;
    let ctx = WikiContext::personal(&repo_cwd);
    ctx.ensure_layout()?;
    crate::credentials::load_env()?;
    let config = Config::resolve(model_override, &ctx.repo_cwd)?;
    crate::env::setup_environment(&config)?;

    let onboarding = onboarding_config::read_onboarding_config().unwrap_or_default();
    let instances = resolve_instances(&target, &onboarding.source_instances);
    if instances.is_empty() {
        bail!(
            "No configured ingestion source matched {}. Run `owly auth configure <connector>` first.",
            format_target(&target)
        );
    }

    for instance in instances {
        println!("\n>_ Owly ingest {} ({})", instance.connector_id, instance.id);
        let connector_id = ConnectorId::parse(&instance.connector_id)
            .with_context(|| format!("unknown connector {}", instance.connector_id))?;
        let runtime = connectors::get(connector_id)
            .with_context(|| format!("connector {} not registered", instance.connector_id))?;

        let pull = (runtime.ingest)(ConnectorIngestOptions {
            instance_id: Some(instance.id.clone()),
            window_hours: Some(24),
            limit: None,
            connector_config: instance.source.connector_config.clone(),
        })?;

        println!("  pull: {}", pull.message);
        if !pull.warnings.is_empty() {
            for w in &pull.warnings {
                println!("  warn: {w}");
            }
        }

        if should_skip_wiki_update(&pull) {
            println!("  wiki: skipped (no new evidence)");
            continue;
        }

        run_wiki_update_for_source(&ctx, &config, instance, &pull, print_mode, stream, verbose).await?;
    }

    Ok(())
}

async fn run_wiki_update_for_source(
    ctx: &WikiContext,
    config: &Config,
    instance: &OnboardingSourceInstanceConfig,
    pull: &ConnectorIngestResult,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    let user_message = create_source_update_message(instance, pull);
    let (result, snapshot) =
        crate::commands::doc_run::run_update_agent(config, ctx, Some(&user_message), print_mode, stream, verbose)
            .await?;

    crate::commands::doc_run::apply_doc_run_result(ctx, config, "update", &result, &snapshot)?;
    if print_mode && !result.completion_message.is_empty() {
        println!("{}", result.completion_message);
    } else {
        println!("  wiki: update complete");
    }
    Ok(())
}

fn resolve_instances<'a>(
    target: &IngestionTarget,
    instances: &'a [OnboardingSourceInstanceConfig],
) -> Vec<&'a OnboardingSourceInstanceConfig> {
    instances
        .iter()
        .filter(|s| s.source.connected_at.is_some() && is_connector_id(&s.connector_id))
        .filter(|s| match target {
            IngestionTarget::All => true,
            IngestionTarget::Connector(id) => s.connector_id == id.as_str(),
            IngestionTarget::SourceInstance(id) => s.id == *id,
        })
        .collect()
}

fn should_skip_wiki_update(pull: &ConnectorIngestResult) -> bool {
    if pull.raw_files.is_empty() {
        return true;
    }
    matches!(pull.status, connectors::IngestStatus::Skipped | connectors::IngestStatus::Error)
}

fn format_target(target: &IngestionTarget) -> String {
    match target {
        IngestionTarget::All => "all".into(),
        IngestionTarget::Connector(id) => id.as_str().into(),
        IngestionTarget::SourceInstance(id) => id.clone(),
    }
}

fn create_source_update_message(instance: &OnboardingSourceInstanceConfig, pull: &ConnectorIngestResult) -> String {
    format!(
        "Update the personal wiki from newly ingested {} evidence.\n\n\
         Source instance: {}\n\
         Connector pull: {}\n\
         Raw files:\n{}\n\n\
         Inspect only the raw files listed above. Keep edits surgical and route synthesis into canonical personal wiki pages.",
        instance.connector_id,
        instance.id,
        pull.message,
        pull.raw_files.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::{ConnectorId, ConnectorIngestResult, IngestStatus};

    #[test]
    fn skip_wiki_update_when_pull_skipped_without_files() {
        let pull = ConnectorIngestResult {
            connector_id: ConnectorId::GitRepo,
            message: "no repos".into(),
            run_id: "run-1".into(),
            status: IngestStatus::Skipped,
            raw_files: Vec::new(),
            warnings: Vec::new(),
        };
        assert!(should_skip_wiki_update(&pull));
    }

    #[test]
    fn run_wiki_update_when_pull_has_raw_files() {
        let pull = ConnectorIngestResult {
            connector_id: ConnectorId::GitRepo,
            message: "ok".into(),
            run_id: "run-2".into(),
            status: IngestStatus::Success,
            raw_files: vec!["/tmp/manifest.json".into()],
            warnings: Vec::new(),
        };
        assert!(!should_skip_wiki_update(&pull));
    }
}
