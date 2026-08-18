use clap::CommandFactory;

use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode};

pub fn print_subcommand_help<T: CommandFactory>() -> ExitCode {
    let mut cmd = T::command();
    if cmd.print_help().is_err() {
        return EXIT_ERROR;
    }
    println!();
    EXIT_SUCCESS
}

/// User-facing stub message (stdout, no log formatting).
pub fn unimplemented(message: &str) {
    log::warn!("cli unimplemented: {message}");
    println!("{message}");
}

/// User-facing error (stderr). Also written to the process JSONL log.
pub fn cli_error(message: impl std::fmt::Display) {
    log::error!("{message}");
    eprintln!("error: {message}");
}
