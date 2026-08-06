use std::sync::Arc;

use anyhow::Context;
use serde_json::json;

use elph_ai::Tool;

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

use super::McpToolRegistry;
use super::expose_tool_name;

impl McpToolRegistry {

    /// Convert discovered MCP tools (+ resource/prompt bridge tools) into harness [`AgentTool`]s.
    ///
    /// For lazy-loaded registries, this triggers deferred discovery on first call.
    /// If discovery fails, already-discovered tools are still returned (graceful degradation).
    pub async fn create_agent_tools(self: &Arc<Self>) -> Vec<AgentTool> {
        // Try discovery, but don't let errors wipe out previously discovered tools.
        if let Err(error) = self.discover_tools().await {
            log::warn!("MCP lazy discovery failed; using already-discovered tools: {error}");
            // Continue with whatever tools we already have — don't return empty.
        }
        let mut out = Vec::new();

        for desc in self.tools.read().iter() {
            let registry = Arc::clone(self);
            let server = desc.server_name.clone();
            let tool_name = desc.tool_name.clone();
            out.push(simple_tool(
                Tool {
                    name: desc.exposed_name.clone(),
                    constrained_sampling: None,
                    description: desc.description.clone(),
                    parameters: desc.parameters.clone(),
                },
                format!("MCP:{}", desc.server_name),
                move |_, args| {
                    let registry = registry.clone();
                    let server = server.clone();
                    let tool_name = tool_name.clone();
                    Box::pin(async move { registry.call_tool(&server, &tool_name, args).await })
                },
            ));
        }

        // Bridge tools for resources / prompts per capable server.
        for server in self.resource_capable.read().iter() {
            out.push(self.bridge_list_resources(server));
            out.push(self.bridge_read_resource(server));
        }
        for server in self.prompt_capable.read().iter() {
            out.push(self.bridge_list_prompts(server));
            out.push(self.bridge_get_prompt(server));
        }
        for server in self.task_capable.read().iter() {
            out.push(self.bridge_tasks_get(server));
            out.push(self.bridge_tasks_update(server));
            out.push(self.bridge_tasks_cancel(server));
        }

        out
    }

