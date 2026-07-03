use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::StatsArgs;

pub fn handle(args: &StatsArgs) -> ExitCode {
    eprintln!(
        "Stats — not yet implemented (session: {:?}, json: {})",
        args.session, args.json
    );
    EXIT_SUCCESS
}
