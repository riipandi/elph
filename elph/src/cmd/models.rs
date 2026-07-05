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
    tracing::warn!(
        provider = args.provider.as_deref().unwrap_or("<all>"),
        search = ?args.search,
        "Models — not yet implemented"
    );
    EXIT_SUCCESS
}
