use clap::Args;

use super::style::{self, CliStyle, S_MUTED};
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
    let sty = CliStyle::auto();
    let mut out = String::new();
    style::section(&mut out, sty, "Import");
    style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
    style::kv(&mut out, sty, "File", args.file.as_deref().unwrap_or("<none>"));
    if args.list {
        style::kv(&mut out, sty, "List mode", "true");
    }
    if args.json {
        style::kv(&mut out, sty, "JSON output", "true");
    }
    print!("{out}");
    EXIT_SUCCESS
}
