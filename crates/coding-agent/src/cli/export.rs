use clap::{Args, ValueEnum};

use super::style::{self, CliStyle, S_MUTED};
use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Args)]
pub struct ExportArgs {
    /// Session ID to export (exports most recent if omitted)
    #[arg(value_name = "SESSION_ID")]
    pub session_id: Option<String>,

    /// Output file path (default: stdout)
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<String>,

    /// Output format
    #[arg(long, value_name = "FORMAT", default_value = "json")]
    pub format: ExportFormat,

    /// Copy to clipboard instead of writing to stdout
    #[arg(short, long)]
    pub clipboard: bool,

    /// Redact sensitive transcript and file data
    #[arg(long)]
    pub sanitize: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ExportFormat {
    #[default]
    Json,
    Markdown,
    Zip,
}

pub fn handle(args: &ExportArgs) -> ExitCode {
    let sty = CliStyle::auto();
    let mut out = String::new();
    style::section(&mut out, sty, "Export");
    style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
    style::kv(&mut out, sty, "Session", args.session_id.as_deref().unwrap_or("<recent>"));
    style::kv(&mut out, sty, "Output", args.output.as_deref().unwrap_or("<stdout>"));
    style::kv(&mut out, sty, "Format", format!("{:?}", args.format));
    if args.clipboard {
        style::kv(&mut out, sty, "Clipboard", "true");
    }
    if args.sanitize {
        style::kv(&mut out, sty, "Sanitize", "true");
    }
    print!("{out}");
    EXIT_SUCCESS
}
