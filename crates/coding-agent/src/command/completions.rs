use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::CompletionsArgs;

pub fn handle(args: &CompletionsArgs) -> ExitCode {
    eprintln!("Completions — not yet implemented (shell: {})", args.shell);
    EXIT_SUCCESS
}
