use clap::{Parser, Subcommand};

use super::help;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode};

#[derive(Parser, Default)]
#[command(
    name = "codegraph",
    about = "Semantic code index + thin impact graph",
    long_about = "Index the project into .elph/store.db for hybrid FTS/vector search and shallow impact queries.\n\
                  First-time setup: elph codegraph build\n\
                  Agent tools: code_search, code_impact, code_status, code_reindex (no build/purge).",
    color = clap::ColorChoice::Auto
)]
pub struct CodegraphArgs {
    #[command(subcommand)]
    pub command: Option<CodegraphCommands>,
}

#[derive(Subcommand)]
pub enum CodegraphCommands {
    /// Full index build (CLI-only; not exposed to the agent)
    Build,
    /// Incremental update (changed files only)
    Update,
    /// Show index statistics and Merkle fingerprint
    Status,
    /// Hybrid keyword + semantic search
    Search {
        /// Search query tokens
        #[arg(required = true)]
        query: Vec<String>,
        /// Max results
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: u32,
    },
    /// Shallow impact / neighbor lookup for a path or symbol
    Impact {
        /// Path, symbol name, or node id
        target: String,
        /// BFS depth
        #[arg(short = 'd', long, default_value_t = 1)]
        depth: u32,
        /// Max nodes
        #[arg(short = 'n', long, default_value_t = 30)]
        limit: u32,
    },
    /// Clear the codegraph index tables
    Purge,
}

pub fn handle(args: &CodegraphArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<CodegraphArgs>();
    };

    let paths = match crate::platform::Paths::resolve() {
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

    match crate::codegraph::run(paths, cmd) {
        Ok(()) => EXIT_SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            EXIT_ERROR
        }
    }
}
