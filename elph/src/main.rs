mod cli;
mod runtime;
mod ui;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();

    if cli.version {
        std::process::exit(cli::version::handle());
    }

    let code = cli::run(&cli);
    std::process::exit(code);
}
