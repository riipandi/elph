use clap::{Args, Parser, Subcommand};

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
    /// Run Elph as an Agent Client Protocol (ACP) server over stdio.
    Acp,
    /// Generate shell completion scripts (bash, zsh, fish, powershell, etc)
    Completions,
    /// Show the configuration Elph discovers for this directory
    Doctor,
    /// Export a session as a ZIP archive
    Export,
    /// Import sessions into Elph
    Import,
    /// Manage MCP server configurations
    Mcp,
    /// List available models and exit
    Models,
    /// Manage AI providers and credentials
    Provider,
    /// Run the local Elph server (REST + WebSocket + web UI)
    Server,
    /// List, search, or restore sessions
    Session,
    /// Show token usage and cost statistics
    Stats,
    /// Check for updates or install a specific version
    Update(UpdateArgs),
    /// Print version information
    Version,
    /// Manage git worktrees
    Worktree,
}

#[derive(Args)]
pub struct UpdateArgs {
    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Emit machine-readable JSON output (for --check)
    #[arg(long)]
    pub json: bool,

    /// Force re-download and install even if already up to date
    #[arg(long)]
    pub force_reinstall: bool,

    /// Install a specific version (e.g. 0.0.0 or 0.0.0-canary)
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,

    /// Switch to the canary release channel (faster updates, may have bugs)
    #[arg(long)]
    pub canary: bool,

    /// Switch to the stable release channel (default, weekly releases)
    #[arg(long)]
    pub stable: bool,
}
