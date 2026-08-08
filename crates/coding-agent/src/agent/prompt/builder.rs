//! Coding-agent system prompt assembly.
//!
//! Layering (generic runtime → product domain):
//! 1. [`elph_agent::render_base_template`] — persona, session env, [`format_skills_for_context`] (`<available_skills>`)
//! 2. [`super::template::coding_agent_engine`] — Grok-style `coding_base.md` (MiniJinja) with tool names
//! 3. `mode_section` — per-mode appendix (`<mode_context>`)
//! 4. [`elph_agent::format_project_context`] — Pi-style `<project_context>` for AGENTS.md

use std::path::Path;

use crate::types::AgentMode;
use elph_agent::{AgentHarnessResources, PromptAssemblyMode, SystemPromptBuilder, SystemPromptTemplateContext};
use elph_agent::{format_skills_for_context, now_iso_timestamp};

use super::context::{ElphCodingPromptContext, has_codegraph_tools};
use super::modes::{build_mode_section, mode_footer_slug};
use super::template::coding_agent_engine;

/// Build the dynamic system prompt for a coding session turn.
///
/// `preferred_chat_language` controls the language the AI uses for conversational
/// responses in the transcript. Code, comments, and documentation remain in English
/// regardless of this value. Pass an empty string to use the default (English).
///
/// `codegraph_enabled` mirrors the `codegraph.enabled` setting: when false, the
/// `<codegraph>` guidance section is omitted even if `code_*` tool names leak into
/// `tool_names` (defense-in-depth on top of the tool-name check).
pub fn build_coding_system_prompt(
    cwd: &Path,
    resources: &AgentHarnessResources,
    tool_names: &[String],
    agents_md: Option<&str>,
    mode: AgentMode,
    preferred_chat_language: impl Into<String>,
    codegraph_enabled: bool,
) -> anyhow::Result<String> {
    let date = now_iso_timestamp().chars().take(10).collect::<String>();
    let shell_path = std::env::var("SHELL").ok();
    let os_name = std::env::consts::OS.to_string();

    let skills_section = if resources.skills.is_empty() {
        String::new()
    } else {
        format_skills_for_context(&resources.skills, cwd)
    };

    let preferred_chat_language: String = preferred_chat_language.into();

    let base_context = SystemPromptTemplateContext {
        persona: "You are an expert, intelligent, and interactive AI agent. Complete the user's request end-to-end using the available context and tools."
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

    let elph_context = ElphCodingPromptContext::new(&base_context);
    let elph_context = if codegraph_enabled && has_codegraph_tools(tool_names) {
        elph_context.with_codegraph_tools(tool_names)
    } else {
        elph_context
    };

    let coding_base = coding_agent_engine().render("coding_base", &elph_context)?;
    SystemPromptBuilder::new()
        .mode(PromptAssemblyMode::Extend)
        .context(base_context)
        .domain_body(coding_base)
        .render()
        .map_err(Into::into)
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
            true,
        )
        .expect("prompt");

        assert!(prompt.contains("You are an expert, intelligent, and interactive AI agent"));
        assert!(prompt.contains("Working directory: /tmp/project"));
        assert!(prompt.contains("<action_safety>"));
        assert!(prompt.contains("<tool_calling>"));
        assert!(prompt.contains("<execution>"));
        assert!(prompt.contains("<output>"));
        assert!(prompt.contains("<mode_context>"));
        assert!(prompt.contains("Mode: Build"));
        assert!(prompt.contains("<available_tools>"));
        assert!(prompt.contains("<tool>read_file</tool>"));
        assert!(prompt.contains("<memory_and_context>"));
        assert!(prompt.contains("Active recall is expected"));
    }

    #[test]
    fn coding_prompt_includes_codegraph_when_tools_present() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[
                "read_file".into(),
                "code_search".into(),
                "code_impact".into(),
                "memory_search".into(),
            ],
            None,
            AgentMode::Build,
            "",
            true,
        )
        .expect("prompt");

        assert!(prompt.contains("<codegraph>"));
        assert!(prompt.contains("code_search"));
        assert!(prompt.contains("code_impact"));
        assert!(prompt.contains("memory_search"));
    }

    #[test]
    fn coding_prompt_omits_codegraph_without_tools() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[
                "read_file".into(),
                "grep".into(),
                // No codegraph tools — the literal string must not appear
                // anywhere (step 3 of <execution> must inline-condition on
                // codegraph.code_search, same as the <codegraph> block).
            ],
            None,
            AgentMode::Build,
            "",
            true,
        )
        .expect("prompt");

        assert!(!prompt.contains("<codegraph>"));
        assert!(!prompt.contains("code_search"));
        assert!(prompt.contains("Locate code with the narrowest tool (`grep` / targeted `read_file`)"));
    }

    #[test]
    fn coding_prompt_omits_codegraph_when_disabled_even_with_tools() {
        // Defense-in-depth: `codegraph.enabled` false must hide the `<codegraph>`
        // guidance section even if `code_*` tool names are present in the active
        // tool list (they still appear in `<available_tools>`).
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".into(), "code_search".into(), "code_impact".into()],
            None,
            AgentMode::Build,
            "",
            false,
        )
        .expect("prompt");

        assert!(!prompt.contains("<codegraph>"));
        assert!(!prompt.contains("code index"));
        // Guidance that only exists inside the `<codegraph>` section must be gone.
        assert!(!prompt.contains("blast radius"));
        assert!(!prompt.contains("`code_search` first"));
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
            true,
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
            true,
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
            true,
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
            true,
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
            true,
        )
        .expect("prompt");

        assert!(prompt.contains("<project_context>"));
        assert!(prompt.contains("<project_instructions path=\"AGENTS.md\">"));
        assert!(prompt.contains("Always run tests."));
    }

    #[test]
    fn coding_prompt_prioritizes_context_rules_and_tool_routing() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[
                "read_file",
                "grep",
                "find_path",
                "edit_file",
                "write_file",
                "shell_exec",
                "list_available_tools",
            ]
            .map(String::from),
            None,
            AgentMode::Build,
            "",
            true,
        )
        .expect("prompt");

        assert!(prompt.contains("Follow this precedence"));
        assert!(prompt.contains("Search file contents and symbols with `grep`"));
        assert!(prompt.contains("Find files by name or glob with `find_path`"));
        assert!(prompt.contains("focused changes to existing files"));
        assert!(prompt.contains("Run independent tool calls in parallel"));
        // Lean-reading directive is present and memory policy is not duplicated in the mode section.
        assert!(prompt.contains("Read selectively: target the ranges or search hits you need"));
        assert!(!prompt.contains("minimize redundant reads"));
    }

    #[test]
    fn tool_calling_rules_have_clean_spacing() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file", "grep", "list_dir", "edit_file", "write_file"].map(String::from),
            None,
            AgentMode::Build,
            "",
            true,
        )
        .expect("prompt");

        assert!(prompt.contains("`list_dir` to inspect"));
        assert!(prompt.contains("with `read_file`"));
        assert!(prompt.contains("Use `edit_file` for focused changes"));
        assert!(prompt.contains("Use `write_file` for new files"));
    }

    #[test]
    fn project_rules_remain_after_language_and_mode_context() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[],
            Some("Always run tests."),
            AgentMode::Build,
            "indonesian",
            true,
        )
        .expect("prompt");

        let language = prompt.find("<language_preference>").expect("language preference");
        let mode = prompt.find("<mode_context>").expect("mode context");
        let project = prompt.find("<project_context>").expect("project context");
        assert!(language < mode);
        assert!(mode < project);
        assert!(prompt.contains("Use indonesian for user-facing prose"));
    }

    #[test]
    fn static_coding_prompt_stays_compact() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[
                "read_file",
                "grep",
                "find_path",
                "list_dir",
                "edit_file",
                "write_file",
                "shell_exec",
                "web_fetch",
                "web_search",
                "ask_user_question",
                "list_available_tools",
                "spawn_agent",
                "send_message",
                "followup_task",
                "wait_agent",
                "list_agents",
            ]
            .map(String::from),
            None,
            AgentMode::Build,
            "",
            true,
        )
        .expect("prompt");

        // Budget was raised from 7_500 to 9_000 when the <mode_context> /
        // <memory_and_context> headers and the expanded tool-calling rules
        // were added to the static prompt (measured ~7.9 KB at tool count 16).
        assert!(prompt.len() < 9_000, "static prompt is {} bytes", prompt.len());
    }

    #[test]
    fn subagent_guidance_is_conditional_and_covers_coordination() {
        let without_subagents = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".to_string()],
            None,
            AgentMode::Build,
            "",
            true,
        )
        .expect("prompt");
        assert!(!without_subagents.contains("<subagents>"));

        let with_subagents = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[
                "spawn_agent",
                "send_message",
                "followup_task",
                "wait_agent",
                "list_agents",
            ]
            .map(String::from),
            None,
            AgentMode::Build,
            "",
            true,
        )
        .expect("prompt");

        assert!(with_subagents.contains("<subagents>"));
        assert!(with_subagents.contains("Start independent subagents before waiting"));
        assert!(with_subagents.contains("exclusive write scope"));
        assert!(with_subagents.contains("Reuse the same subagent with `followup_task`"));
        assert!(with_subagents.contains("`send_message` only queues context without starting a turn"));
        assert!(with_subagents.contains("`wait_agent` blocks until a subagent is idle"));
        assert!(with_subagents.contains("tool results carry status only"));
        // Backfill: subagent names used by the subagent guidance block must
        // resolve to literals even when only `spawn_agent` is registered.
        let only_spawn = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["spawn_agent".to_string()],
            None,
            AgentMode::Build,
            "",
            true,
        )
        .expect("prompt");
        assert!(only_spawn.contains("`spawn_agent`"));
    }
}
