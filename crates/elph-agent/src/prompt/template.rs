//! System prompt base template rendered directly in Rust (no template engine).

use thiserror::Error;

use super::context::SystemPromptTemplateContext;

#[derive(Debug, Error)]
pub enum PromptRenderError {
    #[error("template render failed: {0}")]
    Render(String),
}

/// Render the generic base template (`persona` + optional session env blocks).
///
/// Mirrors the previous `templates/base.md` MiniJinja template:
///
/// ```text
/// {persona}
///
/// Working directory: {working_directory}   (when set)
/// Current date: {current_date}             (when set)
/// OS: {os_name}                            (when set)
/// Shell: {shell_path}                      (when set)
/// {skills_section}                         (when set)
/// ```
pub fn render_base_template(ctx: &SystemPromptTemplateContext) -> String {
    let mut out = String::new();
    out.push_str(ctx.persona.trim());

    if let Some(dir) = ctx.working_directory.as_deref().filter(|s| !s.is_empty()) {
        out.push_str("\n\nWorking directory: ");
        out.push_str(dir);
    }
    if let Some(date) = ctx.current_date.as_deref().filter(|s| !s.is_empty()) {
        out.push_str("\n\nCurrent date: ");
        out.push_str(date);
    }
    if let Some(os) = ctx.os_name.as_deref().filter(|s| !s.is_empty()) {
        out.push_str("\n\nOS: ");
        out.push_str(os);
    }
    if let Some(shell) = ctx.shell_path.as_deref().filter(|s| !s.is_empty()) {
        out.push_str("\n\nShell: ");
        out.push_str(shell);
    }
    if !ctx.skills_section.trim().is_empty() {
        out.push_str("\n\n");
        out.push_str(ctx.skills_section.trim());
    }
    out
}
