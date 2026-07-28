//! Progressive chrome fitting — drops or truncates rightmost segments first.

use crate::tui::labels::{
    FOOTER_SELECT_MODE_BADGE, GitFooterInfo, context_pct_limit_label, editor_border_project_label, footer_mode_label,
    footer_model_name, footer_model_thinking_label, footer_status_left_label, footer_status_right_label,
    footer_status_right_label_with_select, format_token_count, session_header_segments, session_label,
};
use crate::types::{AgentMode, ThinkingLevel};
use elph_tui::utils::{display_width, truncate_with_ellipsis};

use super::stats::{ChromeStats, header_stats_from_chrome};

/// Join segments left-to-right; drop trailing segments until the line fits `max_width`.
pub fn join_segments_fit(segments: &[String], max_width: usize, separator: &str) -> String {
    if segments.is_empty() {
        return String::new();
    }
    if max_width == 0 {
        return String::new();
    }
    let mut end = segments.len();
    while end > 1 {
        let joined = segments[..end].join(separator);
        if display_width(&joined) <= max_width {
            return joined;
        }
        end -= 1;
    }
    truncate_with_ellipsis(&segments[0], max_width)
}

/// Pick the first candidate that fits; truncate the last candidate if none fit.
pub fn pick_fitting_label(candidates: &[String], max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    for candidate in candidates {
        if display_width(candidate) <= max_width {
            return candidate.clone();
        }
    }
    candidates
        .last()
        .map(|c| truncate_with_ellipsis(c, max_width))
        .unwrap_or_default()
}

/// Budget per side in a two-column chrome row (`screen_width` minus horizontal padding).
pub fn chrome_half_width(screen_width: u16) -> usize {
    screen_width.saturating_sub(2).max(1) as usize / 2
}

/// Footer column budgets: left (mode+model) wins when the row is tight.
///
/// `min_left` is the ideal width for `MODE | model` (untruncated). When that
/// exceeds half the row, left steals from right so git/turn can yield first.
pub fn chrome_footer_widths(screen_width: u16, min_left: usize) -> (usize, usize) {
    let total = screen_width.saturating_sub(2).max(1) as usize;
    let half = (total / 2).max(1);
    let left = if min_left > half {
        min_left.min(total).max(1)
    } else {
        half.min(total)
    };
    let right = total.saturating_sub(left);
    (left, right)
}

/// Header left: Skills → MCP → truncate Session (session id kept as long as possible).
pub fn fit_session_header_left(
    session_id: &str,
    mcp_connected: usize,
    skills_count: usize,
    max_width: usize,
) -> String {
    let full = session_label(session_id, mcp_connected, skills_count);
    if display_width(&full) <= max_width {
        return full;
    }
    let segments = session_header_segments(session_id, mcp_connected, skills_count);
    join_segments_fit(&segments, max_width, " | ")
}

/// Header right progressive fit. **Context % and context length always preferred.**
///
/// Drop order when narrowing: cost → used count → keep `48.2% (272K)`.
pub fn fit_header_stats(
    cost_usd: f64,
    tokens_used: u64,
    context_pct: f64,
    context_limit: u64,
    display: &str,
    max_width: usize,
) -> String {
    let cost = format!("${cost_usd:.4}");
    let used = format_token_count(tokens_used);
    let pct_limit = context_pct_limit_label(context_pct, context_limit);
    let full = header_stats_from_chrome(
        &ChromeStats {
            cost_usd,
            tokens_used,
            context_pct,
            context_limit,
            ..ChromeStats::default()
        },
        display,
    );
    // Most complete → least optional; pct + limit is the last non-truncated candidate.
    let mut candidates = vec![full];
    match display {
        "percentage" => {
            candidates.push(format!("{cost} | {pct_limit}"));
            candidates.push(pct_limit.clone());
        }
        "count" => {
            candidates.push(format!("{cost} | {used} | {pct_limit}"));
            candidates.push(format!("{used} | {pct_limit}"));
            candidates.push(pct_limit.clone());
        }
        _ => {
            // both: $cost | used | pct (limit)
            candidates.push(format!("{cost} | {pct_limit}"));
            candidates.push(format!("{used} | {pct_limit}"));
            candidates.push(pct_limit.clone());
        }
    }
    pick_fitting_label(&candidates, max_width)
}

/// Editor bottom-border project label: drop branch first, then truncate `~ name`.
pub fn fit_editor_border_project(project_name: &str, branch: Option<&str>, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let full = editor_border_project_label(project_name, branch);
    let name_only = editor_border_project_label(project_name, None);
    pick_fitting_label(&[full, name_only, format!("~ {project_name}")], max_width)
}

