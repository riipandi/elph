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
            // Interactive mode - TODO: implement interactive CLI
            eprintln!("Interactive mode is not yet implemented.");
            eprintln!("Use --init, --update, or provide a message.");
            eprintln!("Example: owly --init");
            eprintln!("Example: owly \"What can you do?\"");
            std::process::exit(1);
        };

        // Run the command
        run_command(command, &cwd, self.model.as_deref(), self.print, self.verbose).await
    }
}

/// Display a compact header for command execution
pub fn print_command_header(command: &str, provider: &str, model: &str) {
    println!();
    println!("\x1b[1m\x1b[36m>_\x1b[0m \x1b[1mOwly {command}\x1b[0m");
    println!("\x1b[90m  Model: {provider}/{model}\x1b[0m");
    println!();
}

/// Display agent status
pub fn print_agent_status(message: &str) {
    println!("\x1b[90m  {message}\x1b[0m");
}

/// Display tool call
pub fn print_tool_call(name: &str, verbose: bool) {
    if verbose {
        println!("\x1b[36m  > {name}\x1b[0m");
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
        println!("  {icon} {name}");
    }
}

/// Display completion status
pub fn print_completion(message: &str) {
    println!();
    println!("\x1b[32m✓\x1b[0m {message}");
    println!();
}
