//! Tests for Owly agent module.

use owly::agent::*;

#[test]
fn test_prepare_init_command() {
    let cwd = std::env::current_dir().unwrap();
    let (system_prompt, user_prompt) = prepare_init_command(&cwd, None, "big-pickle");

    assert!(!system_prompt.is_empty());
    assert!(!user_prompt.is_empty());
    assert!(system_prompt.contains("initial documentation run"));
    assert!(user_prompt.contains("Initialize Owly documentation"));
}

#[test]
fn test_prepare_init_command_with_user_message() {
    let cwd = std::env::current_dir().unwrap();
    let (system_prompt, user_prompt) = prepare_init_command(&cwd, Some("Focus on API"), "big-pickle");

    assert!(!system_prompt.is_empty());
    assert!(user_prompt.contains("Focus on API"));
}

#[test]
fn test_prepare_update_command_no_metadata() {
    let cwd = std::env::current_dir().unwrap();
    let (system_prompt, user_prompt) = prepare_update_command(&cwd, None, "big-pickle", None);

    assert!(!system_prompt.is_empty());
    assert!(!user_prompt.is_empty());
    assert!(system_prompt.contains("maintenance update run"));
    assert!(user_prompt.contains("Update the existing Owly documentation"));
}

#[test]
fn test_prepare_update_command_with_metadata() {
    use chrono::Utc;
    use owly::metadata::UpdateMetadata;

    let cwd = std::env::current_dir().unwrap();
    let metadata = UpdateMetadata {
        updated_at: Utc::now(),
        command: "init".to_string(),
        git_head: Some("abc123".to_string()),
        model: "opencode/big-pickle".to_string(),
    };

    let (system_prompt, user_prompt) = prepare_update_command(&cwd, None, "big-pickle", Some(&metadata));

    assert!(!system_prompt.is_empty());
    assert!(user_prompt.contains("abc123"));
}

#[test]
fn test_prepare_chat_command() {
    let (system_prompt, user_prompt) = prepare_chat_command("What can you do?");

    assert!(!system_prompt.is_empty());
    assert!(!user_prompt.is_empty());
    assert!(system_prompt.contains("interactive chat turn"));
    assert!(user_prompt.contains("What can you do?"));
}
