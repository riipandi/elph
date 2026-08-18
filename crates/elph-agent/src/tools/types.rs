//! Agent tool definitions and execution callbacks.
//!
//! ## ToolContext pattern (pi v0.82.0+)
//!
//! Instead of capturing `Arc<LocalExecutionEnv>` in each tool factory, tools
//! receive an application-defined `ToolContext` at execution time. This allows
//! the harness to inject per-request context (cwd, env, filesystem, etc.) and
//! keeps tool definitions stateless.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::{ImageContent, TextContent, Tool, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::runtime::local_env::LocalExecutionEnv;

pub type AgentToolCall = ToolCall;

/// Application-defined context for tool execution.
///
/// Replaces the captured `Arc<LocalExecutionEnv>` in each tool factory.
/// The harness creates this context per-turn and passes it to every tool.
#[derive(Clone)]
pub struct ToolContext {
    /// The execution environment (filesystem, shell, etc.).
    pub env: Arc<LocalExecutionEnv>,
    /// Current working directory for path resolution.
    pub cwd: String,
    /// Whether the current turn is in plan mode (tool may be blocked).
    pub is_plan_mode: bool,
    /// Whether the agent is running in headless mode (`elph run`), which relaxes
    /// some tool defaults (e.g. no background-task timeout by default).
    pub is_headless: bool,
    /// Session terminal-capture directory (`APP_DATA/sessions/<SESSION_ID>/terminals`),
    /// when the harness is wired to a persistent session. `None` for stateless
    /// contexts (tests, examples) where output is not persisted to disk.
    pub terminals_dir: Option<PathBuf>,
}

impl ToolContext {
    pub fn new(env: Arc<LocalExecutionEnv>) -> Self {
        Self {
            env,
            cwd: String::new(),
            is_plan_mode: false,
            is_headless: false,
            terminals_dir: None,
        }
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = cwd.into();
        self
    }

    pub fn with_plan_mode(mut self, plan_mode: bool) -> Self {
        self.is_plan_mode = plan_mode;
        self
    }

    pub fn with_headless(mut self, headless: bool) -> Self {
        self.is_headless = headless;
        self
    }

    pub fn with_terminals_dir(mut self, terminals_dir: Option<PathBuf>) -> Self {
        self.terminals_dir = terminals_dir;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub content: Vec<ToolResultContent>,
    pub details: Value,
    /// Names of tools introduced by this result and available from this transcript point onward.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_tool_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate: Option<bool>,
    /// Optional usage metadata from tool execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Box<elph_ai::Usage>>,
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
            added_tool_names: None,
            terminate: None,
            usage: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::text(message)
    }

    pub fn with_usage(mut self, usage: elph_ai::Usage) -> Self {
        self.usage = Some(Box::new(usage));
        self
    }
}

pub type ToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>;

/// Context-aware tool execution function.
///
/// Receives the tool call ID, parsed arguments, optional abort signal, optional
/// progress callback, and the application-defined `ToolContext`.
/// Tool execution failure reported to the loop (`is_error` tool result).
#[derive(Debug)]
pub struct ToolError {
    pub message: String,
    pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|e| e as _)
    }
}

impl From<anyhow::Error> for ToolError {
    fn from(value: anyhow::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<std::io::Error> for ToolError {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}

pub type ToolExecuteFn = Arc<
    dyn Fn(
            String,
            Value,
            Option<CancellationToken>,
            Option<ToolUpdateCallback>,
            ToolContext,
        ) -> Pin<Box<dyn Future<Output = Result<AgentToolResult, ToolError>> + Send>>
        + Send
        + Sync,
>;

/// A context-aware harness tool (pi v0.82.0+ pattern).
///
/// Unlike the old `AgentTool` which captured `Arc<LocalExecutionEnv>` per factory,
/// this trait allows tools to receive context at execution time.
pub trait AgentHarnessTool: Send + Sync {
    fn tool(&self) -> &Tool;
    fn label(&self) -> &str;
    fn execution_mode(&self) -> Option<crate::types::ToolExecutionMode> {
        None
    }
    fn prepare_arguments(&self, args: Value) -> Value {
        args
    }
    fn execute(
        &self,
        id: String,
        args: Value,
        signal: Option<CancellationToken>,
        on_update: Option<ToolUpdateCallback>,
        context: ToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<AgentToolResult, ToolError>> + Send>>;
}

#[derive(Clone)]
pub struct AgentTool {
    pub tool: Tool,
    pub label: String,
    pub execution_mode: Option<crate::types::ToolExecutionMode>,
    pub prepare_arguments: Option<Arc<dyn Fn(Value) -> Value + Send + Sync>>,
    pub execute: ToolExecuteFn,
}

impl AgentTool {
    pub fn name(&self) -> &str {
        &self.tool.name
    }
}

/// Helper: create a context-aware tool from a function that takes `ToolContext`.
///
/// The tool receives `ToolContext` at execution time instead of capturing
/// `Arc<LocalExecutionEnv>` during construction.
pub fn context_aware_tool(
    tool: Tool,
    label: impl Into<String>,
    execute: impl Fn(
        String,
        Value,
        Option<CancellationToken>,
        Option<ToolUpdateCallback>,
        ToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<AgentToolResult, ToolError>> + Send>>
    + Send
    + Sync
    + 'static,
) -> AgentTool {
    let execute_fn: ToolExecuteFn =
        Arc::new(move |id, args, signal, on_update, context| execute(id, args, signal, on_update, context));
    AgentTool {
        tool,
        label: label.into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: execute_fn,
    }
}
