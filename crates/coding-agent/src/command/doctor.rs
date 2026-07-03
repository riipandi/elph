use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::DoctorArgs;

pub fn handle(args: &DoctorArgs) -> ExitCode {
    eprintln!("Doctor — not yet implemented (json: {})", args.json);
    EXIT_SUCCESS
}
