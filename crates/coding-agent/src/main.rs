mod app;
mod cli;
mod command;
mod component;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();

    if cli.version {
        std::process::exit(command::version::handle());
    }

    let code = command::run(&cli);
    std::process::exit(code);
}
