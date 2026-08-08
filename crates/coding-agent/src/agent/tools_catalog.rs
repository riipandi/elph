//! Keep tool surface and `list_available_tools` catalog aligned with agent mode.

use std::collections::HashSet;

use anyhow::Result;
use elph_agent::create_list_available_tools;
use elph_agent::{AgentHarness, CollaborationMode, McpToolRegistry, TursoSessionStorage};

use crate::types::AgentMode;

use super::tool_policy::AgentModePolicy;

/// Rebuild the `list_available_tools` meta tool so its catalog matches `active_names`.
pub async fn refresh_tools_catalog(harness: &AgentHarness<TursoSessionStorage>, active_names: &[String]) -> Result<()> {
    let mut tools = harness.get_tools().await;
    let active_set: HashSet<&str> = active_names.iter().map(String::as_str).collect();

    let snapshot: Vec<_> = tools
        .iter()
        .filter(|tool| {
            let name = tool.name();
            name != "list_available_tools"
                && name != "list_skills"
                && (active_names.is_empty() || active_set.contains(name))
        })
        .cloned()
        .collect();

    // Keep `list_skills` available (always-active catalog tool); it is excluded
    // from the `list_available_tools` snapshot so the two catalogs don't nest.
    tools.retain(|tool| tool.name() != "list_available_tools");
    if !tools.iter().any(|tool| tool.name() == "list_skills")
        && let Some(skills_tool) = harness
            .get_tools()
            .await
            .into_iter()
            .find(|tool| tool.name() == "list_skills")
    {
        tools.push(skills_tool);
    }
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

    let active = AgentModePolicy::active_tool_names_for_mode(mode, &all_registered, mcp_registry);

    match mode {
        AgentMode::Plan => {
            harness.enter_plan_mode().await.map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        AgentMode::Build | AgentMode::Brave | AgentMode::Ask => {
            harness
                .set_collaboration_mode(CollaborationMode::Default)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            harness
                .set_active_tools(active.clone())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
    }

    let active_after = if mode == AgentMode::Plan {
        harness
            .get_active_tools()
            .await
            .into_iter()
            .map(|tool| tool.name().to_string())
            .collect()
    } else {
        active
    };

    refresh_tools_catalog(harness, &active_after).await
}
