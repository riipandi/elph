use clap::{Parser, Subcommand};

use super::style::{self, CliStyle, S_MUTED};
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
    let sty = CliStyle::auto();
    let Some(cmd) = &args.command else {
        return super::help::print_subcommand_help::<WorktreeArgs>();
    };
    let mut out = String::new();
    style::section(&mut out, sty, "Worktree");
    match cmd {
        WorktreeCommands::List => {
            style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
        }
        WorktreeCommands::Show { id_or_path } => {
            style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
            style::kv(&mut out, sty, "Target", id_or_path);
        }
        WorktreeCommands::Rm { id_or_path, force } => {
            style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
            style::kv(&mut out, sty, "Target", id_or_path);
            if *force {
                style::kv(&mut out, sty, "Force", "true");
            }
        }
        WorktreeCommands::Gc => {
            style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
        }
        WorktreeCommands::Db => {
            style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
        }
    }
    print!("{out}");
    EXIT_SUCCESS
}
