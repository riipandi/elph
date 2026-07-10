use clap::{Parser, Subcommand};

use super::help;
use crate::memory;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};

#[derive(Parser, Default)]
#[command(
    name = "memory",
    about = "Agent memory store (floppy) — persistent lessons across sessions",
    color = clap::ColorChoice::Auto
)]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub command: Option<MemoryCommands>,
}

#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Overview: counts, categories, top memories
    Status,
    /// List all memories (optionally filter by category)
    List {
        /// Filter: correction, user, insight, discovery, consolidated
        category: Option<String>,
    },
    /// Show last N tasks with retrievals and outcomes
    Tasks {
        /// Number of tasks to show (default: 10)
        #[arg(default_value_t = 10)]
        limit: u32,
    },
    /// Compact timeline of tasks and memory events
    Log {
        /// Number of events per kind to include (default: 20)
        #[arg(default_value_t = 20)]
        limit: u32,
    },
    /// Semantic search across memories (creates a task record)
    Search {
        /// Search query
        #[arg(required = true)]
        query: Vec<String>,
    },
    /// Delete memories below weight threshold
    Purge {
        /// Weight threshold (default: 0.5)
        #[arg(default_value_t = 0.5)]
        threshold: f64,
    },
}

pub fn handle(args: &MemoryArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<MemoryArgs>();
    };

    let paths = match Paths::resolve() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_ERROR;
        }
    };

    if let Err(err) = crate::platform::ensure_project(&paths) {
        eprintln!("error: {err}");
        return EXIT_ERROR;
    }

    if let Err(err) = crate::platform::ensure_datastore_blocking(&paths) {
        eprintln!("error: {err}");
        return EXIT_ERROR;
    }

    match memory::run(paths, cmd) {
        Ok(()) => EXIT_SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            EXIT_ERROR
        }
    }
}
