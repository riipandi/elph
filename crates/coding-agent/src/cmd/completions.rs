use clap::Args;

use crate::app::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
pub struct CompletionsArgs {
    /// Shell type for completion script
    #[arg(short, long, value_name = "SHELL", default_value = "bash")]
    pub shell: String,
}

pub fn handle(args: &CompletionsArgs) -> ExitCode {
    eprintln!("Completions — not yet implemented (shell: {})", args.shell);
    EXIT_SUCCESS
}
