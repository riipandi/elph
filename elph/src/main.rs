mod app;
mod cmd;
mod coding_agent;
mod command;
mod config;
mod memory;
mod plugins;
mod prompt;
mod runtime;
mod skills;
mod tui;
mod widget;
mod worktree;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cmd::Cli::parse();

    if cli.version {
        std::process::exit(cmd::version::handle());
    }

    let code = cmd::run(&cli);
    std::process::exit(code);
}
