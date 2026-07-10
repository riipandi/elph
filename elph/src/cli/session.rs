use std::env;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use elph_agent::LocalExecutionEnv;

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
            tracing::error!(error = %err, "resolve paths");
            return EXIT_ERROR;
        }
    };
    let cwd = env::current_dir().unwrap_or_else(|_| ".".into());
    let env = Arc::new(LocalExecutionEnv::new(&cwd));
    let manager = match SessionManager::new(&paths, env, &cwd) {
        Ok(manager) => manager,
        Err(err) => {
            tracing::error!(error = %err, "init session manager");
            return EXIT_ERROR;
        }
    };

    match cmd {
        SessionCommands::List | SessionCommands::Search { .. } => match elph_agent::block_on(manager.list()) {
            Ok(sessions) => {
                if sessions.is_empty() {
                    println!("No sessions found for {}", cwd.display());
                } else {
                    for meta in sessions {
                        println!("{}  {}  {}", meta.id, meta.created_at, meta.dir);
                    }
                }
                EXIT_SUCCESS
            }
            Err(err) => {
                tracing::error!(error = %err, "list sessions");
                EXIT_ERROR
            }
        },
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
                    tracing::error!(error = %err, "delete session");
                    EXIT_ERROR
                }
            }
        }
    }
}
