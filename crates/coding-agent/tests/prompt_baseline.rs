//! Prompt token baseline measurements for the system-prompt revamp plan.
//!
//! Renders `build_coding_system_prompt` for each agent mode with a realistic
//! skill + tool set and prints token counts (using the same tokenx estimator the
//! compaction module uses) for the full prompt, `<available_skills>`, and
//! `<available_tools>`. Requires `crates/elph-agent` (dev-dependency) for the
//! estimator and tool types.
//!
//! Run with: `cargo test -p elph --test prompt_baseline -- --nocapture`

use std::path::Path;

use elph_agent::{AgentHarnessResources, Skill};

use elph::agent::prompt::build_coding_system_prompt;
use elph::types::AgentMode;

fn skill(name: &str, description: &str, scope: Option<&str>) -> Skill {
    let mut skill = Skill {
        name: name.to_string(),
        description: description.to_string(),
        content: "# Skill\n\nBody\n".to_string(),
        file_path: format!("/skills/{name}/SKILL.md"),
        disable_model_invocation: false,
        license: None,
        compatibility: None,
        metadata: None,
        allowed_tools: None,
        argument_hint: None,
    };
    if let Some(scope) = scope {
        skill.metadata = Some(std::collections::HashMap::from([(
            "scope".to_string(),
            serde_json::Value::String(scope.to_string()),
        )]));
    }
    skill
}

fn skills_fixture() -> Vec<Skill> {
    vec![
        // Global (works everywhere).
        skill(
            "animation-vocabulary",
            "Reverse-lookup glossary mapping vague motion descriptions to exact animation terms.",
            Some("global"),
        ),
        skill(
            "apple-design",
            "Apple's approach to interface design and physical motion, translated for the web.",
            Some("global"),
        ),
        skill(
            "create-skill",
            "Create a new Elph skill (SKILL.md + optional scripts/references).",
            None,
        ),
        skill(
            "emil-design-eng",
            "Emil Kowalski's philosophy on UI polish, component design, animation decisions.",
            Some("global"),
        ),
        skill("finalize", "Finish a feature: merge a worktree back to main, clean up.", None),
        skill(
            "go-api-perf",
            "Profile-gate and apply 7 Go API performance patterns.",
            Some("global"),
        ),
        skill(
            "go-http-safeguards",
            "Harden a Go net/http server against 7 reliability safeguards.",
            Some("global"),
        ),
        skill(
            "improve-animations",
            "Survey a codebase's animation code and produce prioritized fix plans.",
            Some("global"),
        ),
        skill("math-expert", "Expert mathematician for basic arithmetic.", None),
        skill(
            "openui",
            "Build, debug, integrate, migrate, or document OpenUI and streaming generative UI.",
            Some("global"),
        ),
        skill(
            "pi-port-gap",
            "Analyze pi → elph porting gaps and Elph-specific implementation differences.",
            None,
        ),
        skill(
            "rust-dep-audit",
            "Audit Rust dependencies for security advisories, license compliance, staleness.",
            Some("global"),
        ),
        skill(
            "rust-lean-refactor",
            "Reorganize Rust code to be lean, clean, and non-bloated.",
            Some("project"),
        ),
        skill(
            "rust-verify-harden",
            "Verify build quality gates and harden Rust code (memory safety, Turso/SQLite usage).",
            Some("project"),
        ),
        skill(
            "test-agent-tools",
            "Probe, test, and document every tool the agent harness has access to.",
            None,
        ),
        skill(
            "tui-design",
            "Guide terminal UI development with iocraft and CLI/slash command UX.",
            Some("project"),
        ),
        skill(
            "update-models",
            "Refresh elph-ai chat model catalogs from models.dev with optional live pricing.",
            Some("project"),
        ),
        skill(
            "cargo-workspace-hygiene",
            "Audit a Rust workspace for dependency hygiene: duplicate versions, unused deps, feature bloat.",
            Some("project"),
        ),
    ]
}

/// Native + memory tools (no MCP) — the same profile the current elph session runs.
const NATIVE_TOOL_NAMES: &[&str] = &[
    "ask_user_question",
    "copy_path",
    "create_dir",
    "create_goal",
    "delete_path",
    "edit_file",
    "find_path",
    "followup_task",
    "get_goal",
    "grep",
    "list_agents",
    "list_available_tools",
    "list_dir",
    "memory_contradict",
    "memory_end_task",
    "memory_recent",
    "memory_report",
    "memory_search",
    "memory_start_task",
    "memory_status",
    "move_path",
    "read_file",
    "request_mode_change",
    "send_message",
    "set_goal_budget",
    "shell_exec",
    "spawn_agent",
    "update_goal",
    "wait_agent",
    "web_extract",
    "web_fetch",
    "web_search",
    "write_file",
];

fn count_tokens(text: &str) -> u64 {
    elph_ai::utils::estimate::count_tokens_text(text)
}

fn prompt_tokens(mode: AgentMode) -> usize {
    let resources = AgentHarnessResources {
        skills: skills_fixture(),
        ..Default::default()
    };
    let prompt = build_coding_system_prompt(
        Path::new("/tmp/elph-project"),
        &resources,
        &NATIVE_TOOL_NAMES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        None,
        mode,
        "",
        true,
        true,
    )
    .expect("prompt renders");

    println!("=== mode {} ===", mode.label());
    println!("full prompt bytes: {}", prompt.len());
    println!("full prompt tokens: {}", count_tokens(&prompt));

    // Block token counts.
    let skills_block = "<available_skills>";
    let skills_tokens = if let Some(start) = prompt.find(skills_block) {
        let end = prompt[start..]
            .find("</available_skills>")
            .map(|i| start + i + "</available_skills>".len())
            .unwrap_or(prompt.len());
        count_tokens(&prompt[start..end])
    } else {
        0
    };
    println!("<available_skills> tokens: {skills_tokens}");

    let tools_tokens = if let Some(start) = prompt.find("<available_tools>") {
        let end = prompt[start..]
            .find("</available_tools>")
            .map(|i| start + i + "</available_tools>".len())
            .unwrap_or(prompt.len());
        count_tokens(&prompt[start..end])
    } else {
        0
    };
    println!("<available_tools> tokens: {tools_tokens}");
    println!();

    prompt.len()
}

#[test]
fn render_prompt_baseline_all_modes() {
    for mode in [AgentMode::Build, AgentMode::Plan, AgentMode::Brave] {
        prompt_tokens(mode);
    }
}
