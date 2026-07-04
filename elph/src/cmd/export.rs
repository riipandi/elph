use clap::{Args, ValueEnum};

use crate::runtime::{EXIT_SUCCESS, ExitCode};

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
    eprintln!(
        "Export — not yet implemented (session: {}, output: {}, format: {:?}, clipboard: {}, sanitize: {})",
        args.session_id.as_deref().unwrap_or("<recent>"),
        args.output.as_deref().unwrap_or("<stdout>"),
        args.format,
        args.clipboard,
        args.sanitize,
    );
    EXIT_SUCCESS
}
