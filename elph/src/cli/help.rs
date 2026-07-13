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
    println!("{message}");
}

/// User-facing error (stderr, no log formatting).
pub fn cli_error(message: impl std::fmt::Display) {
    eprintln!("error: {message}");
}
