use super::spacing::{shell_input_gap, shell_section_gap};
use crate::chrome::{
    ActivityState, BannerInfo, BannerMode, BannerState, FooterInfo, FooterMode, StatusBarInfo, render_activity,
    render_banner_with_mode, render_footer_with_mode, render_status_bar,
};
use crate::diff::SlashCommand;
use crate::prompt::{SlashPaletteState, render_slash_palette, slash_palette_visible};
use crate::theme::Theme;
use slt::{Context, widgets::SpinnerState};

/// Shell display tier — full agent chrome (Elph) vs compact shell (Owly).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellTier {
    Full,
    /// Cursor Composer-style agent UI.
    Composer,
    Simple,
}

/// Layout region handed to the host app renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellRegion {
    /// Scrollable transcript (should use available vertical space).
    Chat,
    /// Activity, slash palette, and prompt input stack.
    Input,
}

/// Chrome metadata shared by both shells.
#[derive(Debug, Clone)]
pub struct ShellChrome<'a> {
    pub tier: ShellTier,
    pub banner: BannerInfo<'a>,
    pub banner_state: BannerState,
    pub footer: FooterInfo<'a>,
    pub slash_input: Option<&'a str>,
    pub slash_commands: Option<&'a [SlashCommand]>,
    pub slash_palette: Option<&'a SlashPaletteState>,
    pub running: bool,
    pub activity: Option<ActivityState>,
    pub spinner: SpinnerState,
    pub status_bar: Option<StatusBarInfo<'a>>,
}

impl<'a> ShellChrome<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn full(
        banner: BannerInfo<'a>,
        banner_state: BannerState,
        footer: FooterInfo<'a>,
        slash_input: &'a str,
        slash_commands: &'a [SlashCommand],
        slash_palette: &'a SlashPaletteState,
        running: bool,
        activity: Option<ActivityState>,
        spinner: SpinnerState,
    ) -> Self {
        Self {
            tier: ShellTier::Full,
            banner,
            banner_state,
            footer,
            slash_input: Some(slash_input),
            slash_commands: Some(slash_commands),
            slash_palette: Some(slash_palette),
            running,
            activity,
            spinner,
            status_bar: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn composer(
        status_bar: StatusBarInfo<'a>,
        footer: FooterInfo<'a>,
        slash_input: &'a str,
        slash_commands: &'a [SlashCommand],
        slash_palette: &'a SlashPaletteState,
        running: bool,
        activity: Option<ActivityState>,
        spinner: SpinnerState,
    ) -> Self {
        Self {
            tier: ShellTier::Composer,
            banner: BannerInfo {
                app_name: "Elph",
                version: "",
                update_available: false,
                directory: status_bar.directory,
                model: footer.model_name,
                provider: footer.provider,
                extensions: 0,
                commands: 0,
                skills: 0,
                tools: 0,
                mcp_connected: 0,
                mcp_total: 0,
                mcp_tools: 0,
                tip: "",
            },
            banner_state: BannerState::default(),
            footer,
            slash_input: Some(slash_input),
            slash_commands: Some(slash_commands),
            slash_palette: Some(slash_palette),
            running,
            activity,
            spinner,
            status_bar: Some(status_bar),
        }
    }

    pub fn simple(
        banner: BannerInfo<'a>,
        footer: FooterInfo<'a>,
        running: bool,
        activity: Option<ActivityState>,
        spinner: SpinnerState,
    ) -> Self {
        Self {
            tier: ShellTier::Simple,
            banner,
            banner_state: BannerState::default(),
            footer,
            slash_input: None,
            slash_commands: None,
            slash_palette: None,
            running,
            activity,
            spinner,
            status_bar: None,
        }
    }
}

/// Theme spacing scale — prefer over hard-coded padding (SLT cookbook pattern).
pub fn layout_pad(ui: &Context) -> u32 {
    ui.spacing().xs()
}

fn banner_mode(tier: ShellTier) -> BannerMode {
    match tier {
        ShellTier::Full => BannerMode::Full,
        ShellTier::Composer | ShellTier::Simple => BannerMode::Simple,
    }
}

fn footer_mode(tier: ShellTier) -> FooterMode {
    match tier {
        ShellTier::Full | ShellTier::Composer => FooterMode::Full,
        ShellTier::Simple => FooterMode::Simple,
    }
}

/// Renders banner → chat → input chrome → footer with proportional gaps.
pub fn render_agent_shell(
    ui: &mut Context,
    theme: Theme,
    chrome: ShellChrome<'_>,
    mut render_region: impl FnMut(&mut Context, ShellRegion),
) {
    let pad = shell_section_gap(ui);
    let input_gap = shell_input_gap(ui, chrome.tier == ShellTier::Composer);
    let show_activity = chrome.activity.as_ref().is_some_and(|a| a.visible);
    let show_slash = matches!(chrome.tier, ShellTier::Full | ShellTier::Composer)
        && chrome.slash_input.is_some_and(slash_palette_visible);

    let _ = ui.container().grow(1).gap(pad).col(|ui| {
        if chrome.tier == ShellTier::Composer {
            if let Some(status) = chrome.status_bar {
                render_status_bar(ui, status, theme);
            }
        } else {
            render_banner_with_mode(ui, chrome.banner, chrome.banner_state, theme, banner_mode(chrome.tier));
        }

        let _ = ui.container().grow(1).col(|ui| {
            render_region(ui, ShellRegion::Chat);
        });

        let _ = ui.container().gap(input_gap).col(|ui| {
            if show_activity && let Some(activity) = chrome.activity.as_ref() {
                render_activity(ui, activity, theme, &chrome.spinner);
            }
            if show_slash
                && let (Some(input), Some(commands), Some(palette)) =
                    (chrome.slash_input, chrome.slash_commands, chrome.slash_palette)
            {
                render_slash_palette(ui, input, commands, palette, theme);
            }
            render_region(ui, ShellRegion::Input);
        });

        render_footer_with_mode(ui, chrome.footer, theme, footer_mode(chrome.tier));
    });
}
