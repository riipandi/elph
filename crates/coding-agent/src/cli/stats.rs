use clap::Args;

use super::style::{self, CliStyle, S_MUTED};
use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
pub struct StatsArgs {
    /// Filter statistics to a specific session
    #[arg(long, value_name = "SESSION_ID")]
    pub session: Option<String>,

    /// Emit machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}

pub fn handle(args: &StatsArgs) -> ExitCode {
    let sty = CliStyle::auto();
    let mut out = String::new();
    style::section(&mut out, sty, "Statistics");
    style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
    if let Some(s) = &args.session {
        style::kv(&mut out, sty, "Session", s);
    }
    if args.json {
        style::kv(&mut out, sty, "JSON output", "true");
    }
    print!("{out}");
    EXIT_SUCCESS
}
