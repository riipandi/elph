//! Keep tool surface and `list_available_tools` catalog aligned with agent mode.
//!
//! MCP tools are **registered** on the harness (executable once active) but
//! **default-inactive**. The `list_available_tools` catalog snapshot includes
//! inactive MCP tools so the model can discover and activate them via
//! `name_prefix` + `added_tool_names`.

use anyhow::Result;
use elph_agent::create_list_available_tools;
use elph_agent::create_list_skills_tool;
use elph_agent::{AgentHarness, McpToolRegistry, TursoSessionStorage, is_mcp_tool};

use crate::types::AgentMode;

use super::tool_policy::AgentModePolicy;

/// Rebuild the `list_available_tools` meta tool.
///
/// The catalog snapshot is the **full registry** (minus nested meta tools), not
/// only the active set — so default-inactive MCP tools remain discoverable.
/// `active_names` controls which tools are sent to the model API this turn.
pub async fn refresh_tools_catalog(harness: &AgentHarness<TursoSessionStorage>, active_names: &[String]) -> Result<()> {
    let mut tools = harness.get_tools().await;

    // Full registry for discovery (including default-inactive MCP tools).
    let snapshot: Vec<_> = tools
        .iter()
        .filter(|tool| {
            let name = tool.name();
            name != "list_available_tools" && name != "list_skills"
        })
        .cloned()
        .collect();

    // Rebuild `list_skills` from the live resource set so a late/lazy skill load
    // is visible (the previous snapshot was frozen at tool-build time).
    tools.retain(|tool| tool.name() != "list_available_tools" && tool.name() != "list_skills");
    let skills = harness.get_resources().await.skills;
    tools.push(create_list_skills_tool(skills));
    tools.push(create_list_available_tools(&snapshot));

    let active_opt = if active_names.is_empty() {
        None
    } else {
        let mut names = active_names.to_vec();
        if !names.iter().any(|n| n == "list_available_tools") {
            names.push("list_available_tools".into());
        }
        if !names.iter().any(|n| n == "list_skills") {
            names.push("list_skills".into());
        }
        Some(names)
    };

    harness
        .set_tools(tools, active_opt)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

/// Merge already-active MCP tools that remain registered and mode-allowed.
fn preserve_activated_mcp(
    mode: AgentMode,
    base_active: &[String],
    currently_active: &[String],
    all_registered: &[String],
    mcp_registry: Option<&McpToolRegistry>,
) -> Vec<String> {
    let mut active = base_active.to_vec();
    for name in currently_active {
        if !is_mcp_tool(name) {
            continue;
        }
        if !all_registered.iter().any(|n| n == name) {
            continue;
        }
        if !AgentModePolicy::mcp_allowed_in_mode(mode, name, mcp_registry) {
            continue;
        }
        if !active.iter().any(|n| n == name) {
            active.push(name.clone());
        }
    }
    active.sort();
    active.dedup();
    active
}

/// Apply agent-mode tool permissions to the harness and refresh the meta-tool catalog.
pub async fn reconcile_harness_tools(
    harness: &AgentHarness<TursoSessionStorage>,
    mode: AgentMode,
    mcp_registry: Option<&McpToolRegistry>,
) -> Result<()> {
    let all_registered: Vec<String> = harness
        .get_tools()
        .await
        .into_iter()
        .map(|tool| tool.name().to_string())
        .collect();
    let currently_active: Vec<String> = harness
        .get_active_tools()
        .await
        .into_iter()
        .map(|tool| tool.name().to_string())
        .collect();

    let base = AgentModePolicy::active_tool_names_for_mode(mode, &all_registered, mcp_registry);
    let active = preserve_activated_mcp(mode, &base, &currently_active, &all_registered, mcp_registry);

    match mode {
        AgentMode::Plan => {
            // Collaboration mode rewrite uses baseline (no MCP by default); re-apply
            // our active list so lazily activated, mode-allowed MCP tools survive.
            harness.enter_plan_mode().await.map_err(|e| anyhow::anyhow!("{e}"))?;
            harness
                .set_active_tools(active.clone())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        AgentMode::Build | AgentMode::Brave | AgentMode::Ask => {
            harness.exit_plan_mode().await.map_err(|e| anyhow::anyhow!("{e}"))?;
            harness
                .set_active_tools(active.clone())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
    }

    refresh_tools_catalog(harness, &active).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserve_activated_mcp_keeps_allowed_only() {
        let base = vec!["read_file".into(), "list_available_tools".into()];
        // `…__read_…` / `…__list_…` patterns are read-only in Plan/Ask.
        let currently = vec![
            "read_file".into(),
            "mcp_deepwiki__read_wiki_structure".into(),
            "mcp_fs__write_file".into(),
        ];
        let all = vec![
            "read_file".into(),
            "mcp_deepwiki__read_wiki_structure".into(),
            "mcp_fs__write_file".into(),
            "list_available_tools".into(),
        ];

        let build = preserve_activated_mcp(AgentMode::Build, &base, &currently, &all, None);
        assert!(build.contains(&"mcp_deepwiki__read_wiki_structure".to_string()));
        assert!(build.contains(&"mcp_fs__write_file".to_string()));

        let plan = preserve_activated_mcp(AgentMode::Plan, &base, &currently, &all, None);
        assert!(plan.contains(&"mcp_deepwiki__read_wiki_structure".to_string()));
        assert!(!plan.contains(&"mcp_fs__write_file".to_string()));
    }
}
