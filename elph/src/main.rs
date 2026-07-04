mod cmd;
mod layout;
mod runtime;
mod ui;

use clap::Parser;

fn main() {
    let cli = cmd::Cli::parse();

    if cli.version {
        std::process::exit(cmd::version::handle());
    }

    let code = cmd::run(&cli);
    std::process::exit(code);
}
