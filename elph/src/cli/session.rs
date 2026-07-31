use std::env;

use clap::{Parser, Subcommand};

use super::help;
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
    // Ensure platform schema (sessions/goals/…) exists before listing.
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
                    println!("Deleted session {id}");
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
            if sessions.is_empty() {
                println!("No sessions found for {}", cwd.display());
            } else {
                for meta in sessions {
                    let name = meta.name.as_deref().unwrap_or("-");
                    println!(
                        "{}  created {}  updated {}  cwd={}  name={}",
                        meta.id, meta.created_at, meta.updated_at, meta.cwd, name
                    );
                }
            }
            EXIT_SUCCESS
        }
        Err(err) => {
            help::cli_error(format!("list sessions: {err}"));
            EXIT_ERROR
        }
    }
}