/// Ideal left width for always-visible `MODE | model` (no thinking / IMG).
pub fn footer_mode_model_width(mode: AgentMode, model_label: &str) -> usize {
    let mode_s = footer_mode_label(mode);
    let model_id = footer_model_name(model_label);
    display_width(&format!("{mode_s} | {model_id}"))
}

/// Status footer left progressive fit. **Mode and model name always preferred.**
///
/// Drop order when narrowing: thinking → IMG → provider → truncate model.
/// Mode label is never dropped before model truncation.
pub fn fit_footer_status_left(
    mode: AgentMode,
    model_label: &str,
    thinking_level: ThinkingLevel,
    supports_images: bool,
    max_width: usize,
) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mode_s = footer_mode_label(mode);
    let model_id = footer_model_name(model_label);
    let with_thinking = footer_model_thinking_label(model_label, thinking_level);
    let model_id_thinking = footer_model_thinking_label(model_id, thinking_level);

    // Most complete → least optional; every candidate keeps MODE + model.
    let mut candidates = Vec::with_capacity(8);
    // 1. Full: MODE | provider/model (thinking) [| IMG]
    candidates.push(footer_status_left_label(mode, model_label, thinking_level, supports_images));
    // 2. Drop IMG, keep thinking + provider
    candidates.push(format!("{mode_s} | {with_thinking}"));
    // 3. Drop thinking: MODE | provider/model [| IMG]
    if supports_images {
        candidates.push(format!("{mode_s} | {model_label} | IMG"));
    }
    candidates.push(format!("{mode_s} | {model_label}"));
    // 4. Drop provider: MODE | model (thinking) [| IMG]
    if supports_images {
        candidates.push(format!("{mode_s} | {model_id_thinking} | IMG"));
    }
    candidates.push(format!("{mode_s} | {model_id_thinking}"));
    // 5. Drop thinking + provider + IMG: MODE | model (always last non-truncated)
    if supports_images {
        candidates.push(format!("{mode_s} | {model_id} | IMG"));
    }
    candidates.push(format!("{mode_s} | {model_id}"));
    pick_fitting_label(&candidates, max_width)
}

/// Status footer right progressive fit. **Git yields first when narrowing.**
///
/// Drop order: full (`turn | git`) → turn only → empty/truncated.
pub fn fit_footer_status_right(turn: u32, git: Option<&GitFooterInfo>, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let turn_s = format!("turn: {turn}");
    let full = footer_status_right_label(turn, git);
    let candidates = if git.is_some() { vec![full, turn_s] } else { vec![full] };
    pick_fitting_label(&candidates, max_width)
}

