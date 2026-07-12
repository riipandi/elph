//! Structured help text (OpenWiki `helpContent` parity, Owly-filtered).

pub struct HelpRow {
    pub label: &'static str,
    pub description: &'static str,
}

pub const DESCRIPTION: &str = "Run an agent that generates and maintains a project or local knowledge wiki.";

pub const USAGE: &[&str] = &[
    "owly code [--init|--update] [message]",
    "owly personal [--init|--update] [message]",
    "owly --mode <personal|code> [--init|--update] [message]",
    "owly [--modelId <model>]",
    "owly [--modelId <model>] [message]",
    "owly --update [message]",
    "owly auth list",
    "owly auth configure <provider> [--force]",
    "owly ingest <source|source-instance|all>",
    "owly cron list",
    "owly cron pause <source|all>",
    "owly cron resume <source|all>",
    "owly cron delete <source|all>",
];

pub const COMMANDS: &[HelpRow] = &[
    HelpRow {
        label: "owly code",
        description: "Run Owly for the current repository, writing docs under repo openwiki/ and optional GitHub Actions recurrence.",
    },
    HelpRow {
        label: "owly personal",
        description: "Run Owly as your local personal brain over configured sources, writing to ~/.owly/wiki.",
    },
    HelpRow {
        label: "owly",
        description: "Interactive mode placeholder (not yet implemented).",
    },
    HelpRow {
        label: "owly auth list",
        description: "List supported connector auth commands.",
    },
    HelpRow {
        label: "owly auth configure <provider>",
        description: "Create local connector config (git-repo, web-search, hackernews, or x with manual token).",
    },
    HelpRow {
        label: "owly ingest <source|source-instance|all>",
        description: "Run ingestion and wiki update for one connector, one source instance, or all configured sources.",
    },
    HelpRow {
        label: "owly cron list",
        description: "List saved connector schedules.",
    },
    HelpRow {
        label: "owly cron pause <source|all>",
        description: "Pause saved connector schedules.",
    },
    HelpRow {
        label: "owly cron resume <source|all>",
        description: "Resume paused connector schedules.",
    },
    HelpRow {
        label: "owly cron delete <source|all>",
        description: "Delete saved connector schedules.",
    },
];

pub const OPTIONS: &[HelpRow] = &[
    HelpRow {
        label: "--init",
        description: "Generate initial documentation for a selected mode. Use owly personal --init or owly code --init.",
    },
    HelpRow {
        label: "--update",
        description: "Update existing documentation and ingest configured connectors when relevant.",
    },
    HelpRow {
        label: "--mode <personal|code>",
        description: "Choose the personal brain (local wiki) or the code brain (repository docs).",
    },
    HelpRow {
        label: "-p, --print",
        description: "Run once and print the final assistant output (streaming off unless --stream).",
    },
    HelpRow {
        label: "-s, --stream",
        description: "Stream LLM text to stdout (default for chat without --print).",
    },
    HelpRow {
        label: "-v, --verbose",
        description: "Stream thinking to stderr in dimmed text.",
    },
    HelpRow {
        label: "--modelId <id>",
        description: "Use a model ID for this run (alias: --model).",
    },
    HelpRow {
        label: "--dry-run",
        description: "Show what would run without invoking the agent.",
    },
    HelpRow {
        label: "--credentials",
        description: "Print credential diagnostics and exit.",
    },
    HelpRow {
        label: "-d, --directory <path>",
        description: "Repository root for code mode (defaults to current directory).",
    },
];

pub const EXAMPLES: &[&str] = &[
    "owly",
    "owly personal --init",
    "owly code --init",
    "owly --update",
    "owly --update --mode personal",
    "owly \"What can you do?\"",
    "owly -p \"Summarize what Owly can do\"",
    "owly --modelId big-pickle",
    "owly --update --modelId big-pickle \"Please document the API routes first\"",
    "owly ingest all",
    "owly ingest web-search",
    "owly ingest hackernews",
    "owly cron list",
    "owly cron pause web-search",
    "owly cron resume web-search",
    "owly cron delete web-search",
    "owly auth configure git-repo",
    "owly --dry-run personal --update",
];

fn format_rows(rows: &[HelpRow]) -> String {
    let width = rows.iter().map(|r| r.label.len()).max().unwrap_or(0);
    rows.iter()
        .map(|row| format!("    {:width$}  {}", row.label, row.description, width = width))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Full product help (OpenWiki-style sections).
pub fn get_help_text() -> String {
    [
        "Owly".to_string(),
        format!("  {DESCRIPTION}"),
        String::new(),
        "Usage".to_string(),
        USAGE
            .iter()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        String::new(),
        "Commands".to_string(),
        format_rows(COMMANDS),
        String::new(),
        "Options".to_string(),
        format_rows(OPTIONS),
        String::new(),
        "Examples".to_string(),
        EXAMPLES
            .iter()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_includes_personal_and_ingest() {
        let text = get_help_text();
        assert!(text.contains("owly personal"));
        assert!(text.contains("owly ingest"));
        assert!(!text.contains("ngrok"));
        assert!(!text.contains("auth slack"));
        assert!(!text.contains("auth gmail"));
        assert!(!text.contains("auth notion"));
        assert!(!text.contains("owly auth x"));
    }
}
