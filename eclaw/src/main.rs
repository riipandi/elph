mod cmd;
mod layout;
mod runtime;
mod server;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cmd::Cli::parse();

    if cli.version {
        std::process::exit(cmd::version::handle());
    }

    std::process::exit(cmd::run(&cli));
}
