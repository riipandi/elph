//! Internal mutable agent state.

use std::collections::HashSet;

use elph_ai::Model;

use crate::types::{AgentMessage, AgentState, AgentThinkingLevel, AgentTool};

pub fn default_model() -> Model {
    Model {
        id: "unknown".to_string(),
        name: "unknown".to_string(),
        api: "unknown".to_string(),
        provider: "unknown".to_string(),
        base_url: String::new(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![],
        cost: elph_ai::ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 0,
        max_tokens: 0,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: None,
        anthropic_compat: None,
    }
}

pub struct MutableAgentState {
    system_prompt: String,
    model: Model,
    thinking_level: AgentThinkingLevel,
    tools: Vec<AgentTool>,
    messages: Vec<AgentMessage>,
    is_streaming: bool,
    streaming_message: Option<AgentMessage>,
    pending_tool_calls: HashSet<String>,
    error_message: Option<String>,
}

impl MutableAgentState {
    pub fn from_partial(partial: Option<super::PartialAgentState>) -> Self {
        let partial = partial.unwrap_or_default();
        Self {
            system_prompt: partial.system_prompt.unwrap_or_default(),
            model: partial.model.unwrap_or_else(default_model),
            thinking_level: partial.thinking_level.unwrap_or_default(),
            tools: partial.tools.unwrap_or_default(),
            messages: partial.messages.unwrap_or_default(),
            is_streaming: false,
            streaming_message: None,
            pending_tool_calls: HashSet::new(),
            error_message: None,
        }
    }

    pub fn snapshot(&self) -> AgentState {
        AgentState {
            system_prompt: self.system_prompt.clone(),
            model: self.model.clone(),
            thinking_level: self.thinking_level,
            tools: self.tools.clone(),
            messages: self.messages.clone(),
            is_streaming: self.is_streaming,
            streaming_message: self.streaming_message.clone(),
            pending_tool_calls: self.pending_tool_calls.clone(),
            error_message: self.error_message.clone(),
        }
    }

    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn thinking_level(&self) -> AgentThinkingLevel {
        self.thinking_level
    }

    pub fn tools(&self) -> &[AgentTool] {
        &self.tools
    }

    pub fn messages(&self) -> &[AgentMessage] {
        &self.messages
    }

    pub fn set_streaming(&mut self, streaming: bool) {
        self.is_streaming = streaming;
    }

    pub fn set_streaming_message(&mut self, message: Option<AgentMessage>) {
        self.streaming_message = message;
    }

    pub fn push_message(&mut self, message: AgentMessage) {
        self.messages.push(message);
    }

    pub fn add_pending_tool_call(&mut self, id: String) {
        self.pending_tool_calls.insert(id);
    }

    pub fn remove_pending_tool_call(&mut self, id: &str) {
        self.pending_tool_calls.remove(id);
    }

    pub fn set_pending_tool_calls(&mut self, ids: HashSet<String>) {
        self.pending_tool_calls = ids;
    }

    pub fn set_error_message(&mut self, message: Option<String>) {
        self.error_message = message;
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    pub fn reset(&mut self) {
        self.messages.clear();
        self.is_streaming = false;
        self.streaming_message = None;
        self.pending_tool_calls.clear();
        self.error_message = None;
    }
}
