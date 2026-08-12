//! System prompt base template rendered directly in Rust (no template engine).

use thiserror::Error;

use super::context::SystemPromptTemplateContext;

#[derive(Debug, Error)]
pub enum PromptRenderError {
    #[error("template render failed: {0}")]
    Render(String),
}

/// Sanitize a compiled system prompt: trim trailing whitespace, collapse
/// consecutive blank lines to at most one, and strip leading/trailing blanks.
pub fn sanitize_system_prompt(prompt: &str) -> String {
    let lines: Vec<&str> = prompt
        .lines()
        .map(|line| line.trim_end())
        .collect();

    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut blank_run = 0usize;

    for line in &lines {
        if line.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push(line);
            }
        } else {
            blank_run = 0;
            out.push(line);
        }
    }

    while out.first().map_or(false, |s| s.is_empty()) {
        out.remove(0);
    }
    while out.last().map_or(false, |s| s.is_empty()) {
        out.pop();
    }

    out.join("\n")
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
    sanitize_system_prompt(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_system_prompt_removes_trailing_whitespace() {
        let input = "Line 1  \nLine 2\n\n  \nLine 3  ";
        let output = sanitize_system_prompt(input);
        assert_eq!(output, "Line 1\nLine 2\n\nLine 3");
    }

    #[test]
    fn sanitize_system_prompt_collapses_excessive_blank_lines() {
        let input = "Line 1\n\n\n\n\nLine 2";
        let output = sanitize_system_prompt(input);
        assert_eq!(output, "Line 1\n\nLine 2");
    }

    #[test]
    fn sanitize_system_prompt_collapses_to_single_blank_line() {
        let input = "Line 1\n\n\nLine 2";
        let output = sanitize_system_prompt(input);
        assert_eq!(output, "Line 1\n\nLine 2");
    }

    #[test]
    fn sanitize_system_prompt_trims_leading_and_trailing_blanks() {
        let input = "\n\n\nLine 1\nLine 2\n\n\n";
        let output = sanitize_system_prompt(input);
        assert_eq!(output, "Line 1\nLine 2");
    }

    #[test]
    fn sanitize_system_prompt_handles_empty_string() {
        let input = "";
        let output = sanitize_system_prompt(input);
        assert_eq!(output, "");
    }

    #[test]
    fn sanitize_system_prompt_handles_blank_only() {
        let input = "\n\n\n";
        let output = sanitize_system_prompt(input);
        assert_eq!(output, "");
    }

    #[test]
    fn sanitize_system_prompt_handles_single_line() {
        let input = "Single line  ";
        let output = sanitize_system_prompt(input);
        assert_eq!(output, "Single line");
    }
}
