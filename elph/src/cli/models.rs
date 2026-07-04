use clap::Args;

use crate::runtime::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
pub struct ModelsArgs {
    /// Filter models by provider name
    #[arg(value_name = "PROVIDER")]
    pub provider: Option<String>,

    /// Fuzzy search filter for model names
    #[arg(long, value_name = "QUERY")]
    pub search: Option<String>,
}

pub fn handle(args: &ModelsArgs) -> ExitCode {
    eprintln!(
        "Models — not yet implemented (provider: {}, search: {:?})",
        args.provider.as_deref().unwrap_or("<all>"),
        args.search,
    );
    EXIT_SUCCESS
}
