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

/// Per-session prompt knobs derived from `Settings` and the live agent mode.
///
/// Grouped into a struct so the two booleans are named at every call site — as
/// positional `bool` arguments they were trivially swappable and pushed
/// `build_coding_system_prompt` past the `clippy::too_many_arguments` limit.
#[derive(Clone, Debug, Default)]
pub struct CodingPromptOptions {
    /// Agent permission / interaction mode for this turn.
    pub mode: AgentMode,
    /// Language the AI uses for conversational responses in the transcript.
    /// Code, comments, and documentation remain in English regardless of this
    /// value. Empty string uses the default (English).
    pub preferred_chat_language: String,
    /// Mirrors the `codegraph.enabled` setting: when false, the `<codegraph>`
    /// guidance section is omitted even if `code_*` tool names leak into
    /// `tool_names` (defense-in-depth on top of the tool-name check).
    pub codegraph_enabled: bool,
    /// Mirrors `simplifiedTechnicalEnglish`: renders the `<response_style>` block.
    pub ste_enabled: bool,
    /// Memorable multi-worker display name when workers are enabled.
    pub worker_name: Option<String>,
    /// Live peer summary (comma-separated names), refreshed each turn when available.
    pub worker_peers: Option<String>,
}

impl CodingPromptOptions {
    /// Options for `mode` with both feature sections enabled (test/default helper).
    pub fn new(mode: AgentMode) -> Self {
        Self {
            mode,
            preferred_chat_language: String::new(),
            codegraph_enabled: true,
            ste_enabled: true,
            worker_name: None,
            worker_peers: None,
        }
    }

    pub fn with_worker_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        self.worker_name = if name.trim().is_empty() { None } else { Some(name) };
        self
    }

    pub fn with_worker_peers(mut self, peers: impl Into<String>) -> Self {
        let peers = peers.into();
        self.worker_peers = if peers.trim().is_empty() { None } else { Some(peers) };
        self
    }

    /// Set the preferred conversational language.
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.preferred_chat_language = language.into();
        self
    }

    /// Toggle the `<codegraph>` guidance section.
    pub fn with_codegraph(mut self, enabled: bool) -> Self {
        self.codegraph_enabled = enabled;
        self
    }

    /// Toggle the `<response_style>` (Simplified Technical English) section.
    pub fn with_ste(mut self, enabled: bool) -> Self {
        self.ste_enabled = enabled;
        self
    }
}

