//! Coding-agent system prompt assembly.
//!
//! Layering (generic runtime → product domain):
//! 1. [`elph_agent::prompt::render_base_template`] — persona, session env, [`format_skills_for_context`] (`<available_skills>`)
//! 2. [`super::template::coding_agent_engine`] — Grok-style `coding_base.txt` (MiniJinja) with tool names
//! 3. `mode_section` — per-mode appendix (`<mode_context>`)
//! 4. [`elph_agent::prompt::format_project_context`] — Pi-style `<project_context>` for AGENTS.md

use std::path::Path;

use crate::types::AgentMode;
use elph_agent::harness::AgentHarnessResources;
use elph_agent::harness::format_skills_for_context;
use elph_agent::messages::now_date_with_offset;
use elph_agent::prompt::{PromptAssemblyMode, SystemPromptBuilder, SystemPromptTemplateContext};

use super::context::ElphCodingPromptContext;
use super::modes::mode_footer_slug;
use super::template::coding_agent_engine;

/// Per-session prompt knobs derived from `Settings` and the live agent mode.
///
/// Grouped into a struct so the booleans are named at every call site — as
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
    /// Mirrors `simplifiedTechnicalEnglish`: renders the `<response_style>` block.
    pub ste_enabled: bool,
    /// Memorable multi-worker display name when workers are enabled.
    pub worker_name: Option<String>,
    /// Live peer summary (comma-separated names), refreshed each turn when available.
    pub worker_peers: Option<String>,
    /// Mirrors `settings.memory.enabled`: renders memory tool group and `## Memory`.
    pub memory_enabled: bool,
}

impl CodingPromptOptions {
    /// Options for `mode` with both feature sections enabled (test/default helper).
    pub fn new(mode: AgentMode) -> Self {
        Self {
            mode,
            preferred_chat_language: String::new(),
            ste_enabled: true,
            worker_name: None,
            worker_peers: None,
            memory_enabled: false,
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

    /// Toggle the `<response_style>` (Simplified Technical English) section.
    pub fn with_ste(mut self, enabled: bool) -> Self {
        self.ste_enabled = enabled;
        self
    }

    /// Toggle memory prompt sections (`settings.memory.enabled`).
    pub fn with_memory(mut self, enabled: bool) -> Self {
        self.memory_enabled = enabled;
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
        ste_enabled,
        worker_name,
        worker_peers,
        memory_enabled,
    } = options.clone();
    let date = now_date_with_offset();
    let shell_path = std::env::var("SHELL").ok();
    let os_name = std::env::consts::OS.to_string();

    let skills_section = if resources.skills.is_empty() {
        String::new()
    } else {
        format_skills_for_context(&resources.skills, cwd)
    };

    let mut base_context = SystemPromptTemplateContext {
        persona: "You are a fast and decisive coding agent. Accomplish the task using available tools, per the guidelines below."
            .to_string(),
        working_directory: Some(cwd.display().to_string()),
        current_date: Some(date),
        os_name: Some(os_name),
        shell_path,
        agents_md: agents_md.unwrap_or_default().trim().to_string(),
        skills_section,
        mode_section: String::new(), // Mode guidance is now inline in the template
        agent_mode: mode_footer_slug(mode).to_string(),
        preferred_chat_language,
        is_non_interactive: false,
        ..Default::default()
    }
    .with_active_tool_names(tool_names);

    let elph_context = ElphCodingPromptContext::new(&base_context);
    let elph_context = elph_context.with_ste_code(ste_enabled);
    let elph_context = elph_context.with_worker_name(worker_name.as_deref());
    let elph_context = elph_context.with_worker_peers(worker_peers.as_deref());
    let elph_context = elph_context.with_memory_enabled(memory_enabled);

    let coding_base = coding_agent_engine().render("coding_base", &elph_context)?;
    // Pass an empty skills_section to the base template so the generic renderer
    // does not emit <available_skills>. The coding_base.txt MiniJinja template
    // already embeds it (after </tool_calling>) via `${{ skills_section }}`.
    base_context.skills_section = String::new();
    SystemPromptBuilder::new()
        .mode(PromptAssemblyMode::Extend)
        .context(base_context)
        .domain_body(coding_base)
        .render()
        .map_err(Into::into)
}

/// Slim system prompt for the parallel inbound-worker answer loop.
///
/// Not `coding_base`: that template assumes write/shell/todo tools and a user
/// coding turn. Intercom only has `worker_reply` / `worker_list` / `worker_pending`.
pub fn build_intercom_system_prompt(worker_name: Option<&str>, worker_peers: Option<&str>) -> anyhow::Result<String> {
    #[derive(serde::Serialize)]
    struct IntercomPromptContext<'a> {
        worker_name: &'a str,
        worker_peers: &'a str,
    }
    let name = worker_name.unwrap_or("").trim();
    let peers = worker_peers.unwrap_or("").trim();
    let body = coding_agent_engine().render(
        "intercom_base",
        &IntercomPromptContext {
            worker_name: name,
            worker_peers: peers,
        },
    )?;
    SystemPromptBuilder::new()
        .mode(PromptAssemblyMode::Full)
        .domain_body(body)
        .render()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::harness::AgentHarnessResources;

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

        assert!(prompt.contains("More specific wins"));
        assert!(prompt.contains("Working directory: /tmp/project"));
        assert!(prompt.contains("Current date:"));
        assert!(prompt.contains("## Working loop"));
        assert!(prompt.contains("## Safety"));
        assert!(prompt.contains("## Mode"));
        assert!(prompt.contains("Build — full tool access"));
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

        assert!(prompt.contains("Inactive by default"));
        assert!(prompt.contains("name_prefix"));
        assert!(prompt.contains("<tool name=\"mcp_*\""));
        // Inactive MCP names must not appear in the authoritative active list.
        assert!(!prompt.contains("<tool>mcp_"));
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
        assert!(prompt.contains("Plan — design an implementation-ready plan"));
        assert!(prompt.contains("implementation-ready plan"));
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

        assert!(prompt.contains("Ask — read-only"));
        assert!(prompt.contains("Do not edit files"));
        assert!(!prompt.contains("warrant user confirmation"));
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

        assert!(prompt.contains("Brave — full tool access without approval prompts"));
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

        assert!(prompt.contains("Build — full tool access"));
        assert!(prompt.contains("approval UI"));
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

        assert!(prompt.contains("More specific wins"));
        assert!(prompt.contains("<tool_group name=\"read\">"));
        assert!(prompt.contains("<tool_group name=\"write\">"));
        assert!(prompt.contains("<tool_group name=\"exec\">"));
        assert!(prompt.contains("Parallelize independent calls"));
    }

