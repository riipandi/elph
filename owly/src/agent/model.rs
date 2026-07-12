use anyhow::{Context, Result};
use std::sync::Arc;

use elph_ai::{SimpleStreamOptions, StreamOptions};

use crate::config::Config;
use crate::constants::{
    provider_config, provider_is_configured, provider_oauth_capable, provider_oauth_only,
    resolve_provider_retry_attempts,
};
use crate::interactive;

use super::shared_models::shared_models;

pub(super) async fn resolve_model_and_auth(
    config: &Config,
) -> Result<(elph_ai::Model, Arc<elph_ai::Models>, elph_agent::StreamFn)> {
    let models = shared_models();

    let model = models
        .get_model(&config.provider, &config.model_id)
        .or_else(|| {
            let parts: Vec<&str> = config.model_id.splitn(2, '/').collect();
            if parts.len() == 2 {
                models.get_model(parts[0], parts[1])
            } else {
                None
            }
        })
        .context(format!(
            "Model not found: {}/{}. Use provider/model format (e.g., opencode/big-pickle)",
            config.provider, config.model_id
        ))?;

    let setup = interactive::progress_spinner("Resolving auth...");
    let auth = models.get_auth(&model).await?;
    setup.finish_and_clear();

    if auth.is_none() {
        let provider_cfg =
            provider_config(&config.provider).context(format!("Unknown provider: {}", config.provider))?;
        if provider_oauth_only(&config.provider) {
            anyhow::bail!(
                "No OAuth credentials for {}. Run Owly in an interactive terminal to complete setup.",
                provider_cfg.label
            );
        }
        if provider_oauth_capable(&config.provider) {
            anyhow::bail!(
                "No credentials for {}. Run setup in an interactive terminal or set {}.",
                provider_cfg.label,
                provider_cfg.api_key_env_key
            );
        }
        anyhow::bail!(
            "No API key configured for {}. Run setup in an interactive terminal or set {}.",
            provider_cfg.label,
            provider_cfg.api_key_env_key
        );
    }

    if !provider_is_configured(&config.provider) {
        let provider_cfg =
            provider_config(&config.provider).context(format!("Unknown provider: {}", config.provider))?;
        if provider_oauth_only(&config.provider) {
            anyhow::bail!(
                "OAuth session missing for {}. Run setup in an interactive terminal.",
                provider_cfg.label
            );
        }
        if provider_oauth_capable(&config.provider) {
            anyhow::bail!(
                "Provider {} is not fully configured. Run setup in an interactive terminal or set {}.",
                provider_cfg.label,
                provider_cfg.api_key_env_key
            );
        }
        anyhow::bail!(
            "Provider {} is not fully configured. Run setup in an interactive terminal or set {}.",
            provider_cfg.label,
            provider_cfg.api_key_env_key
        );
    }

    let max_retries = resolve_provider_retry_attempts().map_err(|msg| anyhow::anyhow!(msg))?;

    let stream_fn: elph_agent::StreamFn = {
        let models = models.clone();
        Arc::new(move |m, ctx, opts| {
            let opts = match opts {
                Some(mut simple) => {
                    simple.base.max_retries = Some(max_retries);
                    Some(simple)
                }
                None => Some(SimpleStreamOptions {
                    base: StreamOptions {
                        max_retries: Some(max_retries),
                        ..Default::default()
                    },
                    reasoning: None,
                    thinking_budgets: None,
                }),
            };
            models.stream_simple(m, ctx, opts)
        })
    };

    Ok((model, models, stream_fn))
}
