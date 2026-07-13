use clap::Args;

use crate::cli::help;
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
    help::unimplemented(&format!(
        "Stats — not yet implemented (session={:?}, json={})",
        args.session, args.json
    ));
    EXIT_SUCCESS
}
