//! Footer and header label formatting.

use crate::platform::Paths;
use crate::types::ThinkingLevel;

pub fn session_header_segments(session_id: &str, mcp_connected: usize, skills_count: usize) -> [String; 3] {
    [
        format!("Session: {session_id}"),
        format!("MCP: {mcp_connected}"),
        format!("Skills: {skills_count}"),
    ]
}

pub fn session_label(session_id: &str, mcp_connected: usize, skills_count: usize) -> String {
    session_header_segments(session_id, mcp_connected, skills_count).join(" | ")
}

/// Git metadata shown in the footer left segment (omitted outside a git work tree).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFooterInfo {
    pub branch: String,
    pub files_added: u32,
    pub lines_added: u32,
    pub files_deleted: u32,
    pub lines_deleted: u32,
}

/// Format worktree stats as `[+files/lines -files/lines]` (lines zero-padded to 2 digits).
pub fn format_worktree_stats(files_added: u32, lines_added: u32, files_deleted: u32, lines_deleted: u32) -> String {
    format!("[+{files_added}/{lines_added:02} -{files_deleted}/{lines_deleted:02}]")
}

/// Project directory line for the footer: `~ name`, with optional `[branch] [+A/B -C/D]`.
pub fn project_footer_label(paths: &Paths, git: Option<&GitFooterInfo>) -> String {
    let name = paths.project_dir().file_name().and_then(|s| s.to_str()).unwrap_or("?");
    let mut line = format!("~ {name}");
    if let Some(git) = git {
        let branch = git.branch.trim();
        if !branch.is_empty() {
            line.push_str(&format!(" [{branch}]"));
        }
        line.push(' ');
        line.push_str(&format_worktree_stats(
            git.files_added,
            git.lines_added,
            git.files_deleted,
            git.lines_deleted,
        ));
    }
    line
}

pub fn footer_left_label(project_line: &str, turn: u32) -> String {
    format!("{project_line} | turn: {turn}")
}

pub fn format_tokens_k(n: u64) -> String {
    format!("{}k", n / 1000)
}

pub fn context_usage_label(tokens_used: u64, context_pct: f64, context_limit: u64, display: &str) -> String {
    let used = format_tokens_k(tokens_used);
    let limit = format_tokens_k(context_limit);
    let pct = format!("{context_pct:.1}%");
    match display {
        "percentage" => format!("{pct} ({limit})"),
        "count" => format!("{used} ({limit})"),
        _ => format!("{used} | {pct} ({limit})"),
    }
}

pub fn header_stats_label(
    cost_usd: f64,
    tokens_used: u64,
    context_pct: f64,
    context_limit: u64,
    display: &str,
) -> String {
    let cost = format!("${cost_usd:.2}");
    format!(
        "{cost} | {}",
        context_usage_label(tokens_used, context_pct, context_limit, display)
    )
}

pub fn footer_right_label(model_label: &str, thinking_level: ThinkingLevel, supports_images: bool) -> String {
    if supports_images {
        format!("IMG | {model_label} | {}", thinking_level.label())
    } else {
        format!("{model_label} | {}", thinking_level.label())
    }
}

pub fn model_footer_label(provider_id: Option<&str>, model_id: Option<&str>) -> String {
    match (provider_id, model_id) {
        (Some(provider), Some(model)) => format!("{provider}/{model}"),
        _ => "No model selected".to_string(),
    }
}

/// Display name for the footer when a model is explicitly selected.
pub fn model_display_label(provider_id: &str, model_id: &str) -> String {
    elph_ai::get_builtin_model(provider_id, model_id)
        .map(|model| format!("{} [{provider_id}]", model.name))
        .unwrap_or_else(|| format!("{provider_id}/{model_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_footer_label_shows_unselected_when_missing() {
        assert_eq!(model_footer_label(None, None), "No model selected");
        assert_eq!(model_footer_label(Some("anthropic"), None), "No model selected");
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
        assert_eq!(session_label("abc123", 2, 5), "Session: abc123 | MCP: 2 | Skills: 5");
    }

    #[test]
    fn project_footer_label_includes_branch_and_diff_stats_in_git_repo() {
        let paths = Paths::from_dirs(
            std::path::PathBuf::from("/tmp/home"),
            std::path::PathBuf::from("/tmp/data"),
            std::path::PathBuf::from("/tmp/my-project"),
        );
        let git = GitFooterInfo {
            branch: "refactor-tui".to_string(),
            files_added: 99,
            lines_added: 0,
            files_deleted: 0,
            lines_deleted: 0,
        };
        assert_eq!(
            project_footer_label(&paths, Some(&git)),
            "~ my-project [refactor-tui] [+99/00 -0/00]"
        );
    }

    #[test]
    fn project_footer_label_omits_git_segments_outside_repo() {
        let paths = Paths::from_dirs(
            std::path::PathBuf::from("/tmp/home"),
            std::path::PathBuf::from("/tmp/data"),
            std::path::PathBuf::from("/tmp/my-project"),
        );
        assert_eq!(project_footer_label(&paths, None), "~ my-project");
    }

    #[test]
    fn footer_left_appends_turn_count() {
        assert_eq!(
            footer_left_label("~ elph [main] [+3/42 -1/07]", 0),
            "~ elph [main] [+3/42 -1/07] | turn: 0"
        );
    }

    #[test]
    fn format_worktree_stats_renders_file_and_line_counts() {
        assert_eq!(format_worktree_stats(99, 0, 0, 0), "[+99/00 -0/00]");
        assert_eq!(format_worktree_stats(2, 15, 1, 8), "[+2/15 -1/08]");
    }

    #[test]
    fn context_usage_label_respects_display_mode() {
        assert_eq!(context_usage_label(131_000, 48.2, 272_000, "both"), "131k | 48.2% (272k)");
        assert_eq!(context_usage_label(131_000, 48.2, 272_000, "percentage"), "48.2% (272k)");
        assert_eq!(context_usage_label(131_000, 48.2, 272_000, "count"), "131k (272k)");
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
