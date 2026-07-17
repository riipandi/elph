//! Serializable context for system prompt templates.

use std::collections::HashSet;

use serde::Serialize;

/// Tool name map exposed to MiniJinja templates (`tools.read_file`, `tools.by_kind.read`, …).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolNamesContext {
    pub read_file: String,
    pub grep: String,
    pub find_path: String,
    pub list_dir: String,
    pub edit_file: String,
    pub write_file: String,
    pub shell_exec: String,
    pub web_fetch: String,
    pub web_search: String,
    pub diagnostics: String,
    pub ask_user_question: String,
    pub list_available_tools: String,
    pub by_kind: ToolByKindContext,
}

/// Category aliases used by coding prompt templates.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolByKindContext {
    pub read: String,
    pub edit: String,
    pub shell_exec: String,
}

/// Variables available to generic and domain system prompt templates.
#[derive(Debug, Clone, Serialize)]
pub struct SystemPromptTemplateContext {
    pub persona: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_path: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub agents_md: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub skills_section: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mode_section: String,
    /// Active agent mode slug (`build`, `plan`, `ask`, `brave`) for template conditionals.
    pub agent_mode: String,
    /// Tool names exposed to the model this turn (for `<available_tools>` blocks).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub active_tool_names: Vec<String>,
    pub tools: ToolNamesContext,
    pub is_non_interactive: bool,
}

impl Default for SystemPromptTemplateContext {
    fn default() -> Self {
        Self {
            persona: super::defaults::DEFAULT_SYSTEM_PROMPT.to_string(),
            working_directory: None,
            current_date: None,
            os_name: None,
            shell_path: None,
            agents_md: String::new(),
            skills_section: String::new(),
            mode_section: String::new(),
            agent_mode: "build".to_string(),
            active_tool_names: Vec::new(),
            tools: ToolNamesContext::default(),
            is_non_interactive: false,
        }
    }
}

impl SystemPromptTemplateContext {
    pub fn with_active_tool_names(mut self, names: &[String]) -> Self {
        self.tools = tool_names_context(names);
        self.active_tool_names = names.to_vec();
        self
    }
}

/// Build template tool context from the active tool names for this turn.
pub fn tool_names_context(names: &[String]) -> ToolNamesContext {
    let set: HashSet<&str> = names.iter().map(String::as_str).collect();
    let name = |tool: &str| {
        if set.contains(tool) {
            tool.to_string()
        } else {
            String::new()
        }
    };
    let first = |candidates: &[&str]| {
        candidates
            .iter()
            .find(|candidate| set.contains(**candidate))
            .map(|candidate| (*candidate).to_string())
            .unwrap_or_default()
    };

    ToolNamesContext {
        read_file: name("read_file"),
        grep: name("grep"),
        find_path: name("find_path"),
        list_dir: name("list_dir"),
        edit_file: name("edit_file"),
        write_file: name("write_file"),
        shell_exec: name("shell_exec"),
        web_fetch: name("web_fetch"),
        web_search: name("web_search"),
        diagnostics: name("diagnostics"),
        ask_user_question: name("ask_user_question"),
        list_available_tools: name("list_available_tools"),
        by_kind: ToolByKindContext {
            read: first(&["read_file"]),
            edit: first(&["edit_file", "write_file"]),
            shell_exec: name("shell_exec"),
        },
    }
}