    fn bridge_tasks_get(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "tasks_get");
        simple_tool(
            Tool {
                name,
                constrained_sampling: None,
                description: format!(
                    "[MCP:{server}] Poll SEP-2663 task status (tasks/get). Pass taskId from a prior tool result with resultType=task."
                ),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "taskId": { "type": "string", "description": "Task id from CreateTaskResult" }
                    },
                    "required": ["taskId"]
                }),
            },
            format!("MCP:{server}"),
            move |_, args| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let task_id = args
                        .get("taskId")
                        .and_then(|v| v.as_str())
                        .context("taskId is required")?
                        .to_string();
                    let Some(server_config) = registry.config.servers.get(&server).cloned() else {
                        anyhow::bail!("MCP server \"{server}\" not configured");
                    };
                    let result = registry.pool.get_task(&server, server_config, &task_id).await?;
                    let payload = serde_json::to_value(&result).unwrap_or_else(|_| json!({ "taskId": task_id }));
                    Ok(AgentToolResult::text(payload.to_string()))
                })
            },
        )
    }

    fn bridge_tasks_update(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "tasks_update");
        simple_tool(
            Tool {
                name,
                constrained_sampling: None,
                description: format!(
                    "[MCP:{server}] Deliver inputResponses for an in-task input_required state (tasks/update)."
                ),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "taskId": { "type": "string" },
                        "inputResponses": {
                            "type": "object",
                            "description": "Map of request keys to response values"
                        }
                    },
                    "required": ["taskId", "inputResponses"]
                }),
            },
            format!("MCP:{server}"),
            move |_, args| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let task_id = args
                        .get("taskId")
                        .and_then(|v| v.as_str())
                        .context("taskId is required")?
                        .to_string();
                    let responses = args
                        .get("inputResponses")
                        .cloned()
                        .context("inputResponses is required")?;
                    let input_responses: rmcp::model::InputResponses =
                        serde_json::from_value(responses).context("inputResponses must be a JSON object map")?;
                    let Some(server_config) = registry.config.servers.get(&server).cloned() else {
                        anyhow::bail!("MCP server \"{server}\" not configured");
                    };
                    registry
                        .pool
                        .update_task(&server, server_config, &task_id, input_responses)
                        .await?;
                    Ok(AgentToolResult::text(json!({ "ok": true, "taskId": task_id }).to_string()))
                })
            },
        )
    }

    fn bridge_tasks_cancel(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "tasks_cancel");
        simple_tool(
            Tool {
                name,
                constrained_sampling: None,
                description: format!("[MCP:{server}] Cancel a running task (tasks/cancel)."),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "taskId": { "type": "string" }
                    },
                    "required": ["taskId"]
                }),
            },
            format!("MCP:{server}"),
            move |_, args| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let task_id = args
                        .get("taskId")
                        .and_then(|v| v.as_str())
                        .context("taskId is required")?
                        .to_string();
                    let Some(server_config) = registry.config.servers.get(&server).cloned() else {
                        anyhow::bail!("MCP server \"{server}\" not configured");
                    };
                    registry.pool.cancel_task(&server, server_config, &task_id).await?;
                    Ok(AgentToolResult::text(
                        json!({ "ok": true, "taskId": task_id, "cancelled": true }).to_string(),
                    ))
                })
            },
        )
    }

    fn bridge_list_resources(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "list_resources");
        simple_tool(
            Tool {
                name,
                constrained_sampling: None,
                description: format!("[MCP:{server}] List resources available on this MCP server"),
                parameters: json!({ "type": "object", "properties": {} }),
            },
            format!("MCP:{server}"),
            move |_, _| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let items = registry.resources.read().clone();
                    let filtered: Vec<_> = items.into_iter().filter(|r| r.server_name == server).collect();
                    let payload = json!(
                        filtered
                            .iter()
                            .map(|r| json!({
                                "uri": r.uri,
                                "name": r.name,
                                "description": r.description,
                                "mimeType": r.mime_type,
                            }))
                            .collect::<Vec<_>>()
                    );
                    Ok(AgentToolResult::text(payload.to_string()))
                })
            },
        )
    }

    fn bridge_read_resource(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "read_resource");
        simple_tool(
            Tool {
                name,
                constrained_sampling: None,
                description: format!("[MCP:{server}] Read a resource by URI from this MCP server"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "uri": { "type": "string", "description": "Resource URI" }
                    },
                    "required": ["uri"]
                }),
            },
            format!("MCP:{server}"),
            move |_, args| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let uri = args
                        .get("uri")
                        .and_then(|v| v.as_str())
                        .context("uri is required")?
                        .to_string();
                    registry.read_resource(&server, &uri).await
                })
            },
        )
    }

    fn bridge_list_prompts(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "list_prompts");
        simple_tool(
            Tool {
                name,
                constrained_sampling: None,
                description: format!("[MCP:{server}] List prompt templates on this MCP server"),
                parameters: json!({ "type": "object", "properties": {} }),
            },
            format!("MCP:{server}"),
            move |_, _| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let items = registry.prompts.read().clone();
                    let filtered: Vec<_> = items.into_iter().filter(|p| p.server_name == server).collect();
                    let payload = json!(
                        filtered
                            .iter()
                            .map(|p| json!({
                                "name": p.name,
                                "description": p.description,
                                "arguments": p.arguments_schema,
                            }))
                            .collect::<Vec<_>>()
                    );
                    Ok(AgentToolResult::text(payload.to_string()))
                })
            },
        )
    }

    fn bridge_get_prompt(self: &Arc<Self>, server: &str) -> AgentTool {
        let registry = Arc::clone(self);
        let server_owned = server.to_string();
        let name = expose_tool_name(server, "get_prompt");
        simple_tool(
            Tool {
                name,
                constrained_sampling: None,
                description: format!("[MCP:{server}] Fetch a prompt template with optional arguments"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Prompt name" },
                        "arguments": { "type": "object", "description": "Prompt arguments" }
                    },
                    "required": ["name"]
                }),
            },
            format!("MCP:{server}"),
            move |_, args| {
                let registry = registry.clone();
                let server = server_owned.clone();
                Box::pin(async move {
                    let prompt_name = args
                        .get("name")
                        .and_then(|v| v.as_str())
                        .context("name is required")?
                        .to_string();
                    let arguments = args.get("arguments").cloned();
                    registry.get_prompt(&server, &prompt_name, arguments).await
                })
            },
        )
    }
}
