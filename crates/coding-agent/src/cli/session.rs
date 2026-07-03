use clap::{Args, Subcommand};

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
