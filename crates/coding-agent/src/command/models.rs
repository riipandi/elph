use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::ModelsArgs;

pub fn handle(args: &ModelsArgs) -> ExitCode {
    eprintln!(
        "Models — not yet implemented (provider: {}, search: {:?})",
        args.provider.as_deref().unwrap_or("<all>"),
        args.search,
    );
    EXIT_SUCCESS
}
