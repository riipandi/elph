use anyhow::Result;
use std::path::Path;

use crate::agent::{self, RunAgentResult};
use crate::config::Config;
use crate::constants::OWLY_DIR;
use crate::docs::{self, DocumentationSnapshot};
use crate::ecosystem;
use crate::metadata;
use crate::session::SessionStore;

use super::output::ShellWriter;
use super::writer_util::{write_command_header, write_completion};

pub async fn run_init_command(
    config: &Config,
    cwd: &Path,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
    user_message: Option<&str>,
    writer: &mut ShellWriter<'_>,
) -> Result<()> {
    let owly_dir = cwd.join(OWLY_DIR);
    if owly_dir.exists() {
        writer.line("Documentation already exists. Running update instead...");
        writer.blank();
        return do_update(config, cwd, stream, verbose, session, user_message, writer).await;
    }
    do_init(config, cwd, stream, verbose, session, user_message, writer).await
}

pub async fn run_update_command(
    config: &Config,
    cwd: &Path,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
    user_message: Option<&str>,
    writer: &mut ShellWriter<'_>,
) -> Result<()> {
    let owly_dir = cwd.join(OWLY_DIR);
    if !owly_dir.exists() {
        writer.line("No documentation found. Running init instead...");
        writer.blank();
        return do_init(config, cwd, stream, verbose, session, user_message, writer).await;
    }
    do_update(config, cwd, stream, verbose, session, user_message, writer).await
}

async fn do_init(
    config: &Config,
    cwd: &Path,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
    user_message: Option<&str>,
    writer: &mut ShellWriter<'_>,
) -> Result<()> {
    write_command_header(writer, "Init", &config.provider, &config.model_id);

    let snapshot = docs::create_snapshot(cwd)?;
    let (system_prompt, user_prompt) = agent::prepare_init_command(cwd, user_message, &config.model_id);
    let user_prompt = format!("{user_prompt}{}", crate::prompts::create_runtime_note(cwd));
    let quiet = writer.is_transcript();

    let result = agent::run_agent(agent::RunAgentOptions {
        command: "init",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode: false,
        stream,
        verbose,
        quiet,
        session: Some(session),
        is_followup: false,
        docs_snapshot_before: Some(snapshot.clone()),
        ui_events: writer.ui_sender(),
    })
    .await?;

    finish_doc_run(cwd, config, "init", &result, &snapshot, writer)
}

async fn do_update(
    config: &Config,
    cwd: &Path,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
    user_message: Option<&str>,
    writer: &mut ShellWriter<'_>,
) -> Result<()> {
    write_command_header(writer, "Update", &config.provider, &config.model_id);

    if user_message.is_none() && metadata::is_update_noop(cwd) {
        writer.line("No repository changes detected since the last Owly update; skipping agent run.");
        return Ok(());
    }

    let snapshot = docs::create_snapshot(cwd)?;
    let last_update = metadata::load_metadata(cwd);
    let (system_prompt, user_prompt) =
        agent::prepare_update_command(cwd, user_message, &config.model_id, last_update.as_ref());
    let user_prompt = format!("{user_prompt}{}", crate::prompts::create_runtime_note(cwd));
    let quiet = writer.is_transcript();

    let result = agent::run_agent(agent::RunAgentOptions {
        command: "update",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode: false,
        stream,
        verbose,
        quiet,
        session: Some(session),
        is_followup: false,
        docs_snapshot_before: Some(snapshot.clone()),
        ui_events: writer.ui_sender(),
    })
    .await?;

    finish_doc_run(cwd, config, "update", &result, &snapshot, writer)
}

#[allow(clippy::too_many_arguments)]
pub async fn run_chat_turn(
    config: &Config,
    cwd: &Path,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
    message: &str,
    is_followup: bool,
    writer: &mut ShellWriter<'_>,
) -> Result<()> {
    if !is_followup {
        write_command_header(writer, "Chat", &config.provider, &config.model_id);
    }

    let (system_prompt, user_prompt) = if is_followup {
        let system = crate::prompts::create_interactive_system_prompt();
        (system, message.to_string())
    } else {
        let (system, prompt) = agent::prepare_chat_command(message);
        (system, format!("{prompt}{}", crate::prompts::create_runtime_note(cwd)))
    };

    let quiet = writer.is_transcript();
    let result = agent::run_agent(agent::RunAgentOptions {
        command: "chat",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        cwd,
        print_mode: false,
        stream,
        verbose,
        quiet,
        session: Some(session),
        is_followup,
        docs_snapshot_before: None,
        ui_events: writer.ui_sender(),
    })
    .await?;

    if !result.completion_message.is_empty() {
        writer.line(&result.completion_message);
        writer.blank();
    }
    Ok(())
}

fn finish_doc_run(
    cwd: &Path,
    config: &Config,
    command: &str,
    result: &RunAgentResult,
    before: &DocumentationSnapshot,
    writer: &mut ShellWriter<'_>,
) -> Result<()> {
    if result.skipped {
        if writer.has_live_ui() {
            writer.command_complete(&result.completion_message, true);
        } else {
            writer.line(&result.completion_message);
        }
        return Ok(());
    }
    if result.docs_changed {
        docs::save_update_metadata_if_changed(cwd, command, &config.elph_model_id(), before)?;
        ecosystem::sync_agent_guidance_files(cwd)?;
    }
    if result.completion_message.is_empty() && writer.has_live_ui() {
        if result.docs_changed {
            writer.command_complete("Documentation updated.", true);
        }
        return Ok(());
    }
    write_completion(writer, &result.completion_message);
    Ok(())
}
