use crate::components::inline_line;
use crate::theme::Theme;
use crate::utils::{truncate_to_width_ellipsis, wrap_text};
use slt::{Border, Context};

const LOGO_LINES: [&str; 2] = ["⣿⣿⡟⣿⡟⣿⣿", "⣿⣿⣿⣿⣿⣿⣿"];

/// Startup tips shown in the full banner (randomized via [`pick_tip`]).
pub const BANNER_TIPS: &[&str] = &[
    "Use --no-session for ephemeral mode — no session file is saved, useful for one-off queries.",
    "Press ? in the prompt for keyboard shortcuts.",
    "Use /help to list slash commands.",
    "Shift+↑/↓ scrolls the transcript; Shift+End jumps to the latest message.",
    "Ctrl+L opens the model selector.",
];

/// Banner display mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BannerMode {
    #[default]
    Full,
    Compact,
    /// One-line status (Owly simple TUI).
    Simple,
}

/// Session metadata rendered in the banner.
#[derive(Debug, Clone, Copy)]
pub struct BannerInfo<'a> {
    pub app_name: &'a str,
    pub version: &'a str,
    pub update_available: bool,
    pub directory: &'a str,
    pub model: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub extensions: u32,
    pub commands: u32,
    pub skills: u32,
    pub tools: u32,
    pub mcp_connected: u32,
    pub mcp_total: u32,
    pub mcp_tools: u32,
    pub tip: &'a str,
}

impl<'a> BannerInfo<'a> {
    pub fn header_line(self) -> String {
        let mut line = format!("Welcome to {} v{}", self.app_name, self.version);
        if self.update_available {
            line.push_str(" (update available)");
        }
        line
    }

    pub fn subtitle(self) -> &'static str {
        "Send /changelog to show version history."
    }
}

/// Tracks whether the banner is full or compact.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BannerState {
    pub compact: bool,
}

impl BannerState {
    pub fn on_user_message(&mut self) {
        self.compact = true;
    }
}

/// Picks a tip deterministically from the session id hash.
pub fn pick_tip(session_seed: &str) -> &'static str {
    let mut hash = 0u64;
    for byte in session_seed.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(u64::from(byte));
    }
    let idx = (hash as usize) % BANNER_TIPS.len();
    BANNER_TIPS[idx]
}

/// Renders the banner for the given mode.
pub fn render_banner(ui: &mut Context, info: BannerInfo<'_>, state: BannerState, theme: Theme) {
    render_banner_with_mode(ui, info, state, theme, BannerMode::Full);
}

/// Renders the banner with an explicit display mode.
pub fn render_banner_with_mode(
    ui: &mut Context,
    info: BannerInfo<'_>,
    state: BannerState,
    theme: Theme,
    mode: BannerMode,
) {
    match mode {
        BannerMode::Simple => render_simple_banner(ui, info, theme),
        BannerMode::Full if !state.compact => render_full_banner(ui, info, theme),
        _ => render_compact_banner(ui, info, theme),
    }
}

/// Plain-text lines for printing the simple banner into terminal scrollback.
pub fn simple_banner_lines(info: BannerInfo<'_>) -> Vec<String> {
    let model = info.model.unwrap_or("—");
    let provider = info.provider.unwrap_or("—");
    vec![
        format!("{} v{}", info.app_name, info.version),
        format!("{model} · {provider} · {}", info.directory),
        String::new(),
    ]
}

/// One-line status for simple shells (Owly).
pub fn render_simple_banner(ui: &mut Context, info: BannerInfo<'_>, theme: Theme) {
    let model = info.model.unwrap_or("—");
    let provider = info.provider.unwrap_or("—");
    inline_line(ui, |ui| {
        let _ = ui
            .text(format!("{} v{}", info.app_name, info.version))
            .fg(theme.dim_text());
        let _ = ui.text(" · ").fg(theme.dim_text());
        let _ = ui.text(model).fg(theme.bright_text());
        let _ = ui.text(" · ").fg(theme.dim_text());
        let _ = ui.text(provider).fg(theme.dim_text());
        let _ = ui.text(" · ").fg(theme.dim_text());
        let _ = ui.text(info.directory).fg(theme.dim_text());
    });
}

