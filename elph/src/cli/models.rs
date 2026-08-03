use clap::Args;
use elph_ai::builtin_models;

use super::style::{self, CliStyle, S_ACCENT, S_BODY, S_MUTED};
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
    let sty = CliStyle::auto();
    let models = builtin_models(None).into_arc();
    let query = args.search.as_deref().map(|s| s.to_ascii_lowercase());

    let mut out = String::new();
    let mut count = 0usize;

    for provider in models.get_providers() {
        if let Some(filter) = &args.provider
            && provider.id != *filter
        {
            continue;
        }
        let mut provider_shown = false;
        for model in provider.get_models() {
            if let Some(q) = &query {
                let hay = format!("{} {} {}", provider.id, model.id, model.name).to_ascii_lowercase();
                if !hay.contains(q) {
                    continue;
                }
            }
            if !provider_shown {
                use std::fmt::Write;
                let _ = writeln!(out);
                style::section(&mut out, sty, &format!("Provider: {}", provider.id));
                provider_shown = true;
            }
            use std::fmt::Write;
            let _ = writeln!(
                out,
                "  {}  {}",
                sty.paint(S_ACCENT, format!("{:<24}", model.id)),
                sty.paint(S_MUTED, &model.name),
            );
            count += 1;
        }
    }

    if count == 0 {
        style::info(&mut out, sty, sty.paint(S_MUTED, "No models matched."));
    } else {
        use std::fmt::Write;
        let _ = writeln!(out);
        style::kv(&mut out, sty, "Total", format!("{count} models"));
    }

    print!("{out}");
    EXIT_SUCCESS
}
