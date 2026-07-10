use clap::Args;
use elph_ai::builtin_models;

use crate::platform::{EXIT_SUCCESS, ExitCode};

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
    let models = builtin_models(None).into_arc();
    let query = args.search.as_deref().map(|s| s.to_ascii_lowercase());

    for provider in models.get_providers() {
        if let Some(filter) = &args.provider
            && provider.id != *filter
        {
            continue;
        }
        for model in provider.get_models() {
            if let Some(q) = &query {
                let hay = format!("{} {} {}", provider.id, model.id, model.name).to_ascii_lowercase();
                if !hay.contains(q) {
                    continue;
                }
            }
            println!("{}/{}  {}", provider.id, model.id, model.name);
        }
    }

    EXIT_SUCCESS
}
