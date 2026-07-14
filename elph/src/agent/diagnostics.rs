//! Diagnostics tool — gets errors and warnings for files or the project.
//!
//! This tool is specific to the elph coding agent and runs `cargo check` (or
//! similar lints) to surface compile-time diagnostics. It does not mutate the
//! workspace, so it belongs in the read-only / "Read & Search" tool group.


use elph_agent::AgentTool;
use elph_ai::Tool;
use serde_json::{Value, json};

/// Create the `diagnostics` tool, scoped to the given project root.
pub fn create_diagnostics_tool(cwd: &str) -> AgentTool {
    let project_root = cwd.to_string();
    elph_agent::simple_tool(
        Tool {
            name: "diagnostics".into(),
            description: "Gets errors and warnings for either a specific file or the entire project, useful after making edits to determine if further changes are needed.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional path to a specific file to check. Omit to check the entire project."
                    }
                }
            }),
        },
        "diagnostics",
        move |_, args| {
            let root = project_root.clone();
            Box::pin(async move { execute_diagnostics(root, args).await })
        },
    )
}

async fn execute_diagnostics(project_root: String, args: Value) -> anyhow::Result<elph_agent::AgentToolResult> {
    let path_filter = args.get("path").and_then(|v| v.as_str()).map(str::to_string);

    let mut cmd = tokio::process::Command::new("cargo");
    cmd.arg("check")
        .arg("--message-format=short")
        .current_dir(&project_root);

    let output = cmd
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run cargo check: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // `cargo check` writes diagnostics to stderr with `--message-format=short`.
    // Combine both streams so we capture everything.
    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }

    // Filter to a specific file if requested.
    let diagnostics = if let Some(ref filter) = path_filter {
        combined
            .lines()
            .filter(|line| line.contains(filter))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        // Strip the trailing "Finished" / "Checking" progress lines for cleanliness.
        combined
            .lines()
            .filter(|line| {
                !line.starts_with("    Checking")
                    && !line.starts_with("    Finished")
                    && !line.starts_with("    Updating")
                    && !line.starts_with("   Compiling")
                    && !line.starts_with("warning: generated")
                    && !line.starts_with("aborting due to")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let text = if diagnostics.trim().is_empty() {
        "No diagnostics found.".to_string()
    } else {
        diagnostics
    };

    let has_errors = output.status.code() != Some(0);
    Ok(elph_agent::AgentToolResult {
        content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "hasErrors": has_errors,
            "path": path_filter,
        }),
        added_tool_names: None,
        terminate: None,
    })
}
