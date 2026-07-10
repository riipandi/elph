use crate::components::inline_line;
use crate::prompt::AgentMode;
use crate::theme::Theme;
use crate::utils::{path_basename, str_display_width, truncate_to_width_ellipsis};
use slt::Context;

/// Token usage display format (maps to `footerTokenDisplay` setting).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FooterTokenDisplay {
    #[default]
    Both,
    Percentage,
    Count,
}

impl FooterTokenDisplay {
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "percentage" => Self::Percentage,
            "count" => Self::Count,
            _ => Self::Both,
        }
    }
}

/// Footer display mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FooterMode {
    #[default]
    Full,
    /// Single-line footer for simple shells (Owly).
    Simple,
    /// Model caption rendered under the prompt (Composer layout).
    Composer,
}

/// Session metadata for the footer.
#[derive(Debug, Clone, Copy)]
pub struct FooterInfo<'a> {
    pub model_name: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub thinking_level: &'a str,
    pub supports_images: bool,
    pub cost_usd: f64,
    pub tokens_used: u64,
    pub context_pct: f64,
    pub context_limit: u64,
    pub token_display: FooterTokenDisplay,
    pub project_dir: &'a str,
    pub session_id: &'a str,
    pub mode: AgentMode,
    pub turn: u32,
    pub branch: Option<&'a str>,
    pub git_additions: u32,
    pub git_deletions: u32,
}

fn format_tokens(n: u64) -> String {
    format!("{}k", n / 1000)
}

fn format_token_segment(info: FooterInfo<'_>) -> String {
    let used = format_tokens(info.tokens_used);
    let limit = format_tokens(info.context_limit);
    match info.token_display {
        FooterTokenDisplay::Both => format!("{used} | {:.1}% ({limit})", info.context_pct),
        FooterTokenDisplay::Percentage => format!("{:.1}% ({limit})", info.context_pct),
        FooterTokenDisplay::Count => format!("{used} ({limit})"),
    }
}

/// Renders the two-line status footer (no border).
pub fn render_footer(ui: &mut Context, info: FooterInfo<'_>, theme: Theme) {
    render_footer_with_mode(ui, info, theme, FooterMode::Full);
}

/// Renders the footer with an explicit display mode.
pub fn render_footer_with_mode(ui: &mut Context, info: FooterInfo<'_>, theme: Theme, mode: FooterMode) {
    match mode {
        FooterMode::Composer => render_composer_caption(ui, info, theme),
        FooterMode::Simple => render_simple_footer(ui, info, theme),
        FooterMode::Full => {
            let pad = ui.spacing().xs();
            let _ = ui.container().gap(pad).col(|ui| {
                render_footer_line1(ui, info, theme);
                render_footer_line2(ui, info, theme);
            });
        }
    }
}

fn render_composer_caption(ui: &mut Context, info: FooterInfo<'_>, theme: Theme) {
    let model = info.model_name.unwrap_or("No model");
    inline_line(ui, |ui| {
        let _ = ui.text(model).fg(theme.dim_text());
        let _ = ui.text(" · ").fg(theme.dim_text());
        let _ = ui.text(info.mode.footer_label()).fg(theme.mode_border_color(info.mode));
    });
}

fn render_simple_footer(ui: &mut Context, info: FooterInfo<'_>, theme: Theme) {
    let model = info.model_name.unwrap_or("No model");
    let provider = info.provider.unwrap_or("—");
    inline_line(ui, |ui| {
        let _ = ui.text(model).fg(theme.thinking_color(info.thinking_level));
        let _ = ui.text(" · ").fg(theme.dim_text());
        let _ = ui.text(provider).fg(theme.dim_text());
        let _ = ui.text(" · ").fg(theme.dim_text());
        let _ = ui.text(format!("turn {}", info.turn)).fg(theme.dim_text());
    });
}

fn render_footer_line1(ui: &mut Context, info: FooterInfo<'_>, theme: Theme) {
    let width = ui.width().max(20) as usize;
    let model_color = theme.thinking_color(info.thinking_level);
    let cost_seg = format!("${:.2}", info.cost_usd);
    let token_seg = format_token_segment(info);
    let right_seg = format!("{cost_seg} | {token_seg}");

    if let Some(model) = info.model_name {
        let mut left = model.to_string();
        if let Some(provider) = info.provider {
            left.push_str(" | ");
            left.push_str(provider);
        }
        left.push_str(&format!(" | T: {}", info.thinking_level));
        if info.supports_images {
            left.push_str(" | IMG");
        }

        let right_w = str_display_width(&right_seg);
        let max_left = width.saturating_sub(right_w + 1);
        let truncated = str_display_width(&left) > max_left && max_left > 0;
        let clamped = if truncated {
            truncate_to_width_ellipsis(&left, max_left)
        } else if max_left == 0 {
            String::new()
        } else {
            left.clone()
        };

        inline_line(ui, |ui| {
            if !clamped.is_empty() {
                if !truncated {
                    let _ = ui.text(model).fg(model_color);
                    if let Some(rest) = clamped.strip_prefix(model) {
                        let _ = ui.text(rest).fg(theme.dim_text());
                    }
                } else {
                    let _ = ui.text(clamped).fg(theme.dim_text());
                }
            }
            let _ = ui.spacer();
            let _ = ui.text(&cost_seg).fg(theme.context_usage_color(info.context_pct));
            let _ = ui.text(" | ").fg(theme.dim_text());
            let _ = ui.text(token_seg).fg(theme.context_usage_color(info.context_pct));
        });
    } else {
        let fallback = truncate_to_width_ellipsis("No model selected", width);
        let _ = ui.text(fallback).fg(theme.dim_text());
    }
}

fn render_footer_line2(ui: &mut Context, info: FooterInfo<'_>, theme: Theme) {
    let width = ui.width().max(20) as usize;
    let git_color = theme.git_status_color(info.git_additions, info.git_deletions);
    let git = if info.git_additions == 0 && info.git_deletions == 0 {
        "[-]".to_string()
    } else {
        format!("[+{} -{}]", info.git_additions, info.git_deletions)
    };
    let branch = info.branch.unwrap_or("main");
    let project = path_basename(info.project_dir);
    let right = format!("turn: {} | {branch} {git}", info.turn);
    let left = format!("{project} [{}]", info.session_id);

    let right_w = str_display_width(&right);
    let max_left = width.saturating_sub(right_w + 1);
    let left = if str_display_width(&left) > max_left && max_left > 0 {
        truncate_to_width_ellipsis(&left, max_left)
    } else {
        left
    };

    inline_line(ui, |ui| {
        let _ = ui.text(&left).bold();
        let _ = ui.spacer();
        let _ = ui.text(format!("turn: {} | ", info.turn));
        let _ = ui.text(branch);
        let _ = ui.text(" ");
        let _ = ui.text(git).fg(git_color);
    });
}
