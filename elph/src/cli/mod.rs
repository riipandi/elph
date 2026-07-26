mod acp;
mod codegraph;
mod completions;
mod default;
mod doctor;
mod export;
mod extensions;
mod help;
mod import;
mod mcp;
mod memory;
mod models;
mod provider;
mod run;
mod server;
mod session;
mod stats;
mod tools;
mod update;
pub mod version;
mod worktree;

use clap::{Parser, Subcommand};
use elph_agent::AgentBuilder;
use elph_core::utils::path::AppPaths;

use crate::platform::ExitCode;

pub use codegraph::CodegraphArgs;
pub use completions::CompletionsArgs;
pub use doctor::DoctorArgs;
pub use export::ExportArgs;
pub use extensions::ExtensionsArgs;
pub use import::ImportArgs;
pub use mcp::McpArgs;
pub use memory::{MemoryArgs, MemoryCommands};
pub use models::ModelsArgs;
pub use provider::ProviderArgs;
pub use run::RunArgs;
pub use server::ServerArgs;
pub use session::SessionArgs;
pub use stats::StatsArgs;
pub use tools::ToolsArgs;
pub use update::UpdateArgs;
pub use worktree::WorktreeArgs;

/// Minimalist AI agent companion for coding
#[derive(Parser)]
#[command(name = "elph", about, disable_version_flag = true, color = clap::ColorChoice::Auto)]
pub struct Cli {
    /// Print version information
    #[arg(short = 'V', long = "version", help = "Print version information")]
    pub version: bool,

    /// Resume a specific session by ID (interactive TUI)
    #[arg(long = "resume", value_name = "SESSION_ID")]
    pub resume: Option<String>,

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
    /// Inspect and manage agent memory (floppy)
    Memory(MemoryArgs),
    /// List available models and exit
    Models(ModelsArgs),
    /// Manage Elph extensions
    #[command(visible_alias = "ext")]
    Extensions(ExtensionsArgs),
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
    /// List available agent tools and their descriptions
    Tools(ToolsArgs),
    /// Manage git worktrees
    Worktree(WorktreeArgs),
}

fn init_home() -> Result<crate::platform::Paths, ExitCode> {
    crate::platform::ensure_home_blocking(env!("CARGO_PKG_VERSION")).map_err(|err| {
        help::cli_error(format!("failed to initialize elph home: {err}"));
        crate::platform::EXIT_ERROR
    })
}

fn init_datastore(paths: &crate::platform::Paths) -> Result<(), ExitCode> {
    crate::platform::ensure_datastore_blocking(paths).map_err(|err| {
        help::cli_error(format!("failed to initialize elph databases: {err}"));
        crate::platform::EXIT_ERROR
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

/// Best-effort logs directory when full [`Paths::resolve`] fails.
fn fallback_logs_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(data) = std::env::var_os("ELPH_DATA_DIR") {
        return Some(PathBuf::from(data).join("logs"));
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(xdg).join("elph").join("logs"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/elph/logs"))
}

pub fn run(cli: &Cli) -> ExitCode {
    if let Some(Commands::Completions(args)) = &cli.command {
        return completions::handle(args);
    }

    let console_enabled = cli.command.is_some();
    let agent_builder = AgentBuilder::new(env!("CARGO_PKG_VERSION"))
        .env_prefix("ELPH")
        .app_name("elph")
        .quiet_env("ELPH_QUIET")
        .console_enabled(console_enabled);

    let _log_guard = match crate::platform::Paths::resolve() {
        Ok(paths) => {
            // Panic → ~/.local/share/elph/logs/crash.log (or $ELPH_DATA_DIR/logs/crash.log).
            elph_core::logger::install_panic_hook(paths.logs_dir());
            let init = agent_builder.logs_dir(paths.logs_dir()).build();
            elph_core::logger::init(init.logging)
        }
        Err(_) => {
            // Best-effort crash path if full path resolution failed.
            if let Some(logs) = fallback_logs_dir() {
                elph_core::logger::install_panic_hook(logs);
            }
            let init = agent_builder.build();
            elph_core::logger::init(init.logging)
        }
    };

    let paths = match init_home() {
        Ok(paths) => paths,
        Err(code) => return code,
    };

    let Some(cmd) = &cli.command else {
        if let Err(code) = init_datastore(&paths) {
            return code;
        }
        return default::handle(cli.resume.clone());
    };

    if command_needs_datastore(cmd)
        && let Err(code) = init_datastore(&paths)
    {
        return code;
    }

    match cmd {
        Commands::Acp => acp::handle(),
        Commands::Codegraph(args) => codegraph::handle(args),
        Commands::Completions(args) => completions::handle(args),
        Commands::Doctor(args) => doctor::handle(args),
        Commands::Export(args) => export::handle(args),
        Commands::Extensions(args) => extensions::handle(args),
        Commands::Import(args) => import::handle(args),
        Commands::Mcp(args) => mcp::handle(args),
        Commands::Memory(args) => memory::handle(args),
        Commands::Models(args) => models::handle(args),
        Commands::Provider(args) => provider::handle(args),
        Commands::Run(args) => run::handle(args),
        Commands::Server(args) => server::handle(args),
        Commands::Session(args) => session::handle(args),
        Commands::Stats(args) => stats::handle(args),
        Commands::Update(args) => update::handle(args),
        Commands::Version => version::handle(),
        Commands::Tools(args) => tools::handle(args),
        Commands::Worktree(args) => worktree::handle(args),
    }
}
