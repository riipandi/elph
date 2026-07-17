//! Agent mode guidance appended to the coding system prompt.

use crate::types::AgentMode;

pub fn mode_footer_slug(mode: AgentMode) -> &'static str {
    mode.footer_label()
}

pub fn mode_tool_guidance(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Build => {
            "Mode: Build — full tool access. Mutating tools (write, edit, shell_exec, create_dir, etc.) may require user approval."
        }
        AgentMode::Brave => "Mode: Brave — full tool access without approval prompts. Use mutating tools responsibly.",
        AgentMode::Plan => {
            "Mode: Plan — read-only exploration only. Use web_search, read_file, grep, and similar tools to research. \
             Wrap your implementation plan in <proposed_plan>...</proposed_plan> for user confirmation before editing."
        }
        AgentMode::Ask => {
            "Mode: Ask — read-only exploration. Do not attempt write_file, edit_file, shell_exec, create_dir, or other mutating tools; \
             they are not available in this mode."
        }
    }
}

pub fn mode_appendix_source(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Build => include_str!("../../../templates/agent/mode_build.md"),
        AgentMode::Plan => include_str!("../../../templates/agent/mode_plan.md"),
        AgentMode::Ask => include_str!("../../../templates/agent/mode_ask.md"),
        AgentMode::Brave => include_str!("../../../templates/agent/mode_brave.md"),
    }
}

/// One-line mode summary plus the mode-specific appendix template.
pub fn build_mode_section(mode: AgentMode) -> String {
    format!(
        "<mode_context>\n{}\n\n{}\n</mode_context>",
        mode_tool_guidance(mode),
        mode_appendix_source(mode)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_has_appendix_and_guidance() {
        for mode in [AgentMode::Build, AgentMode::Plan, AgentMode::Ask, AgentMode::Brave] {
            assert!(!mode_tool_guidance(mode).is_empty());
            assert!(!mode_appendix_source(mode).trim().is_empty());
            let section = build_mode_section(mode);
            assert!(section.contains("<mode_context>"));
            assert!(section.contains(mode.label()));
        }
    }

    #[test]
    fn plan_appendix_includes_proposed_plan_tag() {
        assert!(mode_appendix_source(AgentMode::Plan).contains("<proposed_plan>"));
    }

    #[test]
    fn brave_appendix_mentions_no_approval() {
        assert!(mode_appendix_source(AgentMode::Brave).contains("no approval"));
    }
}
