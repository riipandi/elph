//! Named prompt template invocation — elph-agent module.

mod load;
mod parse;
mod substitute;

pub use load::{load_prompt_templates, load_sourced_prompt_templates};
pub use substitute::{format_prompt_template_invocation, parse_command_args, substitute_args};

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
    pub prompt_templates: Vec<crate::harness::types::PromptTemplate>,
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
