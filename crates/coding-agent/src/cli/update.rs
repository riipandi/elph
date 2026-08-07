use clap::Args;

use super::style::{self, CliStyle, S_MUTED};
use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Args)]
pub struct UpdateArgs {
    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Emit machine-readable JSON output (for --check)
    #[arg(long)]
    pub json: bool,

    /// Force re-download and install even if already up to date
    #[arg(long)]
    pub force_reinstall: bool,

    /// Install a specific version (e.g. 0.0.0 or 0.0.0-canary)
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,

    /// Switch to the canary release channel (faster updates, may have bugs)
    #[arg(long)]
    pub canary: bool,

    /// Switch to the stable release channel (default, weekly releases)
    #[arg(long)]
    pub stable: bool,
}

pub fn handle(args: &UpdateArgs) -> ExitCode {
    let sty = CliStyle::auto();
    let mut out = String::new();

    style::section(&mut out, sty, "Update");
    style::info(&mut out, sty, sty.paint(S_MUTED, "Not yet implemented."));

    use std::fmt::Write;
    if args.check {
        let _ = writeln!(out);
        style::kv(&mut out, sty, "Check mode", "true");
    }
    if args.json {
        let _ = writeln!(out);
        style::kv(&mut out, sty, "JSON output", "true");
    }
    if args.force_reinstall {
        style::kv(&mut out, sty, "Force reinstall", "true");
    }
    if let Some(v) = &args.version {
        style::kv(&mut out, sty, "Version", v);
    }
    if args.canary {
        style::kv(&mut out, sty, "Channel", "canary");
    }
    if args.stable {
        style::kv(&mut out, sty, "Channel", "stable");
    }

    print!("{out}");
    EXIT_SUCCESS
}
