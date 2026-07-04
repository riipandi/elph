mod app;
mod cmd;
mod component;
mod exit_message;
mod interrupt;
mod keyboard_enhancement;
mod signal_interrupt;

use clap::Parser;

fn main() {
    let cli = cmd::Cli::parse();

    if cli.version {
        std::process::exit(cmd::version::handle());
    }

    let code = cmd::run(&cli);
    std::process::exit(code);
}
