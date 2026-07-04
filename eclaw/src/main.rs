mod cmd;
mod layout;
mod runtime;
mod server;

use clap::Parser;

fn main() {
    let cli = cmd::Cli::parse();

    if cli.version {
        std::process::exit(cmd::version::handle());
    }

    std::process::exit(cmd::run(&cli));
}
