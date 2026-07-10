use crate::components::inline_line;
use crate::theme::Theme;
use crate::utils::{format_message_timestamp, truncate_to_width_ellipsis};
use slt::Context;

/// Compact session header (Composer-style top bar).
#[derive(Debug, Clone, Copy)]
pub struct StatusBarInfo<'a> {
    pub branch: Option<&'a str>,
    pub directory: &'a str,
    pub tokens_used: u64,
    pub context_limit: u64,
    pub git_additions: u32,
    pub git_deletions: u32,
    pub turn: u32,
    pub turn_total: Option<u32>,
}

fn format_tokens(n: u64) -> String {
    if n >= 1000 {
        format!("{}K", n / 1000)
    } else {
        n.to_string()
    }
}

/// Renders `branch ~ path   82K / 200K | +1 | 4/4          4:02 AM`.
pub fn render_status_bar(ui: &mut Context, info: StatusBarInfo<'_>, theme: Theme) {
    let width = ui.width().max(40) as usize;
    let path_budget = width.saturating_sub(28);
    let branch = info.branch.unwrap_or("main");
    let directory = truncate_to_width_ellipsis(info.directory, path_budget);
    let left = format!("{branch} ~ {directory}");

    let git = if info.git_additions == 0 && info.git_deletions == 0 {
        String::new()
    } else {
        format!(" | +{} -{}", info.git_additions, info.git_deletions)
    };
    let turn = match info.turn_total {
        Some(total) => format!(" | {}/{}", info.turn, total),
        None => format!(" | {}", info.turn),
    };
    let mid = format!(
        "{} / {}{}{}",
        format_tokens(info.tokens_used),
        format_tokens(info.context_limit),
        git,
        turn
    );
    let time = format_message_timestamp(chrono::Local::now());

    inline_line(ui, |ui| {
        let _ = ui.text(left).fg(theme.dim_text());
        let _ = ui.spacer();
        let _ = ui.text(mid).fg(theme.dim_text());
        let _ = ui.text("  ").fg(theme.dim_text());
        let _ = ui.text(time).fg(theme.dim_text()).dim();
    });
}
