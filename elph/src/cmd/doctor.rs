use clap::Args;

use crate::runtime::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
pub struct DoctorArgs {
    /// Emit machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}

pub fn handle(args: &DoctorArgs) -> ExitCode {
    tracing::warn!(json = args.json, "Doctor — not yet implemented");
    EXIT_SUCCESS
}
