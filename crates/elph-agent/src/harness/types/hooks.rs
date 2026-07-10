//! Harness hook result types.

use serde_json::Value;

use crate::types::{AgentMessage, ToolResultContent};

use super::events::CompactResult;
use super::options::AgentHarnessStreamOptionsPatch;

#[derive(Debug, Clone, Default)]
pub struct BeforeAgentStartResult {
    pub messages: Option<Vec<AgentMessage>>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContextResult {
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Default)]
pub struct BeforeProviderRequestResult {
    pub stream_options: Option<AgentHarnessStreamOptionsPatch>,
}

#[derive(Debug, Clone)]
pub struct BeforeProviderPayloadResult {
    pub payload: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallHookResult {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolResultPatch {
    pub content: Option<Vec<ToolResultContent>>,
    pub details: Option<Value>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionBeforeCompactResult {
    pub cancel: bool,
    pub compaction: Option<CompactResult>,
    pub custom_instructions: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionBeforeTreeResult {
    pub cancel: bool,
    pub summary: Option<BranchSummarySummary>,
    pub custom_instructions: Option<String>,
    pub replace_instructions: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BranchSummarySummary {
    pub summary: String,
    pub details: Option<Value>,
}

/// Result returned from generic [`AgentHarness::on`](super::agent_harness::AgentHarness::on) handlers.
#[derive(Debug, Clone)]
pub enum HarnessHookResult {
    BeforeAgentStart(BeforeAgentStartResult),
    Context(ContextResult),
    BeforeProviderRequest(BeforeProviderRequestResult),
    BeforeProviderPayload(BeforeProviderPayloadResult),
    ToolCall(ToolCallHookResult),
    ToolResult(ToolResultPatch),
    SessionBeforeCompact(SessionBeforeCompactResult),
    SessionBeforeTree(SessionBeforeTreeResult),
}
