//! Tests for Owly agent module.

use owly::agent::*;
use owly::mode::WikiContext;

#[test]
fn test_prepare_init_command() {
    let ctx = WikiContext::code(std::env::current_dir().unwrap());
    let (system_prompt, user_prompt) = prepare_init_command(&ctx, None, "big-pickle");

    assert!(!system_prompt.is_empty());
    assert!(!user_prompt.is_empty());
    assert!(system_prompt.contains("initial documentation run"));
    assert!(user_prompt.contains("Initialize Owly documentation"));
    assert!(user_prompt.contains("Wiki brief:"));
    assert!(system_prompt.contains("INSTRUCTIONS.md"));
}

#[test]
fn test_prepare_init_command_with_user_message() {
    let ctx = WikiContext::code(std::env::current_dir().unwrap());
    let (system_prompt, user_prompt) = prepare_init_command(&ctx, Some("Focus on API"), "big-pickle");

    assert!(!system_prompt.is_empty());
    assert!(user_prompt.contains("Focus on API"));
}

#[test]
fn test_prepare_update_command_no_metadata() {
    let ctx = WikiContext::code(std::env::current_dir().unwrap());
    let (system_prompt, user_prompt) = prepare_update_command(&ctx, None, "big-pickle", None);

    assert!(!system_prompt.is_empty());
    assert!(!user_prompt.is_empty());
    assert!(system_prompt.contains("maintenance update run"));
    assert!(user_prompt.contains("Update the existing Owly documentation"));
}

#[test]
fn test_prepare_update_command_with_metadata() {
    use chrono::Utc;
    use owly::metadata::UpdateMetadata;

    let ctx = WikiContext::code(std::env::current_dir().unwrap());
    let metadata = UpdateMetadata {
        updated_at: Utc::now(),
        command: "init".to_string(),
        git_head: Some("abc123".to_string()),
        model: "opencode/big-pickle".to_string(),
    };

    let (system_prompt, user_prompt) = prepare_update_command(&ctx, None, "big-pickle", Some(&metadata));

    assert!(!system_prompt.is_empty());
    assert!(user_prompt.contains("abc123"));
}

#[test]
fn test_prepare_init_command_includes_wiki_brief_from_instructions_file() {
    use owly::instructions::{read_repository_instructions, save_repository_instructions};
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    save_repository_instructions(dir.path(), "Focus on Rust agent runtime.").unwrap();
    let ctx = WikiContext::code(dir.path());
    let (_, user_prompt) = prepare_init_command(&ctx, None, "big-pickle");
    assert!(user_prompt.contains("Focus on Rust agent runtime."));
    assert_eq!(
        read_repository_instructions(dir.path()).as_deref(),
        Some("Focus on Rust agent runtime.")
    );
}

#[test]
fn test_prepare_chat_command() {
    let ctx = WikiContext::code(std::env::current_dir().unwrap());
    let (system_prompt, user_prompt) = prepare_chat_command(&ctx, "What can you do?");

    assert!(!system_prompt.is_empty());
    assert!(!user_prompt.is_empty());
    assert!(system_prompt.contains("interactive chat turn"));
    assert!(user_prompt.contains("What can you do?"));
}
