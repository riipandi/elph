use std::env;

use clap::{Parser, Subcommand};

use super::help;
use super::style::{self, CliStyle, S_ACCENT, S_BODY, S_MUTED, S_OK, S_TIP, S_WARN};
use crate::agent::SessionManager;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};

#[derive(Parser, Default)]
#[command(
    name = "session",
    about = "Manage coding-agent sessions",
    color = clap::ColorChoice::Auto
)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: Option<SessionCommands>,
}

#[derive(Subcommand)]
pub enum SessionCommands {
    /// List recent sessions (same as search with no query)
    List,
    /// Search sessions by keyword
    Search {
        /// Search query to filter sessions
        query: Option<String>,
    },
    /// Permanently delete a session from history
    Delete {
        /// Session ID to delete
        id: String,
    },
}

pub fn handle(args: &SessionArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<SessionArgs>();
    };

    let paths = match Paths::resolve() {
        Ok(p) => p,
        Err(err) => {
            help::cli_error(format!("resolve paths: {err}"));
            return EXIT_ERROR;
        }
    };
    if let Err(err) = crate::platform::ensure_datastore_blocking(&paths) {
        help::cli_error(format!("init datastore: {err}"));
        return EXIT_ERROR;
    }
    let cwd = env::current_dir().unwrap_or_else(|_| ".".into());
    let manager = match SessionManager::new(&paths, &cwd) {
        Ok(manager) => manager,
        Err(err) => {
            help::cli_error(format!("init session manager: {err}"));
            return EXIT_ERROR;
        }
    };

    match cmd {
        SessionCommands::List => list_sessions(&manager, &cwd, None),
        SessionCommands::Search { query } => list_sessions(&manager, &cwd, query.as_deref()),
        SessionCommands::Delete { id } => {
            match elph_agent::block_on(async {
                let sessions = manager.list().await?;
                let meta = sessions
                    .into_iter()
                    .find(|s| s.id == *id)
                    .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
                manager.delete(&meta).await
            }) {
                Ok(()) => {
                    let mut out = String::new();
                    style::success(&mut out, CliStyle::auto(), format!("Deleted session {id}"));
                    print!("{out}");
                    EXIT_SUCCESS
                }
                Err(err) => {
                    help::cli_error(format!("delete session: {err}"));
                    EXIT_ERROR
                }
            }
        }
    }
}

fn list_sessions(manager: &SessionManager, cwd: &std::path::Path, query: Option<&str>) -> ExitCode {
    match elph_agent::block_on(manager.list()) {
        Ok(sessions) => {
            let sessions: Vec<_> = match query {
                Some(q) if !q.trim().is_empty() => {
                    let q = q.to_lowercase();
                    sessions
                        .into_iter()
                        .filter(|s| {
                            s.id.to_lowercase().contains(&q)
                                || s.cwd.to_lowercase().contains(&q)
                                || s.name.as_ref().is_some_and(|n| n.to_lowercase().contains(&q))
                        })
                        .collect()
                }
                _ => sessions,
            };
            let sty = CliStyle::auto();
            let mut out = String::new();
            if sessions.is_empty() {
                style::info(
                    &mut out,
                    sty,
                    sty.paint(S_MUTED, format!("No sessions found for {}", cwd.display())),
                );
                style::tip(
                    &mut out,
                    sty,
                    "Sessions are created automatically when you start a conversation.",
                );
            } else {
                style::section(&mut out, sty, &format!("Sessions ({})", sessions.len()));
                use std::fmt::Write;
                let _ = writeln!(out);
                for meta in &sessions {
                    let name = meta.name.as_deref().unwrap_or("(untitled)");
                    let _ = writeln!(
                        out,
                        "  {}  {}",
                        sty.paint(S_ACCENT, &meta.id[..8.min(meta.id.len())]),
                        sty.paint(S_BODY, name),
                    );
                    let _ = writeln!(
                        out,
                        "   {}  created {}  ·  updated {}  ·  {}",
                        sty.paint(S_MUTED, "·"),
                        sty.paint(S_MUTED, &meta.created_at[..10]),
                        sty.paint(S_MUTED, &meta.updated_at[..10]),
                        sty.paint(S_MUTED, &meta.cwd),
                    );
                }
            }
            print!("{out}");
            EXIT_SUCCESS
        }
        Err(err) => {
            help::cli_error(format!("list sessions: {err}"));
            EXIT_ERROR
        }
    }
}
