use anyhow::Result;
use std::path::Path;
use tokio::sync::mpsc;

use crate::session::SessionStore;
use crate::ui_events::AgentUiEvent;

use super::checkpoint_cmd::{write_checkpoint_history, write_checkpoint_restore};
use super::commands::{run_chat_turn, run_init_command, run_update_command};
use super::help::{slash_message, write_help};
use super::output::ShellWriter;

/// Result of handling one user input line.
pub struct HandleInputResult {
    pub should_exit: bool,
    pub lines: Vec<String>,
}

/// Handle a single REPL / prompt submission.
pub async fn handle_user_input(
    config: &crate::config::Config,
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
