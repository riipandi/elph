//! Tests for Owly prompts module.

use owly::constants::OWLY_DIR;
use owly::prompts::*;

#[test]
fn test_create_system_prompt_contains_owly_dir() {
    let prompt = create_system_prompt();
    assert!(prompt.contains(OWLY_DIR));
}

#[test]
fn test_create_system_prompt_contains_tool_names() {
    let prompt = create_system_prompt();
    // Should reference elph-agent tool names
    assert!(prompt.contains("read"));
    assert!(prompt.contains("write"));
    assert!(prompt.contains("edit"));
    assert!(prompt.contains("bash"));
    assert!(prompt.contains("grep"));
    assert!(prompt.contains("find"));
    assert!(prompt.contains("ls"));
}

#[test]
fn test_create_system_prompt_contains_security_rules() {
    let prompt = create_system_prompt();
    assert!(prompt.contains("Security and privacy rules"));
    assert!(prompt.contains("Do not read .env files"));
}

#[test]
fn test_create_system_prompt_contains_documentation_goals() {
    let prompt = create_system_prompt();
    assert!(prompt.contains("Documentation goals"));
    assert!(prompt.contains("quickstart.md"));
}

#[test]
fn test_create_system_prompt_contains_frontmatter_rules() {
    let prompt = create_system_prompt();
    assert!(prompt.contains("Frontmatter rules"));
    assert!(prompt.contains("YAML frontmatter"));
}

#[test]
fn test_create_init_prompt() {
    let prompt = create_init_prompt("", None, None);
    assert!(prompt.contains("Initialize Owly documentation"));
    assert!(prompt.contains(OWLY_DIR));
}

#[test]
fn test_create_init_prompt_with_user_message() {
    let prompt = create_init_prompt("", None, Some("Focus on API routes"));
    assert!(prompt.contains("Initialize Owly documentation"));
    assert!(prompt.contains("Focus on API routes"));
}

#[test]
fn test_create_update_prompt_no_metadata() {
    let prompt = create_update_prompt(None, "git log output", None, None);
    assert!(prompt.contains("Update the existing Owly documentation"));
    assert!(prompt.contains("No previous Owly update metadata"));
    assert!(prompt.contains("git log output"));
}

#[test]
fn test_create_update_prompt_with_metadata() {
    use chrono::Utc;
    use owly::metadata::UpdateMetadata;

    let metadata = UpdateMetadata {
        updated_at: Utc::now(),
        command: "init".to_string(),
        git_head: Some("abc123".to_string()),
        model: "opencode/big-pickle".to_string(),
    };

    let prompt = create_update_prompt(Some(&metadata), "git diff output", None, None);
    assert!(prompt.contains("Update the existing Owly documentation"));
    assert!(prompt.contains("abc123"));
    assert!(prompt.contains("git diff output"));
}

#[test]
fn test_create_update_prompt_with_user_message() {
    let prompt = create_update_prompt(None, "git log", None, Some("Update API docs"));
    assert!(prompt.contains("Update the existing Owly documentation"));
    assert!(prompt.contains("Update API docs"));
}

#[test]
fn test_create_chat_prompt() {
    let prompt = create_chat_prompt("What can you do?");
    assert!(prompt.contains("interactive chat turn"));
    assert!(prompt.contains("What can you do?"));
}

#[test]
fn test_create_chat_prompt_no_documentation_modification() {
    let prompt = create_chat_prompt("test");
    assert!(prompt.contains("Do not create or update Owly documentation"));
}
