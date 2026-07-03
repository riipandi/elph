use clap::Args;

#[derive(Args, Default)]
pub struct ModelsArgs {
    /// Filter models by provider name
    #[arg(value_name = "PROVIDER")]
    pub provider: Option<String>,

    /// Fuzzy search filter for model names
    #[arg(long, value_name = "QUERY")]
    pub search: Option<String>,
}
