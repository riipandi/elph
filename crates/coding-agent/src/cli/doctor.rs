use clap::Args;

use crate::app::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
pub struct DoctorArgs {
    /// Emit machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}

pub fn handle(args: &DoctorArgs) -> ExitCode {
    eprintln!("Doctor — not yet implemented (json: {})", args.json);
    EXIT_SUCCESS
}
