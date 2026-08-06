use clap::Args;

use super::style::{self, CliStyle, S_MUTED};
use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
pub struct DoctorArgs {
    /// Emit machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}

pub fn handle(args: &DoctorArgs) -> ExitCode {
    let sty = CliStyle::auto();
    let mut out = String::new();
    style::section(&mut out, sty, "Doctor");
    style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));
    if args.json {
        style::kv(&mut out, sty, "JSON output", "true");
    }
    print!("{out}");
    EXIT_SUCCESS
}
