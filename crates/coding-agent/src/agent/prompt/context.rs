//! Elph-specific system prompt context extensions.

use serde::Serialize;

/// Combined context for elph coding prompt templates.
///
/// Flattens the generic `elph_agent::prompt::SystemPromptTemplateContext` fields so
/// MiniJinja templates can access both base and product-specific variables.
#[derive(Debug, Clone, Serialize)]
pub struct ElphCodingPromptContext<'a> {
    #[serde(flatten)]
    pub base: &'a elph_agent::prompt::SystemPromptTemplateContext,
    /// Simplified Technical English (ASD-STE100) response rules are enabled.
    pub ste_code: bool,
    /// Multi-worker display name (memorable-id), when registered.
    pub worker_name: String,
    /// Compact live peer list for the prompt (names only), when multi-worker.
    pub worker_peers: String,
    /// Settings `memory.enabled` — gates memory tool group and `## Memory` policy.
    pub memory_enabled: bool,
}

impl<'a> ElphCodingPromptContext<'a> {
    pub fn new(base: &'a elph_agent::prompt::SystemPromptTemplateContext) -> Self {
        Self {
            base,
            ste_code: true,
            worker_name: String::new(),
            worker_peers: String::new(),
            memory_enabled: false,
        }
    }

    /// Toggle memory policy and tool-group sections (`settings.memory.enabled`).
    pub fn with_memory_enabled(mut self, enabled: bool) -> Self {
        self.memory_enabled = enabled;
        self
    }

    pub fn with_worker_name(mut self, name: Option<&str>) -> Self {
        self.worker_name = name.unwrap_or("").trim().to_string();
        self
    }

    pub fn with_worker_peers(mut self, peers: Option<&str>) -> Self {
        self.worker_peers = peers.unwrap_or("").trim().to_string();
        self
    }

    /// Enable or disable the Simplified Technical English response rules.
    pub fn with_ste_code(mut self, enabled: bool) -> Self {
        self.ste_code = enabled;
        self
    }
}
