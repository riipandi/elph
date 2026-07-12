//! Command execution for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/cli.tsx` and `src/commands.ts`. Original MIT License, Copyright (c) 2026 LangChain.

pub mod doc_run;
mod non_interactive;

pub use doc_run::{apply_doc_run_result, run_init_agent, run_update_agent, should_skip_update_noop};

use anyhow::Result;

use crate::config::Config;
use crate::credentials;
use crate::env;
use crate::interactive;
use crate::mode::WikiContext;
use crate::startup;

/// Available commands
#[derive(Debug)]
pub enum Command {
    /// Initialize documentation
    Init,

    /// Update existing documentation
    Update,

    /// Chat message
    Chat { message: Option<String> },
}

/// Run a command
pub async fn run_command(
    command: Command,
    ctx: &WikiContext,
    model_override: Option<&str>,
    print_mode: bool,
    stream: bool,
    verbose: bool,
    dry_run: bool,
) -> Result<()> {
    ctx.ensure_layout()?;
    credentials::load_env()?;
    let config = Config::resolve(model_override, &ctx.repo_cwd)?;

    if dry_run {
        return non_interactive::run_dry_run(&config, ctx, &command);
    }

    let config = interactive::ensure_provider_setup(config).await?;
    startup::validate_non_interactive(&command, ctx)?;
    env::setup_environment(&config)?;
    non_interactive::run_non_interactive(&config, ctx, command, print_mode, stream, verbose).await
}
