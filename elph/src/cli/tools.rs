use std::sync::Arc;

use clap::Args;
use elph_agent::{BuiltinToolsBuilder, LocalExecutionEnv};

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

/// Tool group definitions — name, feature, and the tool names that belong to each.
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
            "edit_file",
            "write_file",
            "shell_exec",
            "create_dir",
            "copy_path",
            "delete_path",
            "move_path",
        ],
    ),
    ("Web", "tools-web", &["web_search", "web_fetch"]),
    (
        "Collaboration",
        "tools-collaboration",
        &[
            "ask_user_question",
            "spawn_agent",
            "send_message",
            "followup_task",
            "wait_agent",
            "list_agents",
        ],
    ),
];

pub fn handle(args: &ToolsArgs) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error: failed to get current directory: {e}");
            return 1;
        }
    };

    let env = Arc::new(LocalExecutionEnv::new(&cwd));
    let tools = BuiltinToolsBuilder::all(env).build();

    // Build a lookup map: name → (description, parameters)
    let tool_map: std::collections::HashMap<&str, (&str, &serde_json::Value)> = tools
        .iter()
        .map(|t| (t.tool.name.as_str(), (t.tool.description.as_str(), &t.tool.parameters)))
        .collect();

    // Collect MCP tool names (prefix mcp_)
    let mcp_names: Vec<&str> = tool_map
        .keys()
        .filter(|name| name.starts_with("mcp_"))
        .copied()
        .collect();

    // Determine which groups to show
    let group_filter = args.group.as_deref().map(|s| s.to_ascii_lowercase());

    // ── Print grouped tools ──
    let mut total_shown = 0usize;

    for (group_name, feature, expected_names) in GROUPS {
        if let Some(filter) = &group_filter {
            let matches_group = group_name.to_ascii_lowercase().contains(filter.as_str())
                || feature.to_ascii_lowercase().contains(filter.as_str());
            if !matches_group {
                continue;
            }
        }

        // Collect tools that are actually registered
        let available: Vec<&&str> = expected_names
            .iter()
            .filter(|name| tool_map.contains_key(**name))
            .collect();

        if available.is_empty() {
            continue;
        }

        println!();
        println!("  {group_name} ({feature})");
        println!("  {}", "-".repeat(group_name.len() + feature.len() + 3));

        for name in &available {
            if let Some((desc, params)) = tool_map.get(**name) {
                // Truncate description to first sentence or 100 chars
                let short_desc = desc.split_once(". ").map(|(first, _)| first).unwrap_or(desc);
                let short_desc = if short_desc.len() > 100 {
                    format!("{}...", &short_desc[..97])
                } else {
                    short_desc.to_string()
                };
                println!("    {:<24} {}", name, short_desc);

                if args.verbose {
                    print_params(params, "      ");
                }
                total_shown += 1;
            }
        }
    }

    // ── MCP tools ──
    let show_mcp = group_filter
        .as_ref()
        .map(|f| f.contains("other") || f.contains("mcp"))
        .unwrap_or(true);

    if show_mcp && !mcp_names.is_empty() {
        println!();
        println!("  Other (mcp)");
        println!("  -----------");

        // Group by server prefix
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
            println!("    Server: {server}");
            for name in names {
                if let Some((desc, params)) = tool_map.get(name) {
                    let tool_short = name.strip_prefix("mcp_").unwrap_or(name);
                    let short_desc = desc.split_once(". ").map(|(first, _)| first).unwrap_or(desc);
                    let short_desc = if short_desc.len() > 80 {
                        format!("{}...", &short_desc[..77])
                    } else {
                        short_desc.to_string()
                    };
                    println!("      {:<30} {}", tool_short, short_desc);
                    if args.verbose {
                        print_params(params, "        ");
                    }
                    total_shown += 1;
                }
            }
        }
    }

    // ── Meta tools ──
    let show_meta = group_filter
        .as_ref()
        .map(|f| f.contains("other") || f.contains("meta"))
        .unwrap_or(true);

    if show_meta && tool_map.contains_key("list_available_tools") {
        println!();
        println!("  Meta");
        println!("  ----");
        println!(
            "    {:<24} Lists all available tools with descriptions and parameters",
            "list_available_tools"
        );
        if args.verbose {
            println!("      (no parameters)");
        }
        total_shown += 1;
    }

    // ── Summary ──
    println!();
    println!("  Total: {total_shown} tools registered");

    if group_filter.is_some() && total_shown == 0 {
        println!();
        println!("  No tools matched the filter. Available groups: search, edit, web, collaboration, other");
    }

    EXIT_SUCCESS
}

fn print_params(params: &serde_json::Value, indent: &str) {
    if let Some(props) = params.get("properties").and_then(|v| v.as_object()) {
        let required: Vec<String> = params
            .get("required")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        for (name, schema) in props {
            let type_str = schema.get("type").and_then(|v| v.as_str()).unwrap_or("any");
            let desc = schema.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let req = if required.contains(&name.to_string()) {
                " (required)"
            } else {
                ""
            };
            println!("{indent}{name}: {type_str}{req} — {desc}");
        }
    }
}
