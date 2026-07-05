use clap::Args;

use crate::runtime::{EXIT_SUCCESS, ExitCode};

#[derive(Args)]
pub struct ImportArgs {
    /// Path to session file, directory, or share URL
    #[arg(value_name = "FILE")]
    pub file: Option<String>,

    /// List available sessions without importing
    #[arg(long)]
    pub list: bool,

    /// Emit NDJSON output to stdout
    #[arg(long)]
    pub json: bool,
}

pub fn handle(args: &ImportArgs) -> ExitCode {
    tracing::warn!(
        file = args.file.as_deref().unwrap_or("<none>"),
        list = args.list,
        json = args.json,
        "Import — not yet implemented"
    );
    EXIT_SUCCESS
}
