mod acp;
mod codegraph;
mod completions;
mod default;
mod doctor;
mod export;
mod help;
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
use elph_agent::AgentBuilder;
use elph_core::utils::path::AppPaths;

use crate::runtime::ExitCode;

pub use codegraph::CodegraphArgs;
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
#[command(name = "elph", about, disable_version_flag = true, color = clap::ColorChoice::Auto)]
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
    /// Structural knowledge graph for smarter code reviews
    Codegraph(CodegraphArgs),
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

fn init_layout() -> Result<crate::runtime::Paths, ExitCode> {
    crate::runtime::ensure_layout_blocking(env!("CARGO_PKG_VERSION")).map_err(|err| {
        tracing::error!(error = %err, "failed to initialize elph home");
        crate::runtime::EXIT_ERROR
    })
}

fn init_datastore(paths: &crate::runtime::Paths) -> Result<(), ExitCode> {
    crate::runtime::ensure_datastore_blocking(paths).map_err(|err| {
        tracing::error!(error = %err, "failed to initialize elph databases");
        crate::runtime::EXIT_ERROR
    })
}

fn command_needs_datastore(cmd: &Commands) -> bool {
    matches!(
        cmd,
        Commands::Export(_)
            | Commands::Import(_)
            | Commands::Run(_)
            | Commands::Server(_)
            | Commands::Session(_)
            | Commands::Stats(_)
    )
}

pub fn run(cli: &Cli) -> ExitCode {
    let console_enabled = cli.command.is_some();
    let agent_builder = AgentBuilder::new(env!("CARGO_PKG_VERSION"))
        .env_prefix("ELPH")
        .app_name("elph")
        .quiet_env("ELPH_QUIET")
        .console_enabled(console_enabled);

    let _log_guard = match crate::runtime::Paths::resolve() {
        Ok(paths) => {
            let init = agent_builder.logs_dir(paths.logs_dir()).build();
            elph_core::logger::init(init.logging)
        }
        Err(_) => {
            let init = agent_builder.build();
            elph_core::logger::init(init.logging)
        }
    };

    let paths = match init_layout() {
        Ok(paths) => paths,
        Err(code) => return code,
    };

    let Some(cmd) = &cli.command else {
        if let Err(code) = init_datastore(&paths) {
            return code;
        }
        return default::handle();
    };

    if command_needs_datastore(cmd) {
        if let Err(code) = init_datastore(&paths) {
            return code;
        }
    }

    match cmd {
        Commands::Acp => acp::handle(),
        Commands::Codegraph(args) => codegraph::handle(args),
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
