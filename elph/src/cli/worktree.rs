use clap::{Parser, Subcommand};

use super::help;
use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Parser, Default)]
#[command(
    name = "worktree",
    about = "Manage git worktrees for coding-agent",
    color = clap::ColorChoice::Auto
)]
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

pub fn handle(args: &WorktreeArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<WorktreeArgs>();
    };
    match cmd {
        WorktreeCommands::List => {
            help::unimplemented("Worktree list — not yet implemented");
            EXIT_SUCCESS
        }
        WorktreeCommands::Show { id_or_path } => {
            help::unimplemented(&format!("Worktree show — not yet implemented (id_or_path: {id_or_path})"));
            EXIT_SUCCESS
        }
        WorktreeCommands::Rm { id_or_path, force } => {
            help::unimplemented(&format!(
                "Worktree rm — not yet implemented (id_or_path: {id_or_path}, force: {force})"
            ));
            EXIT_SUCCESS
        }
        WorktreeCommands::Gc => {
            help::unimplemented("Worktree gc — not yet implemented");
            EXIT_SUCCESS
        }
        WorktreeCommands::Db => {
            help::unimplemented("Worktree db — not yet implemented");
            EXIT_SUCCESS
        }
    }
}
