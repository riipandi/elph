use clap::{Parser, Subcommand};

use super::help;
use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Parser, Default)]
#[command(
    name = "codegraph",
    about = "Structural knowledge graph for smarter code reviews",
    color = clap::ColorChoice::Auto
)]
pub struct CodegraphArgs {
    #[command(subcommand)]
    pub command: Option<CodegraphCommands>,
}

#[derive(Subcommand)]
pub enum CodegraphCommands {
    /// Full graph build (parse all files)
    Build,
    /// Incremental update (changed files only)
    Update,
    /// Auto-update on file changes
    Watch,
    /// Show graph statistics
    Status,
    /// Analyze change impact (risk-scored review)
    Changes,
    /// Run evaluation benchmarks
    Eval,
    /// Run post-processing (flows, communities, FTS)
    Postprocess,
    /// List registered repositories
    Repos,
    /// Register a repository in the multi-repo registry
    Register {
        /// Path to the repository root
        path: Option<String>,
    },
    /// Remove a repository from the registry
    Unregister {
        /// Repository name or path to remove
        name: String,
    },
    /// Generate interactive HTML graph
    Visualize,
    /// Start MCP server for any AI agent
    Serve,
    /// Generate markdown wiki from communities
    Wiki,
}

pub fn handle(args: &CodegraphArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<CodegraphArgs>();
    };

    match cmd {
        CodegraphCommands::Build => {
            help::unimplemented("codegraph build — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Update => {
            help::unimplemented("codegraph update — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Watch => {
            help::unimplemented("codegraph watch — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Status => {
            help::unimplemented("codegraph status — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Changes => {
            help::unimplemented("codegraph changes — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Eval => {
            help::unimplemented("codegraph eval — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Postprocess => {
            help::unimplemented("codegraph postprocess — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Repos => {
            help::unimplemented("codegraph repos — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Register { path } => {
            help::unimplemented(&format!(
                "codegraph register — not yet implemented (path: {})",
                path.as_deref().unwrap_or("<cwd>")
            ));
            EXIT_SUCCESS
        }
        CodegraphCommands::Unregister { name } => {
            help::unimplemented(&format!("codegraph unregister — not yet implemented (name: {name})"));
            EXIT_SUCCESS
        }
        CodegraphCommands::Visualize => {
            help::unimplemented("codegraph visualize — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Serve => {
            help::unimplemented("codegraph serve — not yet implemented");
            EXIT_SUCCESS
        }
        CodegraphCommands::Wiki => {
            help::unimplemented("codegraph wiki — not yet implemented");
            EXIT_SUCCESS
        }
    }
}
