//! Coding-agent system prompt assembly.
//!
//! Layering (generic runtime → product domain):
//! 1. [`elph_agent::templates::base`] — persona, session env, [`format_skills_for_system_prompt`] (`<available_skills>`)
//! 2. `coding_base.md` — Grok-style sections (`<action_safety>`, `<tool_calling>`, …) with Pi tool names
//! 3. `mode_section` — per-mode appendix (`<mode_context>`)
//! 4. [`elph_agent::format_project_context`] — Pi-style `<project_context>` for AGENTS.md

use std::path::Path;

use crate::types::AgentMode;
use elph_agent::{
    AgentHarnessResources, PromptAssemblyMode, SystemPromptBuilder, SystemPromptTemplateContext,
    format_skills_for_system_prompt, now_iso_timestamp,
};

use super::modes::{build_mode_section, mode_footer_slug};

const CODING_BASE_TEMPLATE: &str = "coding_base";

/// Build the dynamic system prompt for a coding session turn.
///
/// `preferred_chat_language` controls the language the AI uses for conversational
/// responses in the transcript. Code, comments, and documentation remain in English
/// regardless of this value. Pass an empty string to use the default (English).
pub fn build_coding_system_prompt(
    cwd: &Path,
    resources: &AgentHarnessResources,
    tool_names: &[String],
    agents_md: Option<&str>,
    mode: AgentMode,
    preferred_chat_language: impl Into<String>,
) -> anyhow::Result<String> {
    let date = now_iso_timestamp().chars().take(10).collect::<String>();
    let shell_path = std::env::var("SHELL").ok();
    let os_name = std::env::consts::OS.to_string();

    let skills_section = if resources.skills.is_empty() {
        String::new()
    } else {
        format_skills_for_system_prompt(&resources.skills)
    };

    let preferred_chat_language: String = preferred_chat_language.into();
    let use_language_preference = !preferred_chat_language.is_empty() && preferred_chat_language != "english";
    // Clone the value before it's moved into the template context.
    let language_for_prompt = preferred_chat_language.clone();

    let context = SystemPromptTemplateContext {
        persona: "You are an expert AI coding assistant, operate in Elph CLI. Your main goal is to complete the user's request."
            .to_string(),
        working_directory: Some(cwd.display().to_string()),
        current_date: Some(date),
        os_name: Some(os_name),
        shell_path,
        agents_md: agents_md.unwrap_or_default().trim().to_string(),
        skills_section,
        mode_section: build_mode_section(mode),
        agent_mode: mode_footer_slug(mode).to_string(),
        preferred_chat_language,
        is_non_interactive: false,
        ..Default::default()
    }
    .with_active_tool_names(tool_names);

    let mut prompt = SystemPromptBuilder::new()
        .mode(PromptAssemblyMode::Extend)
        .context(context)
        .register_domain_template(CODING_BASE_TEMPLATE, include_str!("../../../templates/agent/coding_base.md"))?
        .domain_template(CODING_BASE_TEMPLATE)
        .render()?;

    // Append a language preference note for conversational responses.
    // Code, comments, and documentation remain in English regardless.
    if use_language_preference {
        prompt.push_str(&format!(
            "\n\n<language_preference>\nThe user prefers conversational responses in {language_for_prompt}. \
Use that language for all user-facing text, explanations, and prose in the transcript. \
Keep code, comments, and documentation in English.\n</language_preference>"
        ));
    }

    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::AgentHarnessResources;

    #[test]
    fn coding_prompt_layers_base_domain_and_mode() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file", "write_file", "grep", "list_dir", "shell_exec"].map(String::from),
            None,
            AgentMode::Build,
            "",
        )
        .expect("prompt");

        assert!(prompt.contains("You are an expert AI coding assistant"));

        assert!(prompt.contains("You are an expert AI coding assistant"));
        assert!(prompt.contains("Working directory: /tmp/project"));
        assert!(prompt.contains("<action_safety>"));
        assert!(prompt.contains("<tool_calling>"));
        assert!(prompt.contains("<output_efficiency>"));
        assert!(prompt.contains("<formatting>"));
        assert!(prompt.contains("<mode_context>"));
        assert!(prompt.contains("Mode: Build"));
        assert!(prompt.contains("<available_tools>"));
        assert!(prompt.contains("<tool>read_file</tool>"));
    }

    #[test]
    fn plan_mode_includes_proposed_plan_guidance() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[],
            None,
            AgentMode::Plan,
            "",
        )
        .expect("prompt");

        assert!(prompt.contains("<proposed_plan>"));
        assert!(prompt.contains("Plan mode"));
        assert!(prompt.contains("read-only mode (plan)"));
    }

    #[test]
    fn ask_mode_is_read_only_in_base_and_appendix() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".to_string()],
            None,
            AgentMode::Ask,
            "",
        )
        .expect("prompt");

        assert!(prompt.contains("Mode: Ask"));
        assert!(prompt.contains("Ask mode"));
        assert!(prompt.contains("read-only mode (ask)"));
        assert!(!prompt.contains("warrant user confirmation"));
        assert!(prompt.contains("Do not call mutating tools"));
    }

    #[test]
    fn brave_mode_skips_build_approval_block() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["write_file".to_string()],
            None,
            AgentMode::Brave,
            "",
        )
        .expect("prompt");

        assert!(prompt.contains("Mode: Brave"));
        assert!(prompt.contains("Brave mode"));
        assert!(prompt.contains("without approval prompts"));
        assert!(!prompt.contains("warrant user confirmation"));
    }

    #[test]
    fn build_mode_includes_approval_safety_block() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["write_file".to_string()],
            None,
            AgentMode::Build,
            "",
        )
        .expect("prompt");

        assert!(prompt.contains("Build mode"));
        assert!(prompt.contains("warrant user confirmation"));
    }

    #[test]
    fn agents_md_uses_pi_project_context_wrapper() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[],
            Some("Always run tests."),
            AgentMode::Build,
            "",
        )
        .expect("prompt");

        assert!(prompt.contains("<project_context>"));
        assert!(prompt.contains("<project_instructions path=\"AGENTS.md\">"));
        assert!(prompt.contains("Always run tests."));
    }
}
