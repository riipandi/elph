//! Prompt management for elph-agent.
//!
//! - [`builtin`] — static prompt constants and formatters used by the runtime
//! - [`external`] — filesystem-backed slash-command templates (`.md` files)
//! - [`invoke`] — slash-command argument parsing and placeholder substitution

pub mod builtin;
pub mod defaults;
pub mod encoding;
pub mod external;

#[cfg(feature = "prompt-templates")]
pub mod context;
#[cfg(feature = "prompt-templates")]
pub mod system_builder;
#[cfg(feature = "prompt-templates")]
pub mod template;

pub mod session_name;

mod invoke;

pub use builtin::plan_mode_reentry_prompt;
pub use builtin::session_name::extract_conversation_for_naming;
pub use builtin::session_name::sanitize_session_name;
pub use defaults::{DEFAULT_SYSTEM_PROMPT, resolve_system_prompt_text};
pub use encoding::PromptEncodingConfig;
pub use encoding::PromptEncodingDelimiter;
pub use encoding::PromptEncodingMode;
pub use encoding::PromptEncodingTargets;
pub use encoding::ToonDecodeError;
pub use encoding::apply_to_tool_result;
pub use encoding::decode_toon_fence;
pub use encoding::encode_value;
pub use encoding::extract_json_value;
pub use encoding::parse_toon_fence;
pub use external::{load_prompt_templates, load_sourced_prompt_templates};
pub use invoke::{format_prompt_template_invocation, parse_command_args, substitute_args};
pub use session_name::generate_session_name;
pub use session_name::generate_session_name_with_prompts;

#[cfg(feature = "prompt-templates")]
pub use context::{SystemPromptTemplateContext, ToolByKindContext, ToolNamesContext, tool_names_context};
#[cfg(feature = "prompt-templates")]
pub use system_builder::{PromptAssemblyMode, SystemPromptBuildError, SystemPromptBuilder, format_project_context};
#[cfg(feature = "prompt-templates")]
pub use template::{PromptRenderError, render_base_template, sanitize_system_prompt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTemplateDiagnosticCode {
    FileInfoFailed,
    ListFailed,
    ReadFailed,
    ParseFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplateDiagnostic {
    pub code: PromptTemplateDiagnosticCode,
    pub message: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPromptTemplatesResult {
    pub prompt_templates: Vec<crate::agent::harness::types::PromptTemplate>,
    pub diagnostics: Vec<PromptTemplateDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedPromptTemplate<TPromptTemplate, TSource> {
    pub prompt_template: TPromptTemplate,
    pub source: TSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedPromptTemplateDiagnostic<TSource> {
    pub code: PromptTemplateDiagnosticCode,
    pub message: String,
    pub path: String,
    pub source: TSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSourcedPromptTemplatesResult<TPromptTemplate, TSource> {
    pub prompt_templates: Vec<SourcedPromptTemplate<TPromptTemplate, TSource>>,
    pub diagnostics: Vec<SourcedPromptTemplateDiagnostic<TSource>>,
}
