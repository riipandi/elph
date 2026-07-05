use clap::Args;

use crate::runtime::{EXIT_SUCCESS, ExitCode};

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
    tracing::warn!(
        session = ?args.session,
        json = args.json,
        "Stats — not yet implemented"
    );
    EXIT_SUCCESS
}
