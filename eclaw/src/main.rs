mod cli;
mod runtime;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();

    if cli.version {
        std::process::exit(cli::version::handle());
    }

    std::process::exit(cli::run(&cli));
}
