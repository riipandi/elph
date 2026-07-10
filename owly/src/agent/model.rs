use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use elph_ai::{builtin_models, get_builtin_model};

use crate::config::Config;
use crate::constants::provider_config;
use crate::ui_events::AgentUiEvent;

use super::listeners::emit_ui;

pub(super) fn progress_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

pub(super) async fn resolve_model_and_auth(
    config: &Config,
    ui_events: &Option<mpsc::UnboundedSender<AgentUiEvent>>,
) -> Result<(elph_ai::Model, Arc<elph_ai::Models>, elph_agent::StreamFn)> {
    let model = get_builtin_model(&config.provider, &config.model_id)
        .or_else(|| {
            let parts: Vec<&str> = config.model_id.splitn(2, '/').collect();
            if parts.len() == 2 {
                get_builtin_model(parts[0], parts[1])
            } else {
                None
            }
        })
        .or_else(|| get_builtin_model(&config.provider, &config.model_id))
        .context(format!(
            "Model not found: {}/{}. Use provider/model format (e.g., opencode/big-pickle)",
            config.provider, config.model_id
        ))?;

    let spinner_active = ui_events.is_none();
    let setup = spinner_active.then(|| progress_spinner("Resolving auth..."));
    if ui_events.is_some() {
        emit_ui(ui_events, AgentUiEvent::Status("Resolving auth...".into()));
    }
    let models = builtin_models(None);
    let auth = models.get_auth(&model).await?;
    if let Some(pb) = setup {
        pb.finish_and_clear();
    }

    if auth.is_none() {
        let provider_cfg =
            provider_config(&config.provider).context(format!("Unknown provider: {}", config.provider))?;
        anyhow::bail!(
            "No API key configured for {}. Set {} environment variable.",
            provider_cfg.label,
            provider_cfg.api_key_env_key
        );
    }

    let models: Arc<elph_ai::Models> = models.into_arc();
    let stream_fn: elph_agent::StreamFn = {
        let models = models.clone();
        Arc::new(move |m, ctx, opts| models.stream_simple(m, ctx, opts))
    };

    Ok((model, models, stream_fn))
}
