//! Command execution for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/cli.tsx` and `src/commands.ts`. Original MIT License, Copyright (c) 2026 LangChain.

mod non_interactive;

use anyhow::Result;
use std::path::Path;

use crate::config::Config;
use crate::credentials;
use crate::env;
use crate::startup::{self, StartupMode};
use crate::tui;

/// Available commands
#[derive(Debug)]
pub enum Command {
    /// Initialize documentation
    Init,

    /// Update existing documentation
    Update,

    /// Interactive chat
    Chat { message: Option<String> },
}

/// Run a command
pub async fn run_command(
    command: Command,
    cwd: &Path,
    model_override: Option<&str>,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    credentials::load_env()?;
    let config = Config::resolve(model_override, cwd)?;

    let mode = startup::resolve_startup_mode(&command, print_mode);

    match mode {
        StartupMode::NonInteractive => {
            startup::validate_non_interactive(&command, cwd)?;
            env::setup_environment(&config)?;
            non_interactive::run_non_interactive(&config, cwd, command, print_mode, stream, verbose).await
        }
        StartupMode::Interactive { initial } => tui::run_interactive(&config, cwd, stream, verbose, initial).await,
    }
}
