//! Interactive Owly shell — stays open for follow-ups (OpenWiki default).

mod output;

use anyhow::Result;
use std::path::Path;
use tokio::sync::mpsc;

pub use output::ShellWriter;

use crate::ui_events::AgentUiEvent;

use crate::agent::{self, RunAgentResult};
use crate::cli::{print_command_header, print_completion};
use crate::config::Config;
use crate::constants::OWLY_DIR;
use crate::docs::{self, DocumentationSnapshot};
use crate::ecosystem;
use crate::metadata;
use crate::session::{SessionRecovery, SessionStore};

/// Result of handling one user input line.
pub struct HandleInputResult {
    pub should_exit: bool,
    pub lines: Vec<String>,
}

/// Handle a single REPL / prompt submission.
pub async fn handle_user_input(
    config: &Config,
    cwd: &Path,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
    input: &str,
    ui_events: Option<mpsc::UnboundedSender<AgentUiEvent>>,
) -> Result<HandleInputResult> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(HandleInputResult {
            should_exit: false,
            lines: Vec::new(),
        });
    }

    let mut lines = Vec::new();
    let mut writer = match ui_events.clone() {
        Some(tx) => ShellWriter::live_ui(&mut lines, tx),
        None => ShellWriter::transcript(&mut lines),
    };

    let lower = trimmed.to_ascii_lowercase();
    if matches!(lower.as_str(), "/exit" | "/quit" | "exit" | "quit" | ":q") {
        writer.line("Goodbye!");
        return Ok(HandleInputResult {
            should_exit: true,
            lines,
        });
    }
    if lower == "/help" || lower == "help" {
        write_help(&mut writer);
        return Ok(HandleInputResult {
            should_exit: false,
            lines,
        });
    }
    if lower == "/clear" || lower == "clear" {
        session.reset_thread(cwd).await?;
        writer.line("Session cleared.");
        return Ok(HandleInputResult {
            should_exit: false,
            lines,
        });
    }
    if lower == "/history" || lower.starts_with("/history ") {
        write_checkpoint_history(session, trimmed, &mut writer).await?;
        return Ok(HandleInputResult {
            should_exit: false,
            lines,
        });
    }
    if lower.starts_with("/restore ") {
        write_checkpoint_restore(session, trimmed, &mut writer).await?;
        return Ok(HandleInputResult {
            should_exit: false,
            lines,
        });
    }
    if lower == "/init" || lower.starts_with("/init ") {
        let msg = slash_message(trimmed, "/init");
        run_init_command(config, cwd, stream, verbose, session, msg, &mut writer).await?;
        return Ok(HandleInputResult {
            should_exit: false,
            lines,
        });
    }
    if lower == "/update" || lower.starts_with("/update ") {
        let msg = slash_message(trimmed, "/update");
        run_update_command(config, cwd, stream, verbose, session, msg, &mut writer).await?;
        return Ok(HandleInputResult {
            should_exit: false,
            lines,
        });
    }

    run_chat_turn(config, cwd, stream, verbose, session, trimmed, true, &mut writer).await?;
    Ok(HandleInputResult {
        should_exit: false,
        lines,
    })
}

