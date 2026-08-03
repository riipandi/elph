#![cfg(feature = "prompt-templates")]

use elph_agent::{PromptAssemblyMode, SystemPromptBuilder, SystemPromptTemplateContext, tool_names_context};

#[test]
fn extend_mode_renders_base_persona() {
    let context = SystemPromptTemplateContext {
        persona: "Test persona".to_string(),
        working_directory: Some("/tmp/project".to_string()),
        current_date: Some("2026-07-17".to_string()),
        ..Default::default()
    };

    let prompt = SystemPromptBuilder::new()
        .mode(PromptAssemblyMode::Extend)
        .context(context)
        .render()
        .expect("render");

    assert!(prompt.contains("Test persona"));
    assert!(prompt.contains("/tmp/project"));
    assert!(prompt.contains("2026-07-17"));
}

#[test]
fn full_mode_renders_domain_body() {
    let context = SystemPromptTemplateContext::default();

    let prompt = SystemPromptBuilder::new()
        .mode(PromptAssemblyMode::Full)
        .context(context)
        .domain_body("Intro.\nread=read_file")
        .render()
        .expect("render");

    assert!(prompt.contains("read=read_file"));
}

#[test]
fn extend_mode_appends_pi_project_context() {
    let context = SystemPromptTemplateContext {
        agents_md: "Follow AGENTS.md.".to_string(),
        ..Default::default()
    };

    let prompt = SystemPromptBuilder::new()
        .mode(PromptAssemblyMode::Extend)
        .context(context)
        .render()
        .expect("render");

    assert!(prompt.contains("<project_context>"));
    assert!(prompt.contains("<project_instructions path=\"AGENTS.md\">"));
    assert!(prompt.contains("Follow AGENTS.md."));
}

#[test]
fn tool_names_context_omits_inactive_tools() {
    let tools = tool_names_context(&["grep".to_string()]);
    assert_eq!(tools.grep, "grep");
    assert!(tools.read_file.is_empty());
}
