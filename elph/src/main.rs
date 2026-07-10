use anyhow::Result;
use clap::Parser;

use elph::cli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = cli::Cli::parse();

    if cli_args.version {
        std::process::exit(cli::version::handle());
    }

    let code = cli::run(&cli_args);
    std::process::exit(code);
}
