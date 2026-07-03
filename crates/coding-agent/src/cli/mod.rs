mod completions;
mod doctor;
mod export;
mod import;
mod mcp;
mod models;
mod plugin;
mod provider;
mod run;
mod server;
mod session;
mod stats;
mod update;
mod worktree;

use clap::{Parser, Subcommand};

pub use completions::CompletionsArgs;
pub use doctor::DoctorArgs;
pub use export::ExportArgs;
pub use import::ImportArgs;
pub use mcp::{McpArgs, McpCommands};
pub use models::ModelsArgs;
pub use plugin::{PluginArgs, PluginCommands};
pub use provider::{ProviderArgs, ProviderCommands};
pub use run::RunArgs;
pub use server::{ServerArgs, ServerCommands};
pub use session::{SessionArgs, SessionCommands};
pub use stats::StatsArgs;
pub use update::UpdateArgs;
pub use worktree::{WorktreeArgs, WorktreeCommands};

/// Minimalist AI agent companion for coding
#[derive(Parser)]
#[command(name = "elph", about, disable_version_flag = true)]
pub struct Cli {
    /// Print version information
    #[arg(short = 'V', long = "version", help = "Print version information")]
    pub version: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run Elph as an Agent Client Protocol (ACP) server over stdio
    Acp,
    /// Generate shell completion scripts (bash, zsh, fish, powershell, etc)
    Completions(CompletionsArgs),
    /// Show the configuration Elph discovers for this directory
    Doctor(DoctorArgs),
    /// Export a session transcript or archive
    Export(ExportArgs),
    /// Import sessions into Elph
    Import(ImportArgs),
    /// Manage MCP server configurations
    Mcp(McpArgs),
    /// List available models and exit
    Models(ModelsArgs),
    /// Manage plugins and extensions
    Plugin(PluginArgs),
    /// Manage AI providers and credentials
    Provider(ProviderArgs),
    /// Run a prompt non-interactively and exit
    Run(RunArgs),
    /// Run the local Elph server (REST + WebSocket + web UI)
    Server(ServerArgs),
    /// List, search, or restore sessions
    Session(SessionArgs),
    /// Show token usage and cost statistics
    Stats(StatsArgs),
    /// Check for updates or install a specific version
    Update(UpdateArgs),
    /// Print version information
    Version,
    /// Manage git worktrees
    Worktree(WorktreeArgs),
}
