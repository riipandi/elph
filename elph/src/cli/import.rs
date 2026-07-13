use clap::Args;

use crate::platform::{EXIT_SUCCESS, ExitCode};

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
    log::warn!(
        "Import — not yet implemented (file={}, list={}, json={})",
        args.file.as_deref().unwrap_or("<none>"),
        args.list,
        args.json
    );
    EXIT_SUCCESS
}
