use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::RunArgs;

pub fn handle(args: &RunArgs) -> ExitCode {
    let prompt = args.prompt.join(" ");
    eprintln!(
        "Run — not yet implemented (prompt: {}, model: {:?}, format: {}, continue: {}, session: {:?}, fork: {}, files: {:?}, yolo: {})",
        if prompt.is_empty() { "<none>" } else { &prompt },
        args.model,
        args.output_format,
        args.r#continue,
        args.session,
        args.fork,
        args.files,
        args.yolo,
    );
    EXIT_SUCCESS
}
