//! Footer and header label formatting.

use crate::types::{AgentMode, ThinkingLevel};

/// Banner text when the user switches agent mode (Shift+Tab) — fixed above StatusRow.
pub fn agent_mode_change_notice(mode: AgentMode) -> String {
    format!("Agent mode: {}.", mode.label())
}

/// Banner text when agent mode cannot change because a turn is in progress.
pub fn agent_mode_busy_notice() -> String {
    "Can't change agent mode while the agent is busy.".to_string()
}

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

/// Git metadata for chrome (branch + worktree stats).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFooterInfo {
    pub branch: String,
    pub files_added: u32,
    pub lines_added: u32,
    pub files_deleted: u32,
    pub lines_deleted: u32,
}

/// Format worktree stats as `[+files/lines -files/lines]` (no zero-padding).
pub fn format_worktree_stats(files_added: u32, lines_added: u32, files_deleted: u32, lines_deleted: u32) -> String {
    format!("[+{files_added}/{lines_added} -{files_deleted}/{lines_deleted}]")
}

/// Project + branch for the editor bottom border: `~ name (branch)`.
pub fn editor_border_project_label(project_name: &str, branch: Option<&str>) -> String {
    match branch.map(str::trim).filter(|b| !b.is_empty()) {
        Some(branch) => format!("~ {project_name} ({branch})"),
        None => format!("~ {project_name}"),
    }
}

/// Agent mode segment on the status footer (capitalized: `Plan`, `Build`, …).
pub fn footer_mode_label(mode: AgentMode) -> String {
    mode.label().to_string()
}

/// Split `provider/model` into `(provider, model_id)`. Bare labels keep the whole string as model.
pub fn split_provider_model(model_label: &str) -> (Option<&str>, &str) {
    match model_label.rsplit_once('/') {
        Some((provider, model)) if !provider.is_empty() && !model.is_empty() => (Some(provider), model),
        _ => (None, model_label),
    }
}

/// Model id only (after last `/`), or the full label when no provider segment exists.
pub fn footer_model_name(model_label: &str) -> &str {
    split_provider_model(model_label).1
}

/// Model + thinking segment: `provider/model (thinking)` or `model (thinking)`.
pub fn footer_model_thinking_label(model_label: &str, thinking_level: ThinkingLevel) -> String {
    format!("{model_label} ({})", thinking_level.label())
}

/// Status footer left: `MODE · provider/model (thinking) [▣]`.
pub fn footer_status_left_label(
    mode: AgentMode,
    model_label: &str,
    thinking_level: ThinkingLevel,
    supports_images: bool,
) -> String {
    let mode = footer_mode_label(mode);
    let model_thinking = footer_model_thinking_label(model_label, thinking_level);
    if supports_images {
        format!("{}{}{} {}", mode, FOOTER_SEP, model_thinking, FOOTER_IMG_INDICATOR)
    } else {
        format!("{}{}{}", mode, FOOTER_SEP, model_thinking)
    }
}

/// Compact badge when native text-selection mode is active (`Ctrl+S`).
pub const FOOTER_SELECT_MODE_BADGE: &str = "sel";

/// Footer separator (middle dot).
pub const FOOTER_SEP: &str = " · ";

/// Footer image/vision indicator.
pub const FOOTER_IMG_INDICATOR: &str = "◐";

/// Status footer right: `turn: N · [+A/B -C/D]` (single-digit counts).
/// When fitting is tight, git yields before turn (see `fit_footer_status_right`).
///
/// With select mode: `sel · turn: N · [+A/B -C/D]`.
pub fn footer_status_right_label(turn: u32, git: Option<&GitFooterInfo>) -> String {
    footer_status_right_label_with_select(turn, git, false)
}

/// Like [`footer_status_right_label`], optionally prefixing the select-mode badge.
pub fn footer_status_right_label_with_select(turn: u32, git: Option<&GitFooterInfo>, select_mode: bool) -> String {
    let turn_s = format!("turn: {turn}");
    let base = match footer_git_stats_label(git) {
        Some(stats) => format!("{}{}{}", turn_s, FOOTER_SEP, stats),
        None => turn_s,
    };
    if select_mode {
        format!("{}{}{}", FOOTER_SELECT_MODE_BADGE, FOOTER_SEP, base)
    } else {
        base
    }
}

/// Git-only footer right segment (`[+A/B -C/D]`).
pub fn footer_git_stats_label(git: Option<&GitFooterInfo>) -> Option<String> {
    git.map(|git| format_worktree_stats(git.files_added, git.lines_added, git.files_deleted, git.lines_deleted))
}

/// Compact token count: `999`, `5K`, `272K`, `1M`, `1.5M` (capital K/M).
pub fn format_token_count(n: u64) -> String {
    const MILLION: u64 = 1_000_000;
    const THOUSAND: u64 = 1_000;
    if n >= MILLION {
        let whole = n / MILLION;
        let tenths = (n % MILLION) / 100_000;
        if tenths == 0 {
            format!("{whole}M")
        } else {
            format!("{whole}.{tenths}M")
        }
    } else if n >= THOUSAND {
        format!("{}K", n / THOUSAND)
    } else {
        format!("{n}")
    }
}

