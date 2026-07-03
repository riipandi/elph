use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::{WorktreeArgs, WorktreeCommands};

pub fn handle(args: &WorktreeArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        eprintln!("Manage git worktrees for coding-agent");
        eprintln!();
        eprintln!("Usage: elph worktree <command>");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  list  List tracked worktrees");
        eprintln!("  show  Show details for a specific worktree");
        eprintln!("  rm    Remove worktrees");
        eprintln!("  gc    Garbage-collect orphaned/stale worktrees");
        eprintln!("  db    Database maintenance");
        eprintln!("  help  Print this message or the help of the given subcommand(s)");
        return EXIT_SUCCESS;
    };
    match cmd {
        WorktreeCommands::List => {
            eprintln!("Worktree list — not yet implemented");
            EXIT_SUCCESS
        }
        WorktreeCommands::Show { id_or_path } => {
            eprintln!("Worktree show — not yet implemented (id_or_path: {id_or_path})");
            EXIT_SUCCESS
        }
        WorktreeCommands::Rm { id_or_path, force } => {
            eprintln!("Worktree rm — not yet implemented (id_or_path: {id_or_path}, force: {force})");
            EXIT_SUCCESS
        }
        WorktreeCommands::Gc => {
            eprintln!("Worktree gc — not yet implemented");
            EXIT_SUCCESS
        }
        WorktreeCommands::Db => {
            eprintln!("Worktree db — not yet implemented");
            EXIT_SUCCESS
        }
    }
}
