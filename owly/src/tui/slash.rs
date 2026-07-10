//! Slash-command helpers for the Owly interactive shell.

/// Owly slash command names (without leading `/`).
pub const OWLY_SLASH_COMMANDS: &[&str] = &["help", "init", "update", "history", "restore", "clear", "exit", "quit"];

/// Normalize prompt text before dispatch so shell handlers receive `/command` form.
///
/// [`handle_prompt_input`] strips the `/` prefix on submit; this restores it for known commands.
pub fn normalize_dispatch_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('/') {
        return trimmed.to_string();
    }
    let head = trimmed.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    if OWLY_SLASH_COMMANDS.contains(&head.as_str()) {
        format!("/{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Returns true when the text is an Owly slash command (with or without `/`).
pub fn is_slash_command(text: &str) -> bool {
    let trimmed = text.trim();
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let head = body.split_whitespace().next().unwrap_or("");
    !head.is_empty() && OWLY_SLASH_COMMANDS.contains(&head.to_ascii_lowercase().as_str())
}

/// Whether the input should show the working activity indicator while dispatch runs.
pub fn input_shows_activity(text: &str) -> bool {
    !is_slash_command(text) || slash_command_shows_activity(text)
}

/// Slash commands that may run long enough to show the working activity indicator.
fn slash_command_shows_activity(text: &str) -> bool {
    if !is_slash_command(text) {
        return false;
    }
    let body = text.trim().strip_prefix('/').unwrap_or(text.trim());
    let head = body.split_whitespace().next().unwrap_or("");
    matches!(head.to_ascii_lowercase().as_str(), "init" | "update" | "restore")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_restores_slash_prefix() {
        assert_eq!(normalize_dispatch_text("init"), "/init");
        assert_eq!(normalize_dispatch_text("update docs"), "/update docs");
        assert_eq!(normalize_dispatch_text("/help"), "/help");
        assert_eq!(normalize_dispatch_text("hello"), "hello");
    }

    #[test]
    fn detects_slash_commands() {
        assert!(is_slash_command("/init"));
        assert!(is_slash_command("help"));
        assert!(!is_slash_command("explain the architecture"));
    }

    #[test]
    fn activity_for_chat_and_long_running_slash_commands() {
        assert!(input_shows_activity("explain the architecture"));
        assert!(input_shows_activity("/init"));
        assert!(input_shows_activity("update docs"));
        assert!(!input_shows_activity("/help"));
        assert!(!input_shows_activity("clear"));
    }
}
