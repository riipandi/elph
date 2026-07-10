use clap::Args;

use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
pub struct CompletionsArgs {
    /// Shell type for completion script
    #[arg(short, long, value_name = "SHELL", default_value = "bash")]
    pub shell: String,
}

pub fn handle(args: &CompletionsArgs) -> ExitCode {
    tracing::warn!(shell = %args.shell, "Completions — not yet implemented");
    EXIT_SUCCESS
}
