use clap::{Parser, Subcommand};

use super::help;
use crate::runtime::{EXIT_SUCCESS, ExitCode};

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
    match cmd {
        SessionCommands::List => {
            help::unimplemented("Session list — not yet implemented");
            EXIT_SUCCESS
        }
        SessionCommands::Search { query } => {
            help::unimplemented(&format!(
                "Session search — not yet implemented (query: {})",
                query.as_deref().unwrap_or("<all>")
            ));
            EXIT_SUCCESS
        }
        SessionCommands::Delete { id } => {
            help::unimplemented(&format!("Session delete — not yet implemented (id: {id})"));
            EXIT_SUCCESS
        }
    }
}
