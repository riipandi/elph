//! MCP client connectivity via rmcp.

use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use super::config::{McpServerConfig, McpStdioConfig};

type McpClient = RunningService<rmcp::RoleClient, ()>;

#[derive(Debug, Clone)]
pub struct McpProbeResult {
    pub name: String,
    pub ok: bool,
    pub tool_count: usize,
    pub message: String,
}

pub async fn probe_stdio_server(name: &str, config: &McpStdioConfig) -> McpProbeResult {
    match with_stdio_client(config, |client| async move {
        let tools = client.list_all_tools().await.unwrap_or_default();
        let count = tools.len();
        let _ = client.cancel().await;
        Ok(count)
    })
    .await
    {
        Ok(count) => McpProbeResult {
            name: name.to_string(),
            ok: true,
            tool_count: count,
            message: format!("connected, {count} tools"),
        },
        Err(error) => McpProbeResult {
            name: name.to_string(),
            ok: false,
            tool_count: 0,
            message: error.to_string(),
        },
    }
}

pub async fn probe_server(name: &str, config: &McpServerConfig) -> McpProbeResult {
    match config {
        McpServerConfig::Stdio(cfg) => probe_stdio_server(name, cfg).await,
    }
}

pub async fn list_tools(config: &McpStdioConfig) -> Result<Vec<Tool>> {
    with_stdio_client(config, |client| async move {
        let tools = client.list_all_tools().await.context("list MCP tools")?;
        let _ = client.cancel().await;
        Ok(tools)
    })
    .await
}

pub async fn call_stdio_tool(config: &McpStdioConfig, tool_name: &str, args: Value) -> Result<CallToolResult> {
    let tool_name = tool_name.to_string();
    timeout(PROBE_TIMEOUT, async {
        with_stdio_client(config, |client| async move {
            let mut params = CallToolRequestParams::new(tool_name);
            if let Value::Object(map) = args {
                params = params.with_arguments(map);
            }
            let result = client.call_tool(params).await.context("call MCP tool")?;
            let _ = client.cancel().await;
            Ok(result)
        })
        .await
    })
    .await
    .context("MCP tool call timed out")?
}

async fn with_stdio_client<T, F, Fut>(config: &McpStdioConfig, f: F) -> Result<T>
where
    F: FnOnce(McpClient) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut command = Command::new(&config.command);
    command.args(&config.args);
    command.envs(config.env.iter());
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let transport = TokioChildProcess::new(command.configure(|_| {})).context("spawn MCP stdio transport")?;

    let client = ().serve(transport).await.context("initialize MCP client")?;

    f(client).await
}

/// Build a stdio command from a loose JSON object (CLI `mcp add`).
pub fn parse_stdio_config(command: String, args: Vec<String>, env: BTreeMap<String, String>) -> McpStdioConfig {
    McpStdioConfig { command, args, env }
}

pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