fn render_compact_banner(ui: &mut Context, info: BannerInfo<'_>, theme: Theme) {
    let width = ui.width().max(20) as usize;
    let directory = truncate_to_width_ellipsis(info.directory, width.saturating_sub(16));
    inline_line(ui, |ui| {
        let _ = ui.text("─ ").fg(theme.dim_text());
        let _ = ui
            .text(format!("{} v{}", info.app_name, info.version))
            .fg(theme.dim_text());
        let _ = ui.text("  ").fg(theme.dim_text());
        let _ = ui.text(directory).fg(theme.bright_text());
    });
}

fn render_full_banner(ui: &mut Context, info: BannerInfo<'_>, theme: Theme) {
    let width = ui.width().max(24) as usize;
    let meta_w = width.saturating_sub(4);
    let tip_w = width.saturating_sub(8);
    let pad = ui.spacing().xs();
    let pad_h = pad.saturating_add(1);

    let _ = ui
        .bordered(Border::Rounded)
        .border_fg(theme.blue_col())
        .p(pad_h)
        .gap(pad)
        .col(|ui| {
            let _ = ui.row(|ui| {
                let _ = ui.col(|ui| {
                    for line in LOGO_LINES {
                        let _ = ui.text(line).fg(theme.special());
                    }
                });
                let _ = ui.container().pl(pad).grow(1).col(|ui| {
                    let _ = ui.text(info.header_line()).bold();
                    let subtitle = truncate_to_width_ellipsis(info.subtitle(), meta_w);
                    let _ = ui.text(subtitle).fg(theme.dim_text());
                });
            });

            render_meta_line(ui, "Directory:", info.directory, meta_w, theme);
            render_model_line(ui, info, meta_w, theme);
            render_meta_line(
                ui,
                "Stats:",
                &format!(
                    "{:02} exts, {:02} commands, {:02} skills, {:02} tools",
                    info.extensions, info.commands, info.skills, info.tools
                ),
                meta_w,
                theme,
            );
            render_meta_line(
                ui,
                "MCP Server:",
                &format!(
                    "{}/{} connected ({} tools)",
                    info.mcp_connected, info.mcp_total, info.mcp_tools
                ),
                meta_w,
                theme,
            );

            let tip_body = wrap_text(info.tip, tip_w);
            if let Some((first, rest)) = tip_body.split_first() {
                inline_line(ui, |ui| {
                    let _ = ui.text("Tip:").fg(theme.yellow_col()).italic();
                    let _ = ui.text(" ").fg(theme.dim_text());
                    let _ = ui.text(first).fg(theme.dim_text()).italic();
                });
                for line in rest {
                    let _ = ui.text(line).fg(theme.dim_text()).italic();
                }
            }
        });
}

fn render_meta_line(ui: &mut Context, label: &str, value: &str, max_width: usize, theme: Theme) {
    let label_w = 13usize;
    let value_w = max_width.saturating_sub(label_w);
    let clipped = truncate_to_width_ellipsis(value, value_w);
    inline_line(ui, |ui| {
        let _ = ui.text(format!("{label:<13}")).fg(theme.dim_text());
        let _ = ui.text(clipped).fg(theme.bright_text());
    });
}

fn render_model_line(ui: &mut Context, info: BannerInfo<'_>, max_width: usize, theme: Theme) {
    let value = match (info.model, info.provider) {
        (Some(model), Some(provider)) => format!("{model} [{provider}] (000 available)"),
        (Some(model), None) => format!("{model} (000 available)"),
        _ => "No model selected".to_string(),
    };
    render_meta_line(ui, "Model:", &value, max_width, theme);
}
