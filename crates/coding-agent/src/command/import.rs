use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::ImportArgs;

pub fn handle(args: &ImportArgs) -> ExitCode {
    eprintln!(
        "Import — not yet implemented (file: {}, list: {}, json: {})",
        args.file.as_deref().unwrap_or("<none>"),
        args.list,
        args.json,
    );
    EXIT_SUCCESS
}
