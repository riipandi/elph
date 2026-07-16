//! Progressive chrome fitting — drops or truncates rightmost segments first.

use crate::tui::labels::{
    GitFooterInfo, context_usage_label, footer_left_label, footer_right_label, format_worktree_stats,
    session_header_segments, session_label,
};
use crate::types::ThinkingLevel;
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

/// Header right: full context → simpler modes → cost only → percent only.
pub fn fit_header_stats(
    cost_usd: f64,
    tokens_used: u64,
    context_pct: f64,
    context_limit: u64,
    display: &str,
    max_width: usize,
) -> String {
    let cost = format!("${cost_usd:.2}");
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
    let percentage = format!(
        "{cost} | {}",
        context_usage_label(tokens_used, context_pct, context_limit, "percentage")
    );
    let count = format!(
        "{cost} | {}",
        context_usage_label(tokens_used, context_pct, context_limit, "count")
    );
    let pct_only = format!("{context_pct:.1}%");
    pick_fitting_label(&[full, percentage, count, cost, pct_only], max_width)
}

/// Footer left: turn → git stats → branch → truncate project name.
pub fn fit_footer_left(project_name: &str, git: Option<&GitFooterInfo>, turn: u32, max_width: usize) -> String {
    let project = format!("~ {project_name}");
    let mut candidates = Vec::new();
    let mut line = project.clone();
    if let Some(git) = git {
        let branch = git.branch.trim();
        if !branch.is_empty() {
            line.push_str(&format!(" [{branch}]"));
        }
        let with_branch = line.clone();
        let stats = format_worktree_stats(git.files_added, git.lines_added, git.files_deleted, git.lines_deleted);
        let with_stats = format!("{with_branch} {stats}");
        candidates.push(footer_left_label(&with_stats, turn));
        candidates.push(with_stats);
        candidates.push(with_branch);
    } else {
        candidates.push(footer_left_label(&line, turn));
    }
    candidates.push(line);
    candidates.push(project);
    pick_fitting_label(&candidates, max_width)
}

/// Footer right: thinking level → IMG → truncate model (model id kept as long as possible).
pub fn fit_footer_right(
    model_label: &str,
    thinking_level: ThinkingLevel,
    supports_images: bool,
    max_width: usize,
) -> String {
    let thinking = thinking_level.label();
    let mut candidates = vec![footer_right_label(model_label, thinking_level, supports_images)];
    if supports_images {
        candidates.push(format!("IMG | {model_label}"));
    }
    candidates.push(format!("{model_label} | {thinking}"));
    candidates.push(model_label.to_string());
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
    fn fit_header_stats_drops_context_before_cost() {
        let wide = fit_header_stats(0.12, 131_000, 48.2, 272_000, "both", 80);
        assert!(wide.contains("131k"));
        let narrow = fit_header_stats(0.12, 131_000, 48.2, 272_000, "both", 8);
        assert!(display_width(&narrow) <= 8);
    }

    #[test]
    fn fit_footer_left_drops_turn_first() {
        let git = GitFooterInfo {
            branch: "main".to_string(),
            files_added: 3,
            lines_added: 42,
            files_deleted: 1,
            lines_deleted: 7,
        };
        let wide = fit_footer_left("elph", Some(&git), 2, 80);
        assert!(wide.contains("turn: 2"));
        let narrow = fit_footer_left("elph", Some(&git), 2, 28);
        assert!(!narrow.contains("turn:"));
    }
}
