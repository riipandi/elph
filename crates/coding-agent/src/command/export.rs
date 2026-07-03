use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::ExportArgs;

pub fn handle(args: &ExportArgs) -> ExitCode {
    eprintln!(
        "Export — not yet implemented (session: {}, output: {}, format: {:?}, clipboard: {}, sanitize: {})",
        args.session_id.as_deref().unwrap_or("<recent>"),
        args.output.as_deref().unwrap_or("<stdout>"),
        args.format,
        args.clipboard,
        args.sanitize,
    );
    EXIT_SUCCESS
}
