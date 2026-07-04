mod app;
mod cli;
mod component;
mod exit_message;
mod interrupt;
mod keyboard_enhancement;
mod signal_interrupt;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();

    if cli.version {
        std::process::exit(cli::version::handle());
    }

    let code = cli::run(&cli);
    std::process::exit(code);
}