/// Build the dynamic system prompt for a coding session turn.
pub fn build_coding_system_prompt(
    cwd: &Path,
    resources: &AgentHarnessResources,
    tool_names: &[String],
    agents_md: Option<&str>,
    options: &CodingPromptOptions,
) -> anyhow::Result<String> {
    let CodingPromptOptions {
        mode,
        preferred_chat_language,
        codegraph_enabled,
        ste_enabled,
        worker_name,
        worker_peers,
    } = options.clone();
    let date = now_iso_timestamp().chars().take(10).collect::<String>();
    let shell_path = std::env::var("SHELL").ok();
    let os_name = std::env::consts::OS.to_string();

    let skills_section = if resources.skills.is_empty() {
        String::new()
    } else {
        format_skills_for_context(&resources.skills, cwd)
    };

    let base_context = SystemPromptTemplateContext {
        persona: "You are a fast, decisive coding agent. Complete the user's request with the fewest effective steps — act first, explain little."
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
    let elph_context = elph_context.with_ste_code(ste_enabled);
    let elph_context = elph_context.with_worker_name(worker_name.as_deref());
    let elph_context = elph_context.with_worker_peers(worker_peers.as_deref());

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
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(prompt.contains("You are a fast, decisive coding agent"));
        assert!(prompt.contains("Working directory: /tmp/project"));
        assert!(prompt.contains("<action_safety>"));
        assert!(prompt.contains("<tool_calling>"));
        assert!(prompt.contains("<execution>"));
        assert!(prompt.contains("<output>"));
        assert!(prompt.contains("<operating_loop>"));
        assert!(prompt.contains("Bias to action"));
        assert!(prompt.contains("<mode_context>"));
        assert!(prompt.contains("Mode: Build"));
        assert!(prompt.contains("<available_tools>"));
        assert!(prompt.contains("<tool>read_file</tool>"));
        assert!(prompt.contains("<memory_and_context>"));
        assert!(prompt.contains("Do not open a memory ritual"));
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
            &CodingPromptOptions::new(AgentMode::Build),
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
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(!prompt.contains("<codegraph>"));
        assert!(!prompt.contains("code_search"));
        assert!(prompt.contains("Locate with the narrowest tool (`grep` / targeted `read_file`)"));
    }

    #[test]
    fn coding_prompt_documents_lazy_mcp_activation() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".into(), "list_available_tools".into()],
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(prompt.contains("inactive by default"));
        assert!(prompt.contains("name_prefix"));
        assert!(prompt.contains("mcp_deepwiki__") || prompt.contains("mcp_<server>__"));
        // Inactive MCP names must not appear in the authoritative active list.
        assert!(!prompt.contains("<tool>mcp_"));
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
            &CodingPromptOptions::new(AgentMode::Build).with_codegraph(false),
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
            &CodingPromptOptions::new(AgentMode::Plan),
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
            &CodingPromptOptions::new(AgentMode::Ask),
        )
        .expect("prompt");

        assert!(prompt.contains("Mode: Ask"));
        assert!(prompt.contains("Ask mode"));
        assert!(prompt.contains("read-only mode (ask)"));
        assert!(!prompt.contains("warrant user confirmation"));
        assert!(prompt.contains("Do not call mutating tools"));
    }

    #[test]
    fn ste_flag_controls_response_style_section() {
        let enabled = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".to_string()],
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");
        assert!(enabled.contains("<response_style>"));
        assert!(enabled.contains("Simplified Technical English"));
        assert!(enabled.contains("No preamble or recap"));

        let disabled = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".to_string()],
            None,
            &CodingPromptOptions::new(AgentMode::Build).with_ste(false),
        )
        .expect("prompt");
        assert!(!disabled.contains("<response_style>"));
        assert!(!disabled.contains("Simplified Technical English"));
        assert!(
            !disabled.contains("No preamble or recap"),
            "STE-only rule must not appear when the flag is off"
        );
    }

    #[test]
    fn non_english_language_preference_does_not_force_english_chat_under_ste() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".to_string()],
            None,
            &CodingPromptOptions::new(AgentMode::Build).with_language("indonesian"),
        )
        .expect("prompt");

        assert!(prompt.contains("<language_preference>"));
        assert!(prompt.contains("Use indonesian for user-facing chat prose"));
        assert!(prompt.contains("<response_style>"));
        assert!(prompt.contains("Write user-facing chat in indonesian"));
        assert!(prompt.contains("Do not switch chat to English just because this section mentions STE"));
        // Repo artifacts stay English even when chat language is not.
        assert!(prompt.contains("use English and the same brevity rules"));
    }

    #[test]
    fn brave_mode_skips_build_approval_block() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["write_file".to_string()],
            None,
            &CodingPromptOptions::new(AgentMode::Brave),
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
            &CodingPromptOptions::new(AgentMode::Build),
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
            &CodingPromptOptions::new(AgentMode::Build),
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
            &CodingPromptOptions::new(AgentMode::Build),
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
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(prompt.contains("`list_dir` to inspect"));
        assert!(prompt.contains("with `read_file`"));
        assert!(prompt.contains("Use `edit_file` for focused changes"));
        assert!(prompt.contains("Use `write_file` for new files"));
    }

    #[test]
    fn shell_use_guidance_renders_alongside_shell_exec() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["shell_exec", "shell_use"].map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(prompt.contains("`shell_use` drives stateful PTY sessions"));
        assert!(prompt.contains("Prefer `shell_exec` for one-shot commands"));
        assert!(
            prompt.contains("sessions persist across calls until `action: close`")
                || prompt.contains("`close` with `all: true`")
        );
    }

    #[test]
    fn shell_use_guidance_omitted_when_tool_inactive() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["shell_exec"].map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(prompt.contains("`shell_exec` runs commands in the working directory"));
        assert!(!prompt.contains("`shell_use` drives stateful PTY sessions"));
    }

    #[test]
    fn project_rules_remain_after_language_and_mode_context() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[],
            Some("Always run tests."),
            &CodingPromptOptions::new(AgentMode::Build).with_language("indonesian"),
        )
        .expect("prompt");

        let language = prompt.find("<language_preference>").expect("language preference");
        let mode = prompt.find("<mode_context>").expect("mode context");
        let project = prompt.find("<project_context>").expect("project context");
        assert!(language < mode);
        assert!(mode < project);
        assert!(prompt.contains("Use indonesian for user-facing chat prose"));
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
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        // Lean ReAct prompt: keep static domain body compact even with STE + subagents.
        assert!(prompt.len() < 10_000, "static prompt is {} bytes", prompt.len());
    }

    #[test]
    fn subagent_guidance_is_conditional_and_covers_coordination() {
        let without_subagents = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".to_string()],
            None,
            &CodingPromptOptions::new(AgentMode::Build),
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
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(with_subagents.contains("<subagents>"));
        assert!(with_subagents.contains("Start independent subagents before waiting"));
        assert!(with_subagents.contains("exclusive write scope"));
        assert!(with_subagents.contains("Reuse with `followup_task`"));
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
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");
        assert!(only_spawn.contains("`spawn_agent`"));
    }
}