/// Always includes percentage and context limit: `48.2% (272K)`.
pub fn context_pct_limit_label(context_pct: f64, context_limit: u64) -> String {
    format!("{context_pct:.1}% ({})", format_token_count(context_limit))
}

pub fn context_usage_label(tokens_used: u64, context_pct: f64, context_limit: u64, display: &str) -> String {
    let used = format_token_count(tokens_used);
    let pct_limit = context_pct_limit_label(context_pct, context_limit);
    match display {
        "percentage" => pct_limit,
        "count" => format!("{used} ({})", format_token_count(context_limit)),
        _ => format!("{used} | {pct_limit}"),
    }
}

pub fn header_stats_label(
    cost_usd: f64,
    tokens_used: u64,
    context_pct: f64,
    context_limit: u64,
    display: &str,
) -> String {
    let cost = format!("${cost_usd:.4}");
    format!(
        "{cost} | {}",
        context_usage_label(tokens_used, context_pct, context_limit, display)
    )
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
    fn agent_mode_change_notice_uses_footer_label() {
        assert_eq!(agent_mode_change_notice(AgentMode::Plan), "Agent mode: Plan.");
        assert_eq!(agent_mode_change_notice(AgentMode::Brave), "Agent mode: Brave.");
        assert!(agent_mode_busy_notice().contains("busy"));
    }

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
    fn editor_border_project_label_uses_parens_for_branch() {
        assert_eq!(
            editor_border_project_label("my-app", Some("feat/cool-new-feature")),
            "~ my-app (feat/cool-new-feature)"
        );
        assert_eq!(editor_border_project_label("my-app", None), "~ my-app");
        assert_eq!(editor_border_project_label("my-app", Some("  ")), "~ my-app");
    }

    #[test]
    fn footer_status_left_includes_mode_model_thinking_and_optional_img() {
        assert_eq!(
            footer_status_left_label(AgentMode::Plan, "opencode/deepseek-v4-flash", ThinkingLevel::Xhigh, true),
            "Plan · opencode/deepseek-v4-flash (xhigh) ◐"
        );
        assert_eq!(
            footer_status_left_label(AgentMode::Build, "openai/gpt-5.6-luna", ThinkingLevel::High, false),
            "Build · openai/gpt-5.6-luna (high)"
        );
    }

    #[test]
    fn footer_model_thinking_label_wraps_level_in_parens() {
        assert_eq!(
            footer_model_thinking_label("opencode/deepseek-v4-flash", ThinkingLevel::Xhigh),
            "opencode/deepseek-v4-flash (xhigh)"
        );
    }

    #[test]
    fn split_provider_model_extracts_model_id() {
        assert_eq!(
            split_provider_model("opencode/deepseek-v4-flash"),
            (Some("opencode"), "deepseek-v4-flash")
        );
        assert_eq!(footer_model_name("opencode/deepseek-v4-flash"), "deepseek-v4-flash");
        assert_eq!(split_provider_model("No model selected"), (None, "No model selected"));
        assert_eq!(footer_model_name("solo-model"), "solo-model");
    }

    #[test]
    fn footer_status_right_uses_single_digit_counts() {
        let git = GitFooterInfo {
            branch: "main".to_string(),
            files_added: 0,
            lines_added: 0,
            files_deleted: 0,
            lines_deleted: 0,
        };
        assert_eq!(footer_status_right_label(0, Some(&git)), "turn: 0 · [+0/0 -0/0]");
        assert_eq!(footer_status_right_label(3, None), "turn: 3");
        assert_eq!(footer_status_right_label(12, Some(&git)), "turn: 12 · [+0/0 -0/0]");
        assert_eq!(
            footer_status_right_label_with_select(0, Some(&git), true),
            "sel · turn: 0 · [+0/0 -0/0]"
        );
        assert_eq!(footer_status_right_label_with_select(3, None, true), "sel · turn: 3");
    }

    #[test]
    fn format_worktree_stats_renders_file_and_line_counts() {
        assert_eq!(format_worktree_stats(99, 0, 0, 0), "[+99/0 -0/0]");
        assert_eq!(format_worktree_stats(2, 15, 1, 8), "[+2/15 -1/8]");
    }

    #[test]
    fn format_token_count_uses_k_and_m() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1_000), "1K");
        assert_eq!(format_token_count(272_000), "272K");
        assert_eq!(format_token_count(1_000_000), "1M");
        assert_eq!(format_token_count(1_500_000), "1.5M");
        assert_eq!(format_token_count(2_000_000), "2M");
    }

    #[test]
    fn context_usage_label_respects_display_mode() {
        assert_eq!(context_usage_label(131_000, 48.2, 272_000, "both"), "131K | 48.2% (272K)");
        assert_eq!(context_usage_label(131_000, 48.2, 272_000, "percentage"), "48.2% (272K)");
        assert_eq!(context_usage_label(131_000, 48.2, 272_000, "count"), "131K (272K)");
        assert_eq!(context_pct_limit_label(48.2, 272_000), "48.2% (272K)");
    }
}
