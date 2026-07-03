mod app;
mod cli;
mod commands;
mod components;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();

    if cli.version {
        std::process::exit(commands::version::handle());
    }

    let code = commands::run(&cli);
    std::process::exit(code);
}
