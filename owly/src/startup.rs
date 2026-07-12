//! Non-interactive startup validation.

use std::io::IsTerminal;

use crate::commands::Command;
use crate::constants::provider_is_configured;
use crate::credentials;
use crate::metadata;
use crate::mode::WikiContext;

/// Validate that non-interactive invocations have credentials and input.
pub fn validate_non_interactive(command: &Command, ctx: &WikiContext) -> anyhow::Result<()> {
    if std::io::stdin().is_terminal() {
        return Ok(());
    }

    match command {
        Command::Chat { message: None } => {
            anyhow::bail!("Pass a message, --init, or --update for non-interactive runs.");
        }
        Command::Init | Command::Update => {}
        Command::Chat { message: Some(msg) } if msg.trim().is_empty() => {
            anyhow::bail!("User message cannot be empty.");
        }
        Command::Chat { message: Some(_) } => {}
    }

    if can_skip_credentials_for_clean_update(command, ctx) {
        return Ok(());
    }

    if !credentials::has_provider_api_key_for_any() && !credentials::has_any_stored_oauth() {
        anyhow::bail!(
            "Provider credentials are required for non-interactive runs. Configure ~/.owly/.env or set provider API keys."
        );
    }

    Ok(())
}

fn can_skip_credentials_for_clean_update(command: &Command, ctx: &WikiContext) -> bool {
    matches!(command, Command::Update)
        && metadata::is_update_noop_ctx(ctx)
        && !credentials::has_provider_api_key_for_any()
}

/// Returns `true` when stdin is a TTY (safe for dialoguer prompts).
pub fn stdin_is_tty() -> bool {
    std::io::stdin().is_terminal()
}

/// Check whether the configured provider has credentials (API key or OAuth).
pub fn provider_has_api_key(provider: &str) -> bool {
    provider_is_configured(provider)
}