    #[test]
    fn coding_prompt_documents_mid_turn_memory() {
        let with_memory = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[
                "read_file",
                "memory_search",
                "memory_recent",
                "memory_report",
                "memory_contradict",
                "memory_start_task",
            ]
            .map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build).with_memory(true),
        )
        .expect("prompt");

        assert!(with_memory.contains("## Memory"));
        assert!(with_memory.contains("<tool_group name=\"memory\">"));
        assert!(with_memory.contains("<tool name=\"memory_search\""));
        assert!(with_memory.contains("search and write **during** the turn"));
        assert!(with_memory.contains("do not wait for turn end"));
        assert!(with_memory.contains("5. Memory:"));

        let tools_without_flag = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file", "memory_search", "memory_recent", "memory_report"].map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build).with_memory(false),
        )
        .expect("prompt");
        assert!(!tools_without_flag.contains("<tool_group name=\"memory\">"));
        assert!(!tools_without_flag.contains("## Memory"));
        assert!(!tools_without_flag.contains("5. Memory:"));

        let without_memory = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".into()],
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");
        assert!(!without_memory.contains("<tool_group name=\"memory\">"));
        assert!(!without_memory.contains("## Memory"));
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

        assert!(prompt.contains("<tool_group name=\"read\">"));
        assert!(prompt.contains("<tool name=\"read_file\""));
        assert!(prompt.contains("<tool name=\"grep\""));
        assert!(prompt.contains("<tool name=\"list_dir\""));
        assert!(prompt.contains("<tool_group name=\"write\">"));
        assert!(prompt.contains("<tool name=\"edit_file\""));
        assert!(prompt.contains("<tool name=\"write_file\""));
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

        assert!(prompt.contains("<tool name=\"shell_use\""));
        assert!(prompt.contains("Stateful PTY/REPL/TUI"));
        assert!(prompt.contains("<tool name=\"shell_exec\""));
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

        assert!(prompt.contains("<tool name=\"shell_exec\""));
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
        let project = prompt.find("<project_context>").expect("project context");
        assert!(language < project);
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
        assert!(prompt.len() < 12_000, "static prompt is {} bytes", prompt.len());
    }

    #[test]
    fn xml_tool_grouping_structure_is_correct() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &[
                "read_file",
                "grep",
                "edit_file",
                "write_file",
                "shell_exec",
                "web_search",
            ]
            .map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        // Verify XML tool grouping structure
        assert!(prompt.contains("<tool_calling note="));
        assert!(prompt.contains("<tool_group name=\"read\">"));
        assert!(prompt.contains("<tool_group name=\"write\">"));
        assert!(prompt.contains("<tool_group name=\"exec\">"));
        assert!(prompt.contains("<tool_group name=\"web\">"));
        assert!(prompt.contains("<tool_group name=\"mcp\">"));

        // Verify tool elements within groups
        assert!(prompt.contains("<tool name=\"read_file\""));
        assert!(prompt.contains("<tool name=\"grep\""));
        assert!(prompt.contains("<tool name=\"edit_file\""));
        assert!(prompt.contains("<tool name=\"write_file\""));
        assert!(prompt.contains("<tool name=\"shell_exec\""));
        assert!(prompt.contains("<tool name=\"web_search\""));

        // Verify rule element in write group
        assert!(prompt.contains("<rule>content_hash"));
    }

    #[test]
    fn error_recovery_section_is_present() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file", "grep"].map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(prompt.contains("## Error Recovery"));
        assert!(prompt.contains("Tool fails with transient error"));
        assert!(prompt.contains("Test fails → check build/test output"));
        assert!(prompt.contains("Context refresh only when"));
        assert!(prompt.contains("You are a fast and decisive coding agent"));
    }

    #[test]
    fn output_format_section_is_present() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file", "grep"].map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(prompt.contains("## Output Format"));
        assert!(prompt.contains("Code changes: Show diff summary"));
        assert!(prompt.contains("Test results: Pass/fail count"));
        assert!(prompt.contains("Build errors: First 10 lines"));
        assert!(prompt.contains("Ask for full output when"));
        assert!(prompt.contains("You are a fast and decisive coding agent"));
    }

    #[test]
    fn parallel_tool_calls_section_is_present() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file", "grep"].map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        assert!(prompt.contains("## Parallel Tool Calls"));
        assert!(prompt.contains("Batch up to 5 independent read operations"));
        assert!(prompt.contains("Never parallelize write operations"));
        assert!(prompt.contains("Background jobs: max 2 concurrent"));
        assert!(prompt.contains("You are a fast and decisive coding agent"));
    }

    #[test]
    fn skills_format_uses_inline_attributes() {
        let mut resources = AgentHarnessResources::default();
        resources.skills.push(elph_agent::harness::Skill {
            name: "test-skill".to_string(),
            description: "Test skill for unit testing".to_string(),
            content: String::new(),
            file_path: "/path/to/skill".to_string(),
            disable_model_invocation: false,
            license: None,
            compatibility: None,
            metadata: None,
            allowed_tools: None,
            argument_hint: None,
        });

        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &resources,
            &["read_file"].map(String::from),
            None,
            &CodingPromptOptions::new(AgentMode::Build),
        )
        .expect("prompt");

        // Verify new skills format with inline attributes
        assert!(prompt.contains("<available_skills note="));
        assert!(prompt.contains("<skill name=\"test-skill\""));
        assert!(prompt.contains("path=\"/path/to/skill\""));

        // Verify old format is not used
        assert!(!prompt.contains("<skill name=\"test-skill\" location="));
        assert!(!prompt.contains("</skill>test-skill"));

        // Verify skills appear after tool_calling, not before it
        let tool_calling = prompt.find("<tool_calling").expect("tool_calling section");
        let available_skills = prompt.find("<available_skills").expect("available_skills section");
        assert!(tool_calling < available_skills, "available_skills must come after tool_calling");
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
        assert!(with_subagents.contains("exclusive write scope"));
    }

    #[test]
    fn intercom_prompt_is_not_coding_base() {
        let prompt = build_intercom_system_prompt(Some("calm-fox"), Some("brave-owl")).expect("prompt");
        assert!(prompt.contains("calm-fox"));
        assert!(prompt.contains("brave-owl"));
        assert!(prompt.contains("worker_reply"));
        assert!(prompt.contains("separate answer loop"));
        assert!(!prompt.contains("## Working loop"));
        assert!(!prompt.contains("edit_file"));
        assert!(!prompt.contains("todo_write"));
        assert!(!prompt.contains("You are a fast and decisive coding agent"));
    }

    #[test]
    fn coding_prompt_points_inbound_at_intercom_loop() {
        let prompt = build_coding_system_prompt(
            Path::new("/tmp/project"),
            &AgentHarnessResources::default(),
            &["read_file".to_string()],
            None,
            &CodingPromptOptions::new(AgentMode::Build).with_worker_name("calm-fox"),
        )
        .expect("prompt");
        assert!(prompt.contains("separate intercom loop"));
        assert!(!prompt.contains("Reply to inbound worker messages as normal text"));
    }
}
