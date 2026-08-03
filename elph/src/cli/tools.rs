use std::sync::Arc;

use clap::Args;
use elph_agent::{BuiltinToolsBuilder, LocalExecutionEnv};

use super::style::{self, CliStyle, S_ACCENT, S_BODY, S_HEADER, S_MUTED, S_OK, S_TIP};
use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
pub struct ToolsArgs {
    /// Show tool parameters (JSON schema)
    #[arg(long)]
    pub verbose: bool,

    /// Filter by group: search, edit, web, collaboration, other
    #[arg(long, value_name = "GROUP")]
    pub group: Option<String>,
}

const GROUPS: &[(&str, &str, &[&str])] = &[
    (
        "Read & Search",
        "tools-search",
        &["read_file", "grep", "find_path", "list_dir", "diagnostics"],
    ),
    (
        "Edit",
        "tools-edit",
        &[
            "edit_file", "write_file", "shell_exec", "create_dir", "copy_path", "delete_path", "move_path",
        ],
    ),
    ("Web", "tools-web", &["web_search", "web_fetch"]),
    (
        "Collaboration",
        "tools-collaboration",
        &["ask_user_question", "spawn_agent", "send_message", "followup_task", "wait_agent", "list_agents"],
    ),
];

pub fn handle(args: &ToolsArgs) -> ExitCode {
    let sty = CliStyle::auto();
    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error: failed to get current directory: {e}");
            return 1;
        }
    };

    let env = Arc::new(LocalExecutionEnv::new(&cwd));
    let tools = BuiltinToolsBuilder::all(env).build();

    let tool_map: std::collections::HashMap<&str, (&str, &serde_json::Value)> = tools
        .iter()
        .map(|t| (t.tool.name.as_str(), (t.tool.description.as_str(), &t.tool.parameters)))
        .collect();

    let mcp_names: Vec<&str> = tool_map.keys().filter(|name| name.starts_with("mcp_")).copied().collect();
    let group_filter = args.group.as_deref().map(|s| s.to_ascii_lowercase());

    let mut out = String::new();
    let mut total_shown = 0usize;

    for (group_name, feature, expected_names) in GROUPS {
        if let Some(filter) = &group_filter {
            let matches_group = group_name.to_ascii_lowercase().contains(filter.as_str())
                || feature.to_ascii_lowercase().contains(filter.as_str());
            if !matches_group {
                continue;
            }
        }

        let available: Vec<&&str> = expected_names.iter().filter(|name| tool_map.contains_key(**name)).collect();
        if available.is_empty() {
            continue;
        }

        use std::fmt::Write;
        let _ = writeln!(out);
        style::section(&mut out, sty, group_name);

        for name in &available {
            if let Some((desc, _params)) = tool_map.get(**name) {
                let short_desc = desc.split_once(". ").map(|(first, _)| first).unwrap_or(desc);
                let short_desc = if short_desc.len() > 100 {
                    format!("{}…", &short_desc[..97])
                } else {
                    short_desc.to_string()
                };
                let _ = writeln!(
                    out,
                    "  {}  {}",
                    sty.paint(S_ACCENT, format!("{name:<24}")),
                    sty.paint(S_MUTED, short_desc),
                );
                total_shown += 1;
            }
        }
    }

    // MCP tools
    let show_mcp = group_filter.as_ref().map(|f| f.contains("other") || f.contains("mcp")).unwrap_or(true);
    if show_mcp && !mcp_names.is_empty() {
        use std::fmt::Write;
        let _ = writeln!(out);
        style::section(&mut out, sty, "MCP tools");

        let mut by_server: std::collections::BTreeMap<String, Vec<&str>> = std::collections::BTreeMap::new();
        for name in &mcp_names {
            let server = name
                .strip_prefix("mcp_")
                .and_then(|s| s.split_once("__"))
                .map(|(server, _)| server)
                .unwrap_or("unknown")
                .to_string();
            by_server.entry(server).or_default().push(name);
        }

        for (server, names) in &by_server {
            let _ = writeln!(out, "  {} {}", sty.paint(S_HEADER, "Server:"), sty.paint(S_BODY, server));
            for name in names {
                if let Some((desc, _params)) = tool_map.get(name) {
                    let tool_short = name.strip_prefix("mcp_").unwrap_or(name);
                    let short_desc = desc.split_once(". ").map(|(first, _)| first).unwrap_or(desc);
                    let short_desc = if short_desc.len() > 80 {
                        format!("{}…", &short_desc[..77])
                    } else {
                        short_desc.to_string()
                    };
                    let _ = writeln!(
                        out,
                        "    {}  {}",
                        sty.paint(S_ACCENT, format!("{tool_short:<28}")),
                        sty.paint(S_MUTED, short_desc),
                    );
                    total_shown += 1;
                }
            }
        }
    }

    // Meta tools
    let show_meta = group_filter.as_ref().map(|f| f.contains("other") || f.contains("meta")).unwrap_or(true);
    if show_meta && tool_map.contains_key("list_available_tools") {
        use std::fmt::Write;
        let _ = writeln!(out);
        style::section(&mut out, sty, "Meta");
        let _ = writeln!(
            out,
            "  {}  {}",
            sty.paint(S_ACCENT, format!("{:<24}", "list_available_tools")),
            sty.paint(S_MUTED, "Lists all available tools with descriptions and parameters"),
        );
        total_shown += 1;
    }

    use std::fmt::Write;
    let _ = writeln!(out);
    style::kv(&mut out, sty, "Total", format!("{total_shown} tools registered"));

    if group_filter.is_some() && total_shown == 0 {
        let _ = writeln!(out);
        style::tip(&mut out, sty, "No tools matched the filter. Available groups: search, edit, web, collaboration, other");
    }

    print!("{out}");
    EXIT_SUCCESS
}
