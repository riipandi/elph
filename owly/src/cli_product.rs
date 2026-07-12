//! Product subcommands: auth, ingest, cron (not ngrok).

use anyhow::{Context, Result, bail};

use crate::auth::{self, is_unsupported_auth_provider};
use crate::connectors::is_connector_id;
use crate::constants::{is_valid_model_id, normalize_model_id};
use crate::ingestion::{self, IngestionTarget};

#[derive(Debug)]
pub enum ProductCommand {
    Help,
    AuthList,
    AuthConfigure {
        provider: String,
        force: bool,
    },
    Ingest {
        target: IngestionTarget,
        model_id: Option<String>,
        print: bool,
    },
    Cron {
        action: CronAction,
        target: Option<String>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum CronAction {
    List,
    Pause,
    Resume,
    Delete,
}

/// Parse `auth` / `ingest` / `cron` when the first argv token matches.
pub fn parse_product_command(parts: &[String]) -> Result<Option<ProductCommand>> {
    let Some(head) = parts.first() else {
        return Ok(None);
    };
    match head.as_str() {
        "auth" => parse_auth(&parts[1..]),
        "ingest" => parse_ingest(&parts[1..]),
        "cron" => parse_cron(&parts[1..]),
        "ngrok" => bail!("OpenWiki `ngrok` is not supported in Owly."),
        _ => Ok(None),
    }
}

pub async fn execute(cmd: ProductCommand, stream: bool, verbose: bool) -> Result<()> {
    match cmd {
        ProductCommand::Help => {
            println!("{}", crate::help_content::get_help_text());
            Ok(())
        }
        ProductCommand::AuthList => auth::run_auth_list(),
        ProductCommand::AuthConfigure { provider, force } => {
            let result = auth::run_auth_configure(&provider, force)?;
            println!(
                "\x1b[32m✓\x1b[0m Connector config {} at {}",
                result.status,
                result.config_path.display()
            );
            for step in result.next_steps {
                println!("  - {step}");
            }
            Ok(())
        }
        ProductCommand::Ingest {
            target,
            model_id,
            print,
        } => ingestion::run_ingestion(target, model_id.as_deref(), print, stream, verbose).await,
        ProductCommand::Cron { action, target } => match action {
            CronAction::List => crate::schedules::cron_list(),
            CronAction::Pause => {
                let t = target.as_deref().unwrap_or("all");
                crate::schedules::cron_pause(t)
            }
            CronAction::Resume => {
                let t = target.as_deref().unwrap_or("all");
                crate::schedules::cron_resume(t)
            }
            CronAction::Delete => {
                let t = target.as_deref().context("Usage: owly cron delete <source|all>")?;
                crate::schedules::cron_delete(t)
            }
        },
    }
}

fn parse_auth(parts: &[String]) -> Result<Option<ProductCommand>> {
    if parts.first().is_some_and(|p| p == "configure") {
        let provider = parts.get(1).cloned().unwrap_or_default();
        if provider.is_empty() {
            bail!("Usage: owly auth configure <provider> [--force]");
        }
        let force = parts.iter().any(|p| p == "--force");
        return Ok(Some(ProductCommand::AuthConfigure { provider, force }));
    }
    if parts.first().is_some_and(|p| p == "tools") {
        let provider = parts.get(1).map(String::as_str).unwrap_or("");
        bail!("OpenWiki `auth tools` is not supported in Owly (notion MCP tools excluded). Provider: {provider}");
    }
    let provider = parts.first().cloned().unwrap_or_else(|| "list".into());
    if provider == "list" {
        return Ok(Some(ProductCommand::AuthList));
    }
    if is_unsupported_auth_provider(&provider) {
        bail!(
            "OpenWiki auth for `{provider}` is not supported in Owly. \
             Use `owly auth configure <connector>` for git-repo, web-search, or hackernews."
        );
    }
    if auth::is_configure_connector(&provider) {
        bail!("Use `owly auth configure {provider}` to create connector config (OAuth is not supported).");
    }
    bail!("Unknown auth command: {provider}\n\n{}", auth::format_auth_provider_list());
}

fn parse_ingest(parts: &[String]) -> Result<Option<ProductCommand>> {
    let target_raw = parts.first().cloned().unwrap_or_else(|| "all".into());
    let target = ingestion::parse_ingestion_target(&target_raw)
        .ok_or_else(|| anyhow::anyhow!("Usage: owly ingest <source|source-instance|all> [--print] [--modelId <id>]"))?;

    let mut model_id = None;
    let mut print = false;
    let mut i = 1;
    while i < parts.len() {
        match parts[i].as_str() {
            "-p" | "--print" => print = true,
            "--modelId" | "--model-id" | "--model" => {
                let raw = parts.get(i + 1).context("--modelId requires a value")?;
                let parsed = normalize_model_id(raw);
                if !is_valid_model_id(&parsed) {
                    bail!("Invalid model ID: {raw}");
                }
                model_id = Some(parsed);
                i += 1;
            }
            flag if flag.starts_with("--modelId=") || flag.starts_with("--model=") => {
                let raw = flag.split('=').nth(1).unwrap_or("");
                let parsed = normalize_model_id(raw);
                if !is_valid_model_id(&parsed) {
                    bail!("Invalid model ID: {raw}");
                }
                model_id = Some(parsed);
            }
            other => bail!("Unknown option for ingest: {other}"),
        }
        i += 1;
    }

    Ok(Some(ProductCommand::Ingest {
        target,
        model_id,
        print,
    }))
}

fn parse_cron(parts: &[String]) -> Result<Option<ProductCommand>> {
    let action = match parts.first().map(String::as_str) {
        Some("list") if parts.len() == 1 => CronAction::List,
        Some("pause") => CronAction::Pause,
        Some("resume") => CronAction::Resume,
        Some("delete") => CronAction::Delete,
        _ => bail!("Usage: owly cron list | pause <source|all> | resume <source|all> | delete <source|all>"),
    };
    let target = parts.get(1).cloned();
    if matches!(action, CronAction::List) {
        return Ok(Some(ProductCommand::Cron { action, target: None }));
    }
    if target.is_none() {
        bail!("Usage: owly cron {:?} <source|all>", format!("{action:?}").to_lowercase());
    }
    let t = target.as_deref().unwrap();
    if t != "all" && !is_connector_id(t) && !crate::connectors::is_safe_source_instance_id(t) {
        bail!("Unknown cron target: {t}");
    }
    Ok(Some(ProductCommand::Cron { action, target }))
}
