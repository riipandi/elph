use clap::CommandFactory;

use crate::runtime::{EXIT_ERROR, EXIT_SUCCESS, ExitCode};

pub fn print_subcommand_help<T: CommandFactory>() -> ExitCode {
    let mut cmd = T::command();
    if cmd.print_help().is_err() {
        return EXIT_ERROR;
    }
    println!();
    EXIT_SUCCESS
}

pub fn unimplemented(message: &str) {
    tracing::warn!("{message}");
}
