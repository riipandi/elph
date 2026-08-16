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
    let lines: Vec<&str> = prompt.lines().map(|line| line.trim_end()).collect();

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

    while out.first().is_some_and(|s| s.is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|s| s.is_empty()) {
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
/// Current date: {current_date} | OS: {os_name} | Shell: {shell_path}  (when set)
/// {skills_section}                         (when set)
/// ```
pub fn render_base_template(ctx: &SystemPromptTemplateContext) -> String {
    let mut out = String::new();
    out.push_str(ctx.persona.trim());

    let mut context_parts = Vec::new();

    if let Some(dir) = ctx.working_directory.as_deref().filter(|s| !s.is_empty()) {
        context_parts.push(format!("Working directory: {}", path_to_relative_with_tilde(dir)));
    }

    let mut meta_parts = Vec::new();
    if let Some(date) = ctx.current_date.as_deref().filter(|s| !s.is_empty()) {
        meta_parts.push(format!("Current date: {}", date));
    }
    if let Some(os) = ctx.os_name.as_deref().filter(|s| !s.is_empty()) {
        meta_parts.push(format!("OS: {}", os));
    }
    if let Some(shell) = ctx.shell_path.as_deref().filter(|s| !s.is_empty()) {
        meta_parts.push(format!("Shell: {}", shell));
    }

    if !meta_parts.is_empty() {
        context_parts.push(meta_parts.join(" | "));
    }

    if !context_parts.is_empty() {
        out.push_str("\n\n");
        out.push_str(&context_parts.join("\n"));
    }
    if !ctx.skills_section.trim().is_empty() {
        out.push_str("\n\n");
        out.push_str(ctx.skills_section.trim());
    }
    sanitize_system_prompt(&out)
}

/// Convert an absolute path to a relative path using `~` for home directory.
/// If the path doesn't start with the home directory, returns the original path.
fn path_to_relative_with_tilde(path: &str) -> String {
    if let Some(home_dir) = std::env::var("HOME").ok() {
        if path.starts_with(&home_dir) {
            // Replace home directory with ~, ensuring we handle the path separator correctly
            let remainder = path.strip_prefix(&home_dir).unwrap_or(path);
            // Remove leading slash if present
            let remainder = remainder.strip_prefix('/').unwrap_or(remainder);
            return format!("~/{}", remainder);
        }
    }
    path.to_string()
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

    #[test]
    fn path_to_relative_with_tilde_converts_home_path() {
        // Set a temporary HOME environment variable for testing
        unsafe {
            std::env::set_var("HOME", "/Users/testuser");
        }

        let abs_path = "/Users/testuser/Developer/github.com/riipandi/elph";
        let result = path_to_relative_with_tilde(abs_path);
        assert_eq!(result, "~/Developer/github.com/riipandi/elph");

        // Clean up
        unsafe {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn path_to_relative_with_tilde_leaves_non_home_paths_unchanged() {
        unsafe {
            std::env::set_var("HOME", "/Users/testuser");
        }

        let other_path = "/var/log/system.log";
        let result = path_to_relative_with_tilde(other_path);
        assert_eq!(result, "/var/log/system.log");

        unsafe {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn path_to_relative_with_tilde_handles_no_home_env() {
        unsafe {
            std::env::remove_var("HOME");
        }

        let path = "/Users/testuser/Developer/project";
        let result = path_to_relative_with_tilde(path);
        assert_eq!(result, "/Users/testuser/Developer/project");
    }
}
