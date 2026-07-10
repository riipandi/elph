//! MCP tool registry — discover remote tools and bridge them into the agent harness.

use std::sync::Arc;

use anyhow::{Context, Result};
use elph_ai::Tool;
use rmcp::model::{CallToolResult, ContentBlock, Tool as McpTool};
use serde_json::{Value, json};

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult, ToolResultContent};

use super::client::call_stdio_tool;
use super::client::list_tools;
use super::config::{McpConfig, McpServerConfig};

/// A discovered MCP tool ready for exposure to the model.
#[derive(Debug, Clone)]
pub struct McpToolDescriptor {
    pub server_name: String,
    pub tool_name: String,
    pub exposed_name: String,
    pub description: String,
    pub parameters: Value,
}

/// Registry of MCP servers and their discovered tools.
pub struct McpToolRegistry {
    config: McpConfig,
    tools: Vec<McpToolDescriptor>,
}

impl McpToolRegistry {
    pub fn empty() -> Self {
        Self {
            config: McpConfig::default(),
            tools: Vec::new(),
        }
    }

    pub async fn load(config: McpConfig) -> Result<Self> {
        let mut tools = Vec::new();
        for (server_name, server_config) in &config.servers {
            let discovered = discover_server_tools(server_name, server_config).await?;
            tools.extend(discovered);
        }
        Ok(Self { config, tools })
    }

    pub fn config(&self) -> &McpConfig {
        &self.config
    }

    pub fn descriptors(&self) -> &[McpToolDescriptor] {
        &self.tools
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    pub fn server_count(&self) -> usize {
        self.config.servers.len()
    }

    /// Convert discovered MCP tools into harness [`AgentTool`]s.
    pub fn create_agent_tools(self: &Arc<Self>) -> Vec<AgentTool> {
        self.tools
            .iter()
            .map(|desc| {
                let registry = Arc::clone(self);
                let server = desc.server_name.clone();
                let tool_name = desc.tool_name.clone();
                simple_tool(
                    Tool {
                        name: desc.exposed_name.clone(),
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
                )
            })
            .collect()
    }

    pub async fn call_tool(&self, server: &str, tool_name: &str, args: Value) -> Result<AgentToolResult> {
        let Some(server_config) = self.config.servers.get(server) else {
            anyhow::bail!("MCP server \"{server}\" not configured");
        };
        match server_config {
            McpServerConfig::Stdio(cfg) => {
                let result = call_stdio_tool(cfg, tool_name, args).await?;
                Ok(mcp_result_to_agent(result))
            }
        }
    }
}

async fn discover_server_tools(server_name: &str, config: &McpServerConfig) -> Result<Vec<McpToolDescriptor>> {
    match config {
        McpServerConfig::Stdio(cfg) => {
            let remote_tools = list_tools(cfg)
                .await
                .with_context(|| format!("list tools from MCP server \"{server_name}\""))?;
            Ok(remote_tools
                .into_iter()
                .map(|tool| descriptor_from_mcp(server_name, &tool))
                .collect())
        }
    }
}

fn descriptor_from_mcp(server_name: &str, tool: &McpTool) -> McpToolDescriptor {
    let tool_name = tool.name.to_string();
    let exposed_name = expose_tool_name(server_name, &tool_name);
    let description = tool
        .description
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| format!("MCP tool from server {server_name}"));
    let full_description = format!("[MCP:{server_name}] {description}");
    McpToolDescriptor {
        server_name: server_name.to_string(),
        tool_name,
        exposed_name,
        description: full_description,
        parameters: Value::Object((*tool.input_schema).clone()),
    }
}

fn expose_tool_name(server: &str, tool: &str) -> String {
    format!("mcp_{}__{}", sanitize_identifier(server), sanitize_identifier(tool))
}

fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn mcp_result_to_agent(result: CallToolResult) -> AgentToolResult {
    let is_error = result.is_error.unwrap_or(false);
    let mut content = Vec::new();

    for block in &result.content {
        match block {
            ContentBlock::Text(text) => {
                content.push(ToolResultContent::Text(elph_ai::TextContent::new(&text.text)));
            }
            ContentBlock::Image(image) => {
                content.push(ToolResultContent::Image(elph_ai::ImageContent::new(
                    &image.data,
                    &image.mime_type,
                )));
            }
            other => {
                if let Ok(text) = serde_json::to_string(other) {
                    content.push(ToolResultContent::Text(elph_ai::TextContent::new(text)));
                }
            }
        }
    }

    if content.is_empty() {
        if let Some(structured) = &result.structured_content {
            content.push(ToolResultContent::Text(elph_ai::TextContent::new(
                structured.to_string(),
            )));
        } else if is_error {
            content.push(ToolResultContent::Text(elph_ai::TextContent::new(
                "MCP tool returned an error with no content",
            )));
        } else {
            content.push(ToolResultContent::Text(elph_ai::TextContent::new(
                "MCP tool completed with no output",
            )));
        }
    }

    let mut agent_result = AgentToolResult {
        content,
        details: json!({
            "structured_content": result.structured_content,
            "is_error": is_error,
        }),
        terminate: None,
    };
    if is_error {
        agent_result = AgentToolResult::error(
            agent_result
                .content
                .iter()
                .filter_map(|c| match c {
                    ToolResultContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        agent_result.details = json!({
            "structured_content": result.structured_content,
            "is_error": true,
        });
    }
    agent_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expose_tool_name_sanitizes() {
        assert_eq!(expose_tool_name("my-server", "read/file"), "mcp_my_server__read_file");
    }
}
