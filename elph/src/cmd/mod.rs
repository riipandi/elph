mod acp;
mod completions;
mod default;
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
pub mod version;
mod worktree;

use clap::{Parser, Subcommand};

use crate::runtime::ExitCode;

pub use completions::CompletionsArgs;
pub use doctor::DoctorArgs;
pub use export::ExportArgs;
pub use import::ImportArgs;
pub use mcp::McpArgs;
pub use models::ModelsArgs;
pub use plugin::PluginArgs;
pub use provider::ProviderArgs;
pub use run::RunArgs;
pub use server::ServerArgs;
pub use session::SessionArgs;
pub use stats::StatsArgs;
pub use update::UpdateArgs;
pub use worktree::WorktreeArgs;

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

pub fn run(cli: &Cli) -> ExitCode {
    if let Err(err) = crate::layout::ensure_blocking(env!("CARGO_PKG_VERSION")) {
        eprintln!("failed to initialize elph home: {err}");
        return crate::runtime::EXIT_ERROR;
    }

    let Some(cmd) = &cli.command else {
        return default::handle();
    };

    match cmd {
        Commands::Acp => acp::handle(),
        Commands::Completions(args) => completions::handle(args),
        Commands::Doctor(args) => doctor::handle(args),
        Commands::Export(args) => export::handle(args),
        Commands::Import(args) => import::handle(args),
        Commands::Mcp(args) => mcp::handle(args),
        Commands::Models(args) => models::handle(args),
        Commands::Plugin(args) => plugin::handle(args),
        Commands::Provider(args) => provider::handle(args),
        Commands::Run(args) => run::handle(args),
        Commands::Server(args) => server::handle(args),
        Commands::Session(args) => session::handle(args),
        Commands::Stats(args) => stats::handle(args),
        Commands::Update(args) => update::handle(args),
        Commands::Version => version::handle(),
        Commands::Worktree(args) => worktree::handle(args),
    }
}
