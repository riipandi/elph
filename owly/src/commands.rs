//! Command execution for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/cli.tsx` and `src/commands.ts`. Original MIT License, Copyright (c) 2026 LangChain.

use anyhow::Result;
use std::path::Path;

use crate::agent;
use crate::cli::{print_command_header, print_completion};
use crate::config::Config;
use crate::constants::OWLY_DIR;
use crate::credentials;
use crate::docs;
use crate::env;
use crate::metadata;

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
    // Load environment
    credentials::load_env()?;

    // Resolve configuration
    let config = Config::resolve(model_override, cwd)?;

    // Setup environment
    env::setup_environment(&config)?;

    match command {
        Command::Init => run_init(&config, cwd, print_mode, stream, verbose).await,
        Command::Update => run_update(&config, cwd, print_mode, stream, verbose).await,
        Command::Chat { message } => {
            if let Some(msg) = message {
                run_chat(&config, cwd, &msg, print_mode, stream, verbose).await
            } else {
                // Interactive mode: multi-turn chat with ask_user tools
                agent::run_interactive(&config, cwd, stream, verbose).await
            }
        }
    }
}

/// Run the init command
async fn run_init(config: &Config, cwd: &Path, print_mode: bool, stream: bool, verbose: bool) -> Result<()> {
    print_command_header("Init", &config.provider, &config.model_id);

    // Check if documentation already exists
    let owly_dir = cwd.join(OWLY_DIR);
    if owly_dir.exists() {
        println!("Documentation already exists. Updating...");
        println!();
        // Delegate to update instead of calling run_update directly
        let (system_prompt, user_prompt) =
            agent::prepare_update_command(cwd, None, &config.model_id, metadata::load_metadata(cwd).as_ref());
        let result = agent::run_agent(agent::RunAgentOptions {
            command: "update",
            system_prompt: &system_prompt,
            user_prompt: &user_prompt,
            config,
            cwd,
            print_mode,
            stream,
            verbose,
        })
        .await?;
        if print_mode {
            println!("{}", result);
        } else {
            docs::save_update_metadata(cwd, "update", &config.elph_model_id())?;
            print_completion(&result);
        }
        return Ok(());
    }

    // Prepare command
    let (system_prompt, user_prompt) = agent::prepare_init_command(cwd, None, &config.model_id);

    // Run agent
    let result = agent::run_agent(agent::RunAgentOptions {
        command: "init",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
    })
    .await?;

    if print_mode {
        println!("{}", result);
    } else {
        // Save update metadata
        docs::save_update_metadata(cwd, "init", &config.elph_model_id())?;
        print_completion(&result);
    }

    Ok(())
}

/// Run the update command
async fn run_update(config: &Config, cwd: &Path, print_mode: bool, stream: bool, verbose: bool) -> Result<()> {
    print_command_header("Update", &config.provider, &config.model_id);

    // Check if documentation exists
    let owly_dir = cwd.join(OWLY_DIR);
    if !owly_dir.exists() {
        println!("No documentation found. Initializing...");
        println!();
        // Delegate to init instead of calling run_init directly
        let (system_prompt, user_prompt) = agent::prepare_init_command(cwd, None, &config.model_id);
        let result = agent::run_agent(agent::RunAgentOptions {
            command: "init",
            system_prompt: &system_prompt,
            user_prompt: &user_prompt,
            config,
            cwd,
            print_mode,
            stream,
            verbose,
        })
        .await?;
        if print_mode {
            println!("{}", result);
        } else {
            docs::save_update_metadata(cwd, "init", &config.elph_model_id())?;
            print_completion(&result);
        }
        return Ok(());
    }

    // Check if update is a no-op
    if !print_mode && metadata::is_update_noop(cwd) {
        println!("No changes detected. Skipping.");
        return Ok(());
    }

    // Load last update metadata
    let last_update = metadata::load_metadata(cwd);

    // Prepare command
    let (system_prompt, user_prompt) = agent::prepare_update_command(cwd, None, &config.model_id, last_update.as_ref());

    // Run agent
    let result = agent::run_agent(agent::RunAgentOptions {
        command: "update",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
    })
    .await?;

    if print_mode {
        println!("{}", result);
    } else {
        // Save update metadata
        docs::save_update_metadata(cwd, "update", &config.elph_model_id())?;
        print_completion(&result);
    }

    Ok(())
}

/// Run the chat command
async fn run_chat(
    config: &Config,
    cwd: &Path,
    message: &str,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    print_command_header("Chat", &config.provider, &config.model_id);

    // Prepare command
    let (system_prompt, user_prompt) = agent::prepare_chat_command(message);

    // Run agent
    let result = agent::run_agent(agent::RunAgentOptions {
        command: "chat",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
    })
    .await?;

    println!("{}", result);

    Ok(())
}
