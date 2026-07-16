//! Footer and header label formatting.

use crate::agent::{DEFAULT_MODEL_ID, DEFAULT_PROVIDER};
use crate::platform::Paths;
use crate::types::ThinkingLevel;

pub fn session_label(session_id: &str, mcp_connected: usize, skills_count: usize) -> String {
    format!("Session: {session_id} | MCP: {mcp_connected} | Skills: {skills_count}")
}

pub fn project_footer_label(paths: &Paths, branch: Option<&str>) -> String {
    let name = paths.project_dir().file_name().and_then(|s| s.to_str()).unwrap_or("?");
    let branch = branch.filter(|b| !b.is_empty()).unwrap_or("main");
    format!("~ {name} [{branch}]")
}

pub fn footer_left_label(project_line: &str, turn: u32) -> String {
    format!("{project_line} | turn: {turn}")
}

pub fn format_tokens_k(n: u64) -> String {
    format!("{}k", n / 1000)
}

pub fn header_stats_label(
    cost_usd: f64,
    tokens_used: u64,
    context_pct: f64,
    context_limit: u64,
    display: &str,
) -> String {
    let cost = format!("${cost_usd:.2}");
    let used = format_tokens_k(tokens_used);
    let limit = format_tokens_k(context_limit);
    let pct = format!("{context_pct:.1}%");
    match display {
        "percentage" => format!("{cost} | {pct} ({limit})"),
        "count" => format!("{cost} | {used} ({limit})"),
        _ => format!("{cost} | {used} | {pct} ({limit})"),
    }
}

pub fn footer_right_label(model_label: &str, thinking_level: ThinkingLevel, supports_images: bool) -> String {
    if supports_images {
        format!("IMG | {model_label} | {}", thinking_level.label())
    } else {
        format!("{model_label} | {}", thinking_level.label())
    }
}

pub fn model_footer_label(provider_id: Option<&str>, model_id: Option<&str>) -> String {
    let provider = provider_id.unwrap_or(DEFAULT_PROVIDER);
    let model = model_id.unwrap_or(DEFAULT_MODEL_ID);
    format!("{provider}/{model}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_footer_label_falls_back_to_opencode_big_pickle() {
        assert_eq!(model_footer_label(None, None), "opencode/big-pickle");
    }

    #[test]
    fn model_footer_label_uses_session_override() {
        assert_eq!(
            model_footer_label(Some("anthropic"), Some("claude-sonnet-4")),
            "anthropic/claude-sonnet-4"
        );
    }

    #[test]
    fn session_label_includes_mcp_and_skills() {
        assert_eq!(
            session_label("abc123", 2, 5),
            "Session: abc123 | MCP: 2 | Skills: 5"
        );
    }

    #[test]
    fn project_footer_label_uses_git_branch() {
        let paths = Paths::from_dirs(
            std::path::PathBuf::from("/tmp/home"),
            std::path::PathBuf::from("/tmp/data"),
            std::path::PathBuf::from("/tmp/my-project"),
        );
        assert_eq!(
            project_footer_label(&paths, Some("refactor-tui")),
            "~ my-project [refactor-tui]"
        );
    }

    #[test]
    fn footer_left_appends_turn_count() {
        assert_eq!(footer_left_label("~ elph [main]", 0), "~ elph [main] | turn: 0");
    }

    #[test]
    fn footer_right_label_omits_img_when_unsupported() {
        assert_eq!(
            footer_right_label("opencode/big-pickle", ThinkingLevel::High, false),
            "opencode/big-pickle | high"
        );
        assert_eq!(
            footer_right_label("openai/gpt-4.1", ThinkingLevel::High, true),
            "IMG | openai/gpt-4.1 | high"
        );
    }
}
