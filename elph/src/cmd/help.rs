use clap::CommandFactory;
use std::io::{IsTerminal, stderr};

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
    eprintln!("{}", format_warning(message));
}

fn format_warning(message: &str) -> String {
    if std::env::var("NO_COLOR").as_deref() == Ok("true") || !stderr().is_terminal() {
        message.to_string()
    } else {
        format!("\x1b[33m{message}\x1b[0m")
    }
}
