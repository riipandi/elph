use clap::{Parser, Subcommand};

use super::help;
use crate::memory;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};

#[derive(Parser, Default)]
#[command(
    name = "memory",
    about = "Project memory (floppy) — lessons, work log, and auto-recall",
    long_about = "Inspect and maintain the project-local agent memory store (.elph/store.db).\n\
                  Auto recall and work capture are enabled by default (settings.memory.*).",
    color = clap::ColorChoice::Auto
)]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub command: Option<MemoryCommands>,
}

#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Overview: counts, categories, top memories, and auto-feature flags
    Status,
    /// List memories (newest first when unfiltered)
    List {
        /// Filter: correction, user, insight, discovery, work, consolidated
        category: Option<String>,
        /// Max entries to show
        #[arg(short = 'n', long, default_value_t = 50)]
        limit: u32,
    },
    /// Newest memories first (optional category)
    Recent {
        /// Filter: correction, user, insight, discovery, work, consolidated
        category: Option<String>,
        /// Max entries
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: u32,
    },
    /// Show recent recall tasks with retrievals
    Tasks {
        /// Number of tasks to show (default: 10)
        #[arg(default_value_t = 10)]
        limit: u32,
    },
    /// Compact timeline of tasks and memory events
    Log {
        /// Number of events per kind (default: 20)
        #[arg(default_value_t = 20)]
        limit: u32,
    },
    /// Semantic search (read-only; does not create a training task)
    Search {
        /// Search query
        #[arg(required = true)]
        query: Vec<String>,
    },
    /// Delete memories below a weight threshold
    Purge {
        /// Weight threshold (default: 0.5, range 0–5)
        #[arg(default_value_t = 0.5)]
        threshold: f64,
    },
    /// Wipe the entire memory store (all memories + tasks; requires confirmation)
    Flush,
    /// Merge near-duplicate memories (maintenance)
    Consolidate,
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
