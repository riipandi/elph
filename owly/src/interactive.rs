//! Terminal user feedback: dialoguer prompts and indicatif progress.

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::config::Config;
use crate::onboarding;
use crate::startup::stdin_is_tty;

/// Spinner used for auth resolution, agent thinking, and OAuth progress.
pub fn progress_spinner(message: impl Into<String>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.into());
    pb.set_draw_target(ProgressDrawTarget::stderr());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Run the dialoguer onboarding wizard when credentials are missing (TTY only).
pub async fn ensure_provider_setup(config: Config) -> Result<Config> {
    if !onboarding::needs_setup(&config) {
        return Ok(config);
    }
    if !stdin_is_tty() {
        anyhow::bail!(
            "Provider credentials are not configured. Run Owly in an interactive terminal to complete setup, \
             or set API keys / OAuth tokens in ~/.owly/.env."
        );
    }

    tokio::task::spawn_blocking(move || {
        let mut config = config;
        onboarding::run_wizard(&mut config).context("credential setup cancelled")?;
        Ok(config)
    })
    .await
    .context("credential setup interrupted")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::path::PathBuf;

    #[test]
    fn progress_spinner_finishes_cleanly() {
        let pb = progress_spinner("Working");
        pb.finish_and_clear();
    }

    #[test]
    fn ensure_setup_skips_when_configured() {
        let config = Config {
            provider: "opencode".to_string(),
            model_id: "big-pickle".to_string(),
            cwd: PathBuf::from("/tmp"),
        };
        if onboarding::needs_setup(&config) {
            return;
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        let out = rt.block_on(ensure_provider_setup(config)).unwrap();
        assert_eq!(out.provider, "opencode");
    }
}