/// Like [`fit_footer_status_right`], with optional select-mode badge on the left of the right cluster.
///
/// Drop order: full (`sel | turn | git`) → `sel | turn` → `sel` → empty/truncated.
pub fn fit_footer_status_right_with_select(
    turn: u32,
    git: Option<&GitFooterInfo>,
    max_width: usize,
    select_mode: bool,
) -> String {
    if !select_mode {
        return fit_footer_status_right(turn, git, max_width);
    }
    if max_width == 0 {
        return String::new();
    }
    let turn_s = format!("{FOOTER_SELECT_MODE_BADGE} | turn: {turn}");
    let full = footer_status_right_label_with_select(turn, git, true);
    let mut candidates = if git.is_some() { vec![full, turn_s] } else { vec![full] };
    candidates.push(FOOTER_SELECT_MODE_BADGE.to_string());
    pick_fitting_label(&candidates, max_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_segments_drops_rightmost_first() {
        let segments = vec![
            "Session: abc".to_string(),
            "MCP: 2".to_string(),
            "Skills: 5".to_string(),
        ];
        let wide = join_segments_fit(&segments, 80, " | ");
        assert_eq!(wide, "Session: abc | MCP: 2 | Skills: 5");
        let narrow = join_segments_fit(&segments, 24, " | ");
        assert_eq!(narrow, "Session: abc | MCP: 2");
        let tight = join_segments_fit(&segments, 14, " | ");
        assert!(display_width(&tight) <= 14);
        assert!(tight.starts_with("Session"));
    }

    #[test]
    fn pick_fitting_label_falls_back_to_truncation() {
        let candidates = vec!["long header stats here".to_string(), "short".to_string()];
        let out = pick_fitting_label(&candidates, 6);
        assert!(display_width(&out) <= 6);
    }

    #[test]
    fn fit_header_stats_keeps_pct_and_context_limit() {
        let wide = fit_header_stats(0.12, 131_000, 48.2, 272_000, "both", 80);
        assert!(wide.contains("131K"));
        assert!(wide.contains("48.2%"));
        assert!(wide.contains("272K"));

        // Drop cost first; still keep pct + limit.
        let mid = fit_header_stats(0.12, 131_000, 48.2, 272_000, "both", 22);
        assert!(mid.contains("48.2%"));
        assert!(mid.contains("272K"));
        assert!(!mid.contains('$') || mid.contains("48.2%"));

        // Very tight: still prefers pct + limit over cost-only.
        let narrow = fit_header_stats(0.12, 131_000, 48.2, 272_000, "both", 16);
        assert_eq!(narrow, "48.2% (272K)");
    }

    #[test]
    fn fit_editor_border_project_drops_branch_before_name() {
        let wide = fit_editor_border_project("my-app", Some("feat/cool-new-feature"), 80);
        assert_eq!(wide, "~ my-app (feat/cool-new-feature)");
        let mid = fit_editor_border_project("my-app", Some("feat/cool-new-feature"), 12);
        assert_eq!(mid, "~ my-app");
        let tight = fit_editor_border_project("my-app", Some("feat/cool"), 6);
        assert!(display_width(&tight) <= 6);
    }

    #[test]
    fn fit_footer_status_left_always_keeps_mode_and_model() {
        let wide =
            fit_footer_status_left(AgentMode::Plan, "opencode/deepseek-v4-flash", ThinkingLevel::Xhigh, true, 80);
        assert!(wide.contains("IMG"));
        assert!(wide.contains("(xhigh)"));
        assert!(wide.contains("opencode/deepseek-v4-flash"));
        assert!(wide.starts_with("Plan"));

        // Drop thinking / optional bits; mode + model remain.
        let mid = fit_footer_status_left(AgentMode::Plan, "opencode/deepseek-v4-flash", ThinkingLevel::Xhigh, true, 36);
        assert!(mid.starts_with("Plan"));
        assert!(mid.contains("deepseek-v4-flash") || mid.contains("opencode"));

        // Narrower: drop provider; still Mode | model.
        let no_provider =
            fit_footer_status_left(AgentMode::Plan, "opencode/deepseek-v4-flash", ThinkingLevel::Xhigh, true, 28);
        assert!(no_provider.starts_with("Plan"));
        assert!(no_provider.contains("deepseek-v4-flash"));
        assert!(!no_provider.contains("opencode/"));

        // Very tight: still starts with mode; model may be truncated.
        let tight =
            fit_footer_status_left(AgentMode::Plan, "opencode/deepseek-v4-flash", ThinkingLevel::Xhigh, true, 12);
        assert!(display_width(&tight) <= 12);
        assert!(tight.starts_with("Plan"), "mode must remain: {tight}");
        assert!(tight.contains('|') || tight.contains('…'), "model segment: {tight}");
    }

    #[test]
    fn fit_footer_status_right_drops_git_before_turn() {
        let git = GitFooterInfo {
            branch: "main".to_string(),
            files_added: 3,
            lines_added: 42,
            files_deleted: 1,
            lines_deleted: 7,
        };
        let wide = fit_footer_status_right(2, Some(&git), 40);
        assert!(wide.contains("turn: 2"));
        assert!(wide.contains("[+3/42 -1/7]"));
        // Git yields first; keep turn.
        let narrow = fit_footer_status_right(2, Some(&git), 14);
        assert_eq!(narrow, "turn: 2");
        assert!(!narrow.contains('['));
        // Extremely tight: may truncate turn.
        let tight = fit_footer_status_right(2, Some(&git), 4);
        assert!(display_width(&tight) <= 4);
        assert!(!tight.contains('['));

        let sel_wide = fit_footer_status_right_with_select(0, Some(&git), 48, true);
        assert_eq!(sel_wide, "sel | turn: 0 | [+3/42 -1/7]");
        let sel_mid = fit_footer_status_right_with_select(0, Some(&git), 18, true);
        assert_eq!(sel_mid, "sel | turn: 0");
    }

    #[test]
    fn chrome_footer_widths_gives_left_priority_when_tight() {
        let min_left = footer_mode_model_width(AgentMode::Plan, "opencode/deepseek-v4-flash");
        assert!(min_left > 20);
        let (left, right) = chrome_footer_widths(50, min_left);
        // Half of (50-2)=24 each; min_left > 24 so left steals.
        assert!(left >= min_left.min(48));
        assert_eq!(left + right, 48);
        let (left_wide, right_wide) = chrome_footer_widths(120, min_left);
        assert_eq!(left_wide, 59); // (120-2)/2
        assert_eq!(right_wide, 59);
    }
}