fn slash_message<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    input
        .strip_prefix(prefix)
        .or_else(|| input.strip_prefix(&prefix.to_ascii_uppercase()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn write_help(writer: &mut ShellWriter<'_>) {
    writer.blank();
    writer.line("Commands:");
    writer.line("  /init [message]    Initialize documentation");
    writer.line("  /update [message]  Update existing documentation");
    writer.line("  /history [n]       List recent checkpoints (default 10)");
    writer.line("  /restore <#|id>    Rewind session to a checkpoint");
    writer.line("  /clear             Start a fresh checkpoint thread");
    writer.line("  /help              Show this help");
    writer.line("  /exit              Quit");
    writer.blank();
    writer.line("Any other input is sent to the agent as a chat follow-up.");
    writer.blank();
}

fn history_limit(input: &str) -> usize {
    input
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10)
        .clamp(1, 50)
}

async fn write_checkpoint_history(session: &SessionStore, input: &str, writer: &mut ShellWriter<'_>) -> Result<()> {
    let limit = history_limit(input);
    let summaries = session.list_checkpoint_history(limit).await?;
    writer.blank();
    if summaries.is_empty() {
        writer.line("No checkpoints for this thread.");
    } else {
        writer.line(format!("Checkpoints (newest first, showing up to {limit}):"));
        for (index, summary) in summaries.iter().enumerate() {
            let short_id = summary.checkpoint_id.get(..8).unwrap_or(summary.checkpoint_id.as_str());
            writer.line(format!(
                "  #{} step={} source={} id={}… ({} message(s))",
                index + 1,
                summary.step,
                summary.source,
                short_id,
                summary.message_count
            ));
        }
        writer.line("Use /restore <#> or /restore <id-prefix> to rewind.");
    }
    writer.blank();
    Ok(())
}

async fn write_checkpoint_restore(session: &mut SessionStore, input: &str, writer: &mut ShellWriter<'_>) -> Result<()> {
    let arg = input
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: /restore <#|checkpoint_id>"))?;
    let checkpoint_id = session.resolve_checkpoint_id(arg).await?;
    let restored = session.restore_checkpoint(&checkpoint_id).await?;
    writer.blank();
    writer.line(format!(
        "Restored {restored} message(s) from checkpoint {}…",
        checkpoint_id.get(..8).unwrap_or(checkpoint_id.as_str())
    ));
    writer.line("Next turn will fork from this checkpoint.");
    writer.blank();
    Ok(())
}

fn write_command_header(writer: &mut ShellWriter<'_>, command: &str, provider: &str, model: &str) {
    if writer.has_live_ui() {
        writer.command_start(command, provider, model);
    } else if writer.is_transcript() {
        writer.blank();
        writer.line(format!(">_ Owly {command}"));
        writer.line(format!("provider: {provider}"));
        writer.line(format!("model: {model}"));
        writer.blank();
    } else {
        print_command_header(command, provider, model);
    }
}

fn write_completion(writer: &mut ShellWriter<'_>, message: &str) {
    if writer.has_live_ui() {
        writer.command_complete(message, true);
    } else if writer.is_transcript() {
        writer.blank();
        writer.line(format!("✓ {message}"));
        writer.blank();
    } else {
        print_completion(message);
    }
}

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

/// Startup hints shown in the TUI transcript before the first prompt.
pub fn startup_transcript_lines(
    restored_count: usize,
    recovery: &SessionRecovery,
    db_path: &std::path::Path,
) -> Vec<String> {
    let mut lines = Vec::new();
    if restored_count > 0 {
        lines.push(format!(
            "restored {restored_count} message(s) from {}",
            db_path.display()
        ));
    }
    if recovery.draft_restored {
        lines.push("recovered partial assistant response from interrupted turn".to_string());
    }
    if let Some(interrupt) = &recovery.pending_interrupt {
        lines.push(format!(
            "agent was waiting for your input ({interrupt}) when the last session ended"
        ));
    }
    if restored_count > 0 || recovery.draft_restored || recovery.pending_interrupt.is_some() {
        lines.push(String::new());
    }
    lines.push("Type /help for commands or /exit to quit.".to_string());
    lines.push(String::new());
    lines
}

/// Map an initial CLI command to the first prompt submission.
pub fn initial_input(initial: crate::startup::InitialRun) -> String {
    match initial {
        crate::startup::InitialRun::Init => "/init".to_string(),
        crate::startup::InitialRun::Update => "/update".to_string(),
        crate::startup::InitialRun::Chat { message } => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startup::InitialRun;

    #[test]
    fn initial_input_maps_flags() {
        assert_eq!(initial_input(InitialRun::Init), "/init");
        assert_eq!(initial_input(InitialRun::Update), "/update");
        assert_eq!(
            initial_input(InitialRun::Chat {
                message: "hello".into()
            }),
            "hello"
        );
    }

    #[test]
    fn history_limit_parses_and_clamps() {
        assert_eq!(history_limit("/history"), 10);
        assert_eq!(history_limit("/history 25"), 25);
        assert_eq!(history_limit("/history 999"), 50);
        assert_eq!(history_limit("/history 0"), 1);
    }
}
