//! CLI argument parsing and execution.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/cli.tsx`. Original MIT License, Copyright (c) 2026 LangChain.

use clap::Parser;
use std::path::PathBuf;

use crate::commands::{Command, run_command};

/// Owly v0.0.1 - agent docs for codebases
#[derive(Parser)]
#[command(
    name = "owly",
    about = "Owly v0.0.1 agent docs for codebases",
    long_about = None,
    after_help = "Tip: ask for a docs change, or use /exit when you are done."
)]
pub struct Cli {
    /// Run once and print the final assistant output
    #[arg(short, long)]
    pub print: bool,

    /// Use a model ID for this run (providerId/modelId)
    #[arg(long)]
    pub model: Option<String>,

    /// Generate initial owly documentation
    #[arg(long)]
    pub init: bool,

    /// Update existing owly documentation
    #[arg(long)]
    pub update: bool,

    /// Show stream response from LLM (without thinking)
    #[arg(short, long)]
    pub stream: bool,

    /// Show stream response and thinking from LLM
    #[arg(short, long)]
    pub verbose: bool,

    /// Message to send to the agent
    #[arg(trailing_var_arg = true)]
    pub message: Option<Vec<String>>,

    /// Working directory (defaults to current directory)
    #[arg(short, long)]
    pub directory: Option<PathBuf>,
}

impl Cli {
    pub async fn execute(self) -> anyhow::Result<()> {
        let cwd = self
            .directory
            .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

        // Determine command
        let command = if self.init {
            Command::Init
        } else if self.update {
            Command::Update
        } else if let Some(msg) = self.message {
            let msg = msg.join(" ");
            Command::Chat { message: Some(msg) }
        } else if self.print {
            // --print without message is an error
            anyhow::bail!("--print requires a message argument");
        } else {
            // TODO: improve TUI implementation for interactive mode
            // Interactive mode: multi-turn chat with ask_user tools
            Command::Chat { message: None }
        };

        // Run the command
        run_command(
            command,
            &cwd,
            self.model.as_deref(),
            self.print,
            self.stream,
            self.verbose,
        )
        .await
    }
}

/// Display the help banner with ASCII art
pub fn print_banner(provider: &str, model: &str, directory: &std::path::Path) {
    println!();
    println!("  ╔═══╗  ╔═══╗");
    println!("  ║   ║  ║   ║");
    println!("  ║ O ║──║ W ║──╗");
    println!("  ║   ║  ║   ║  ║");
    println!("  ╚═══╝  ╚═══╝  ║");
    println!("                 ║");
    println!("  ┌──────────────╨─────────────────────────────────────┐");
    println!(
        "  │ \x1b[36;1m>_ Owly\x1b[0m \x1b[2mv{}\x1b[0m agent docs for codebases      │",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "  │ provider: \x1b[32m{provider}\x1b[0m{:<width$}│",
        "",
        width = 33 - provider.len()
    );
    println!(
        "  │ model: \x1b[32m{model}\x1b[0m{:<width$}│",
        "",
        width = 38 - model.len()
    );
    println!("  │ directory: {:<42}│", truncate_path(directory, 40));
    println!("  └────────────────────────────────────────────────────┘");
    println!();
}

/// Display a compact header for command execution
pub fn print_command_header(command: &str, provider: &str, model: &str) {
    println!();
    println!("\x1b[36;1m>_ Owly {command}\x1b[0m");
    println!("provider: \x1b[32m{provider}\x1b[0m");
    println!("model: \x1b[32m{model}\x1b[0m");
    println!();
}

/// Display agent status
pub fn print_agent_status(message: &str) {
    println!("\x1b[2m[status]\x1b[0m {message}");
}

/// Display tool call
pub fn print_tool_call(name: &str, verbose: bool) {
    if verbose {
        eprintln!("  \x1b[36m> {name}\x1b[0m");
    }
}

/// Display tool result
pub fn print_tool_result(name: &str, success: bool, verbose: bool) {
    if verbose {
        let icon = if success {
            "\x1b[32m✓\x1b[0m"
        } else {
            "\x1b[31m✗\x1b[0m"
        };
        eprintln!("  {icon} {name}");
    }
}

/// Display completion status
pub fn print_completion(message: &str) {
    println!();
    println!("\x1b[32;1m✓\x1b[0m {message}");
    println!();
}

/// Truncate a path for display
fn truncate_path(path: &std::path::Path, max_len: usize) -> String {
    let s = path.display().to_string();
    if s.len() <= max_len {
        s
    } else {
        format!("...{}", &s[s.len() - max_len + 3..])
    }
}
