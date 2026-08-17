use std::fmt::Write;

use clap::Args;
use elph_ai::{Model, Provider, builtin_models};

use super::style::{self, CliStyle, S_MUTED};
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

    // Collect filtered (provider, model) rows, preserving first-seen provider order.
    let mut rows: Vec<(&Provider, Model)> = Vec::new();
    for provider in models.get_providers() {
        let provider_matches = match &args.provider {
            Some(filter) => provider.id == *filter || provider.name.eq_ignore_ascii_case(filter),
            None => true,
        };
        if !provider_matches {
            continue;
        }
        for model in provider.get_models() {
            if let Some(q) = &query {
                let hay = format!("{} {} {}", provider.id, model.id, model.name).to_ascii_lowercase();
                if !hay.contains(q) {
                    continue;
                }
            }
            rows.push((provider, model));
        }
    }

    let mut out = String::new();
    style::section(&mut out, sty, "Models");

    if rows.is_empty() {
        let _ = writeln!(out);
        style::info(&mut out, sty, sty.paint(S_MUTED, "No models matched."));
        let mut filters = Vec::new();
        if let Some(p) = &args.provider {
            filters.push(format!("provider={p}"));
        }
        if let Some(s) = &args.search {
            filters.push(format!("search={s}"));
        }
        if !filters.is_empty() {
            style::tip(&mut out, sty, format!("Filters: {}", filters.join(", ")));
        }
        print!("{out}");
        return EXIT_SUCCESS;
    }

    // Providers in first-seen order (for stable section grouping).
    let mut ordered: Vec<&Provider> = Vec::new();
    for (p, _) in &rows {
        if !ordered.iter().any(|x| x.id == p.id) {
            ordered.push(p);
        }
    }

    style::kv(&mut out, sty, "Providers", ordered.len());
    style::kv(&mut out, sty, "Models", rows.len());
    if let Some(s) = &args.search {
        style::kv(&mut out, sty, "Query", s);
    }

    let _ = writeln!(out);
    for provider in ordered {
        let pname = if provider.name.is_empty() {
            &provider.id
        } else {
            &provider.name
        };
        style::section(&mut out, sty, &format!("{pname} ({})", provider.id));

        let group: Vec<&Model> = rows
            .iter()
            .filter(|(p, _)| p.id == provider.id)
            .map(|(_, m)| m)
            .collect();
        let name_w = group
            .iter()
            .map(|m| m.name.chars().count())
            .max()
            .unwrap_or(0)
            .clamp(8, 44);
        let id_w = group
            .iter()
            .map(|m| m.id.chars().count())
            .max()
            .unwrap_or(0)
            .clamp(8, 44);

        for model in group {
            let id_painted = sty.paint(S_MUTED, format!("{:<width$}", model.id, width = id_w));
            let _ = writeln!(
                out,
                "  {name:<width$}  {id}  {spec}",
                name = model.name,
                width = name_w,
                id = id_painted,
                spec = sty.paint(S_MUTED, model_spec(model)),
            );
        }
    }

    print!("{out}");
    EXIT_SUCCESS
}

/// Compact, human-readable model summary: context window + per-M token price.
fn model_spec(m: &Model) -> String {
    let ctx = if m.context_window >= 1_000_000 {
        format!("{:.1}M", m.context_window as f64 / 1_000_000.0)
    } else if m.context_window >= 1000 {
        format!("{}k", m.context_window / 1000)
    } else {
        m.context_window.to_string()
    };
    let price = if m.cost.input == 0.0 && m.cost.output == 0.0 {
        "free".to_string()
    } else {
        format!("${:.2}/${:.2}", m.cost.input, m.cost.output)
    };
    let mut s = format!("· {} ctx · {} per M", ctx, price);
    if m.reasoning {
        s.push_str(" · reasoning");
    }
    s
}
