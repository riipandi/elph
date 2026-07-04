use clap::{Args, Subcommand};

use crate::app::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
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
        eprintln!("Manage coding-agent sessions");
        eprintln!();
        eprintln!("Usage: elph session <command>");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  list    List recent sessions (same as search with no query)");
        eprintln!("  search  Search sessions by keyword");
        eprintln!("  delete  Permanently delete a session from history");
        eprintln!("  help    Print this message or the help of the given subcommand(s)");
        return EXIT_SUCCESS;
    };
    match cmd {
        SessionCommands::List => {
            eprintln!("Session list — not yet implemented");
            EXIT_SUCCESS
        }
        SessionCommands::Search { query } => {
            eprintln!(
                "Session search — not yet implemented (query: {})",
                query.as_deref().unwrap_or("<all>")
            );
            EXIT_SUCCESS
        }
        SessionCommands::Delete { id } => {
            eprintln!("Session delete — not yet implemented (id: {id})");
            EXIT_SUCCESS
        }
    }
}
