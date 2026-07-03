use clap::{Args, Subcommand};

#[derive(Args, Default)]
pub struct WorktreeArgs {
    #[command(subcommand)]
    pub command: Option<WorktreeCommands>,
}

#[derive(Subcommand)]
pub enum WorktreeCommands {
    /// List tracked worktrees
    List,
    /// Show details for a specific worktree
    Show {
        /// Worktree ID or path
        id_or_path: String,
    },
    /// Remove worktrees
    Rm {
        /// Worktree ID or path
        id_or_path: String,
        /// Remove without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Garbage-collect orphaned/stale worktrees
    Gc,
    /// Database maintenance
    Db,
}
