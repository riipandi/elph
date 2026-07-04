use clap::Args;

use crate::runtime::{EXIT_SUCCESS, ExitCode};

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
    println!("Update — not yet implemented");
    if args.check {
        println!("  --check: true");
    }
    if args.json {
        println!("  --json: true");
    }
    if args.force_reinstall {
        println!("  --force-reinstall: true");
    }
    if let Some(v) = &args.version {
        println!("  --version: {v}");
    }
    if args.canary {
        println!("  --canary: true");
    }
    if args.stable {
        println!("  --stable: true");
    }
    EXIT_SUCCESS
}
