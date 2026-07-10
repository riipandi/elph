use anyhow::Result;
use std::path::Path;

use crate::agent::{self, RunAgentResult};
use crate::cli::{print_command_header, print_completion};
use crate::config::Config;
use crate::constants::OWLY_DIR;
use crate::docs::{self, DocumentationSnapshot};
use crate::ecosystem;
use crate::metadata;
use crate::session::SessionStore;

use super::Command;

pub(super) async fn run_non_interactive(
    config: &Config,
    cwd: &Path,
    command: Command,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    let mut session = SessionStore::open(cwd).await?;

    match command {
        Command::Init => run_non_interactive_init(config, cwd, print_mode, stream, verbose, &mut session).await,
        Command::Update => run_non_interactive_update(config, cwd, print_mode, stream, verbose, &mut session).await,
        Command::Chat { message: Some(msg) } => {
            run_non_interactive_chat(config, cwd, &msg, print_mode, stream, verbose, &mut session).await
        }
        Command::Chat { message: None } => {
            anyhow::bail!("Interactive chat requires a terminal. Pass a message or use --init or --update.");
        }
    }
}

async fn run_non_interactive_init(
    config: &Config,
    cwd: &Path,
    print_mode: bool,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
) -> Result<()> {
    let owly_dir = cwd.join(OWLY_DIR);
    if owly_dir.exists() {
        println!("Documentation already exists. Updating...");
        println!();
        return do_non_interactive_update(config, cwd, print_mode, stream, verbose, session).await;
    }
    do_non_interactive_init(config, cwd, print_mode, stream, verbose, session).await
}

async fn run_non_interactive_update(
    config: &Config,
    cwd: &Path,
    print_mode: bool,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
) -> Result<()> {
    let owly_dir = cwd.join(OWLY_DIR);
    if !owly_dir.exists() {
        println!("No documentation found. Initializing...");
        println!();
        return do_non_interactive_init(config, cwd, print_mode, stream, verbose, session).await;
    }
    do_non_interactive_update(config, cwd, print_mode, stream, verbose, session).await
}

async fn do_non_interactive_init(
    config: &Config,
    cwd: &Path,
    print_mode: bool,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
) -> Result<()> {
    print_command_header("Init", &config.provider, &config.model_id);

    let snapshot = docs::create_snapshot(cwd)?;
    let (system_prompt, user_prompt) = agent::prepare_init_command(cwd, None, &config.model_id);
    let user_prompt = format!("{user_prompt}{}", crate::prompts::create_runtime_note(cwd));

    let result = agent::run_agent(agent::RunAgentOptions {
        command: "init",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
        session: Some(session),
        is_followup: false,
        docs_snapshot_before: Some(snapshot.clone()),
        quiet: false,
        ui_events: None,
    })
    .await?;

    finish_non_interactive_doc_run(cwd, config, "init", &result, &snapshot, print_mode)
}

async fn do_non_interactive_update(
    config: &Config,
    cwd: &Path,
    print_mode: bool,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
) -> Result<()> {
    if !print_mode && metadata::is_update_noop(cwd) {
        println!("No changes detected. Skipping.");
        return Ok(());
    }

    print_command_header("Update", &config.provider, &config.model_id);

    let snapshot = docs::create_snapshot(cwd)?;
    let last_update = metadata::load_metadata(cwd);
    let (system_prompt, user_prompt) = agent::prepare_update_command(cwd, None, &config.model_id, last_update.as_ref());
    let user_prompt = format!("{user_prompt}{}", crate::prompts::create_runtime_note(cwd));

    let result = agent::run_agent(agent::RunAgentOptions {
        command: "update",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
        session: Some(session),
        is_followup: false,
        docs_snapshot_before: Some(snapshot.clone()),
        quiet: false,
        ui_events: None,
    })
    .await?;

    finish_non_interactive_doc_run(cwd, config, "update", &result, &snapshot, print_mode)
}

async fn run_non_interactive_chat(
    config: &Config,
    cwd: &Path,
    message: &str,
    print_mode: bool,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
) -> Result<()> {
    print_command_header("Chat", &config.provider, &config.model_id);

    let (system_prompt, user_prompt) = agent::prepare_chat_command(message);
    let user_prompt = format!("{user_prompt}{}", crate::prompts::create_runtime_note(cwd));

    let result = agent::run_agent(agent::RunAgentOptions {
        command: "chat",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
        session: Some(session),
        is_followup: false,
        docs_snapshot_before: None,
        quiet: false,
        ui_events: None,
    })
    .await?;

    if print_mode {
        if !result.completion_message.is_empty() {
            println!("{}", result.completion_message);
        }
    } else if !result.completion_message.is_empty() {
        println!("{}", result.completion_message);
        println!();
    }

    Ok(())
}

fn finish_non_interactive_doc_run(
    cwd: &Path,
    config: &Config,
    command: &str,
    result: &RunAgentResult,
    before: &DocumentationSnapshot,
    print_mode: bool,
) -> Result<()> {
    if result.skipped {
        if !print_mode {
            println!("{}", result.completion_message);
        }
        return Ok(());
    }

    if result.docs_changed {
        docs::save_update_metadata_if_changed(cwd, command, &config.elph_model_id(), before)?;
        ecosystem::sync_agent_guidance_files(cwd)?;
    }

    if print_mode {
        if !result.completion_message.is_empty() {
            println!("{}", result.completion_message);
        }
    } else {
        print_completion(&result.completion_message);
    }

    Ok(())
}
