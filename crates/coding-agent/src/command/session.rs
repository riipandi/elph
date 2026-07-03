use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::{SessionArgs, SessionCommands};

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
