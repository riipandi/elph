//! Agent tool definitions and execution callbacks.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::{ImageContent, TextContent, Tool, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub type AgentToolCall = ToolCall;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub content: Vec<ToolResultContent>,
    pub details: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(TextContent),
    Image(ImageContent),
}

impl AgentToolResult {
    pub fn text(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolResultContent::Text(TextContent::new(message))],
            details: Value::Object(Default::default()),
            terminate: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::text(message)
    }
}

pub type ToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>;

pub type ToolExecuteFn = Arc<
    dyn Fn(
            String,
            Value,
            Option<CancellationToken>,
            Option<ToolUpdateCallback>,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct AgentTool {
    pub tool: Tool,
    pub label: String,
    pub execution_mode: Option<super::ToolExecutionMode>,
    pub prepare_arguments: Option<Arc<dyn Fn(Value) -> Value + Send + Sync>>,
    pub execute: ToolExecuteFn,
}

impl AgentTool {
    pub fn name(&self) -> &str {
        &self.tool.name
    }
}
