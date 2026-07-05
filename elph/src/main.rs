mod app;
mod cmd;
mod command;
mod config;
mod datastore;
mod logger;
mod memory;
mod plugins;
mod prompt;
mod runtime;
mod session;
mod skills;
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
