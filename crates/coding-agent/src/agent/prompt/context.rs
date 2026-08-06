//! Elph-specific system prompt context extensions.

use std::collections::HashSet;

use serde::Serialize;

use crate::codegraph::tools::CODEGRAPH_TOOL_NAMES;

/// Elph-specific tool names exposed to coding prompt templates.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ElphToolNamesContext {
    pub code_search: String,
    pub code_impact: String,
    pub code_status: String,
    pub code_reindex: String,
}

/// Combined context for elph coding prompt templates.
///
/// Flattens the generic `elph_agent::SystemPromptTemplateContext` fields and
/// adds elph-specific `codegraph` tool names so MiniJinja templates can access
/// both base and product-specific variables.
#[derive(Debug, Clone, Serialize)]
pub struct ElphCodingPromptContext<'a> {
    #[serde(flatten)]
    pub base: &'a elph_agent::SystemPromptTemplateContext,
    pub codegraph: ElphToolNamesContext,
}

impl<'a> ElphCodingPromptContext<'a> {
    pub fn new(base: &'a elph_agent::SystemPromptTemplateContext) -> Self {
        Self {
            base,
            codegraph: ElphToolNamesContext::default(),
        }
    }

    pub fn with_codegraph_tools(mut self, names: &[String]) -> Self {
        let set: HashSet<&str> = names.iter().map(String::as_str).collect();
        let name = |tool: &str| {
            if set.contains(tool) {
                tool.to_string()
            } else {
                String::new()
            }
        };
        self.codegraph = ElphToolNamesContext {
            code_search: name("code_search"),
            code_impact: name("code_impact"),
            code_status: name("code_status"),
            code_reindex: name("code_reindex"),
        };
        self
    }
}

/// Check whether any codegraph tools are active.
pub fn has_codegraph_tools(names: &[String]) -> bool {
    names.iter().any(|name| CODEGRAPH_TOOL_NAMES.contains(&name.as_str()))
}
