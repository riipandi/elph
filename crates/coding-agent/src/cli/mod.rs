mod acp;
mod completions;
mod default;
mod doctor;
mod export;
mod extensions;
mod help;
mod import;
pub(crate) mod interactive;
mod mcp;
mod memory;
mod models;
mod provider;
mod run;
mod server;
mod session;
mod session_launch;
mod stats;
pub mod style;
mod update;
pub mod version;
mod worktree;

use crate::utils::path::AppPaths;
use clap::{Parser, Subcommand};
use elph_agent::AgentBuilder;

use crate::platform::ExitCode;

/// RAII guard that installs a SIGINT handler forwarding to the elph-tui
/// progress-ticker interrupt flag while alive, restoring the previous
/// disposition on drop.
///
/// Only active around CLI progress phases (boot, datastore init)
/// where the tick threads can observe the flag and abort with a clean
/// "Interrupted." message + exit 130. Long-running phases (`server`, `run`,
/// the TUI) never have the guard installed, so Ctrl+C keeps its default
/// terminate behavior there.
pub(crate) struct CliProgressInterruptGuard {
    #[cfg(unix)]
    previous: libc::sighandler_t,
}

impl CliProgressInterruptGuard {
    pub(crate) fn new() -> Self {
        #[cfg(unix)]
        {
            // SAFETY: libc::signal installs an async-signal-safe handler that
            // only stores to an atomic flag (no allocation, no locking). The
            // previous disposition is saved and restored on drop.
            extern "C" fn handle_sigint(_sig: libc::c_int) {
                elph_tui::note_interrupt();
            }
            let previous = unsafe { libc::signal(libc::SIGINT, handle_sigint as *const () as libc::sighandler_t) };
            Self { previous }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }
}

impl Drop for CliProgressInterruptGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: restoring a previously saved disposition is a plain signal
        // registration; no shared state is touched.
        unsafe {
            libc::signal(libc::SIGINT, self.previous);
        }
    }
}

pub use acp::AcpArgs;
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
pub use update::UpdateArgs;
pub use worktree::WorktreeArgs;

/// Minimalist AI agent companion for coding
#[derive(Parser)]
#[command(name = "elph", about, disable_version_flag = true, color = clap::ColorChoice::Auto)]
pub struct Cli {
    /// Print version information
    #[arg(short = 'V', long = "version", help = "Print version information")]
    pub version: bool,

    /// Continue the most recent session for this project (CWD / PROJECT_DIR) — no new session
    #[arg(
        short = 'c',
        long = "continue",
        help = "Continue last session for the current project"
    )]
    pub continue_session: bool,

    /// Resume a specific session by session ID (interactive TUI)
    #[arg(
        short = 'r',
        long = "resume",
        value_name = "SESSION_ID",
        help = "Resume a specific session by session ID"
    )]
    pub resume: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // clap subcommand payloads; boxing changes CLI parse ergonomics
pub enum Commands {
    /// Run Elph as an Agent Client Protocol (ACP) server
    Acp(AcpArgs),
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
    /// Run a prompt non-interactively (headless)
    Run(RunArgs),
    /// Run the Elph server (REST + WebSocket + web UI)
    Server(ServerArgs),
    /// List, search, or restore sessions
    Session(SessionArgs),
    /// Show token usage and cost statistics
    Stats(StatsArgs),
    /// Check for updates or install a specific version
    Update(UpdateArgs),
    /// Manage git worktrees
    Worktree(WorktreeArgs),
}

fn command_label(cli: &Cli) -> &'static str {
    match &cli.command {
        None if cli.resume.is_some() => "tui-resume",
        None if cli.continue_session => "tui-continue",
        None => "tui",
        Some(Commands::Acp(_)) => "acp",
        Some(Commands::Completions(_)) => "completions",
        Some(Commands::Doctor(_)) => "doctor",
        Some(Commands::Export(_)) => "export",
        Some(Commands::Import(_)) => "import",
        Some(Commands::Mcp(_)) => "mcp",
        Some(Commands::Memory(_)) => "memory",
        Some(Commands::Models(_)) => "models",
        Some(Commands::Extensions(_)) => "extensions",
        Some(Commands::Provider(_)) => "provider",
        Some(Commands::Run(_)) => "run",
        Some(Commands::Server(_)) => "server",
        Some(Commands::Session(_)) => "session",
        Some(Commands::Stats(_)) => "stats",
        Some(Commands::Update(_)) => "update",
        Some(Commands::Worktree(_)) => "worktree",
    }
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

    let console_enabled = false;
    let agent_builder = AgentBuilder::new(env!("CARGO_PKG_VERSION"))
        .env_prefix("ELPH")
        .app_name("elph")
        .quiet_env("ELPH_QUIET")
        .console_enabled(console_enabled);

    let _log_guard = match crate::platform::Paths::resolve() {
        Ok(paths) => {
            elph_agent::logger::install_panic_hook(paths.logs_dir());
            let logging = crate::platform::Settings::peek_logging(&paths);
            let init = agent_builder
                .clone()
                .logging_settings(logging)
                .logs_dir(paths.logs_dir())
                .build();
            elph_agent::logger::init(init.logging)
        }
        Err(_) => {
            log::warn!("path resolve failed; using fallback logs directory");
            if let Some(logs) = fallback_logs_dir() {
                elph_agent::logger::install_panic_hook(logs);
            }
            let init = agent_builder.build();
            elph_agent::logger::init(init.logging)
        }
    };

    log::debug!("cli start version={} command={}", env!("CARGO_PKG_VERSION"), command_label(cli));

    // Boot phases (home scaffold + datastore init) render progress on stderr;
    // keep Ctrl+C interactive there, then restore default signal behavior
    // before any long-running command handler (server, run, TUI) dispatches.
    let _progress_interrupt = CliProgressInterruptGuard::new();

    // Close any in-process `shell_use` PTY sessions when the process exits so
    // terminal sessions don't outlive the agent turn.
    let _shell_use_teardown = crate::tui::ShellUseTeardownGuard;

    let paths = match init_home() {
        Ok(paths) => paths,
        Err(code) => return code,
    };

    let Some(cmd) = &cli.command else {
        if let Err(code) = init_datastore(&paths) {
            return code;
        }
        return default::handle(cli.continue_session, cli.resume.clone());
    };

    if command_needs_datastore(cmd)
        && let Err(code) = init_datastore(&paths)
    {
        return code;
    }
    drop(_progress_interrupt);

    match cmd {
        Commands::Acp(args) => acp::handle(args),
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
        Commands::Worktree(args) => worktree::handle(args),
    }
}
