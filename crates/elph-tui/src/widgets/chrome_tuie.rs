//! tuie footer and activity chrome widgets.

use crate::shell::ShellChromeData;
use crate::theme::Theme;
use crate::utils::{path_basename, str_display_width, truncate_to_width_ellipsis};
use tuie::prelude::*;

fn format_tokens(n: u64) -> String {
    format!("{}k", n / 1000)
}

fn format_token_segment(chrome: &ShellChromeData) -> String {
    let used = format_tokens(chrome.tokens_used);
    let limit = format_tokens(chrome.context_limit);
    format!("{used} | {:.1}% ({limit})", chrome.context_pct)
}

fn footer_lines(chrome: &ShellChromeData) -> (String, String, String) {
    let cost_seg = format!("${:.2}", chrome.cost_usd);
    let token_seg = format_token_segment(chrome);

    let mut line1 = String::new();
    if chrome.model_name.is_empty() {
        line1 = "No model selected".to_string();
    } else {
        line1.push_str(&chrome.model_name);
        if !chrome.provider.is_empty() {
            line1.push_str(" | ");
            line1.push_str(&chrome.provider);
        }
        line1.push_str(&format!(" | T: {}", chrome.thinking_level));
        if chrome.supports_images {
            line1.push_str(" | IMG");
        }
        let right_seg = format!("{cost_seg} | {token_seg}");
        let right_w = str_display_width(&right_seg);
        let max_left = 120usize.saturating_sub(right_w + 1);
        if str_display_width(&line1) > max_left && max_left > 0 {
            line1 = truncate_to_width_ellipsis(&line1, max_left);
        }
    }

    let git = if chrome.git_additions == 0 && chrome.git_deletions == 0 {
        "[-]".to_string()
    } else {
        format!("[+{} -{}]", chrome.git_additions, chrome.git_deletions)
    };
    let branch = if chrome.branch.is_empty() {
        "main".to_string()
    } else {
        chrome.branch.clone()
    };
    let project = path_basename(&chrome.project_dir);
    let line2 = format!("{project} [{}]    turn: {} | {branch} {git}", chrome.session_id, chrome.turn);
    let line3 = format!("{cost_seg} | {token_seg}");
    (line1, line2, line3)
}

fn activity_line(chrome: &ShellChromeData) -> String {
    let mut line = chrome.activity_label.clone();
    if chrome.activity_cancel_requested {
        line.push_str(" (cancelling)");
    } else {
        line.push_str("  Enter queue · Ctrl+Enter follow-up · Ctrl+C cancel");
    }
    line
}

/// Stable handles for shell-side activity sync.
#[derive(Clone)]
#[cfg_attr(test, allow(dead_code))]
pub(crate) struct ActivityHandles {
    pub text_id: WidgetId<Text>,
}

/// Live-updating activity strip between transcript and prompt.
pub struct ActivityPane {
    root: Box<Pane>,
    text_id: WidgetId<Text>,
    cached: String,
}

impl ActivityPane {
    pub fn new(theme: Theme) -> Box<Self> {
        let mut text_id = WidgetId::EMPTY;
        let root = Pane::new().child(
            Text::new()
                .content("")
                .style(Style::new().fg(theme.muted))
                .id(&mut text_id) as Box<dyn Widget>,
        );
        Box::new(Self {
            root,
            text_id,
            cached: String::new(),
        })
    }

    pub fn sync(&mut self, chrome: &ShellChromeData, theme: Theme) {
        let visible = chrome.activity_visible;
        let next = if visible { activity_line(chrome) } else { String::new() };
        if next == self.cached {
            return;
        }
        self.cached = next;
        if let Some(text) = self.root.get_widget_mut(self.text_id) {
            text.set_content(self.cached.clone());
            text.set_style(Style::new().fg(theme.muted));
        }
        self.root.dirty_layout();
    }

    pub(crate) fn handles(&self) -> ActivityHandles {
        ActivityHandles { text_id: self.text_id }
    }

    #[cfg(not(test))]
    pub(crate) fn sync_in(root: &mut dyn Widget, handles: &ActivityHandles, chrome: &ShellChromeData, theme: Theme) {
        let visible = chrome.activity_visible;
        let next = if visible { activity_line(chrome) } else { String::new() };
        if let Some(text) = root.get_widget_mut(handles.text_id) {
            text.set_content(next);
            text.set_style(Style::new().fg(theme.muted));
        }
    }
}

impl DelegateWidget for ActivityPane {
    tuie::delegate_widget!(root);
}

/// Stable handles for shell-side footer sync.
#[derive(Clone)]
#[cfg_attr(test, allow(dead_code))]
pub(crate) struct FooterHandles {
    pub line1_id: WidgetId<Text>,
    pub line2_id: WidgetId<Text>,
    pub line3_id: WidgetId<Text>,
}

/// Live-updating status footer (model, session, tokens).
pub struct FooterPane {
    root: Box<Pane>,
    line1_id: WidgetId<Text>,
    line2_id: WidgetId<Text>,
    line3_id: WidgetId<Text>,
    cached: (String, String, String),
}

impl FooterPane {
    pub fn new(theme: Theme) -> Box<Self> {
        let mut line1_id = WidgetId::EMPTY;
        let mut line2_id = WidgetId::EMPTY;
        let mut line3_id = WidgetId::EMPTY;
        let root = Pane::new().vertical().gap(0).children([
            Text::new()
                .content("")
                .style(Style::new().fg(theme.foreground))
                .id(&mut line1_id) as Box<dyn Widget>,
            Text::new()
                .content("")
                .style(Style::new().fg(theme.foreground).bold())
                .id(&mut line2_id) as Box<dyn Widget>,
            Text::new()
                .content("")
                .style(Style::new().fg(theme.foreground))
                .id(&mut line3_id) as Box<dyn Widget>,
        ]);
        Box::new(Self {
            root,
            line1_id,
            line2_id,
            line3_id,
            cached: (String::new(), String::new(), String::new()),
        })
    }

    pub fn sync(&mut self, chrome: &ShellChromeData, theme: Theme) {
        let next = footer_lines(chrome);
        if next == self.cached {
            return;
        }
        self.cached = next.clone();
        let model_color = theme.thinking_color(&chrome.thinking_level);
        if let Some(text) = self.root.get_widget_mut(self.line1_id) {
            text.set_content(next.0.clone());
            text.set_style(Style::new().fg(model_color));
        }
        if let Some(text) = self.root.get_widget_mut(self.line2_id) {
            text.set_content(next.1.clone());
            text.set_style(Style::new().fg(theme.foreground).bold());
        }
        if let Some(text) = self.root.get_widget_mut(self.line3_id) {
            text.set_content(next.2.clone());
            text.set_style(Style::new().fg(theme.context_usage_color(chrome.context_pct)));
        }
        self.root.dirty_layout();
    }

    pub(crate) fn handles(&self) -> FooterHandles {
        FooterHandles {
            line1_id: self.line1_id,
            line2_id: self.line2_id,
            line3_id: self.line3_id,
        }
    }

    #[cfg(not(test))]
    pub(crate) fn sync_in(root: &mut dyn Widget, handles: &FooterHandles, chrome: &ShellChromeData, theme: Theme) {
        let next = footer_lines(chrome);
        let model_color = theme.thinking_color(&chrome.thinking_level);
        if let Some(text) = root.get_widget_mut(handles.line1_id) {
            text.set_content(next.0.clone());
            text.set_style(Style::new().fg(model_color));
        }
        if let Some(text) = root.get_widget_mut(handles.line2_id) {
            text.set_content(next.1.clone());
            text.set_style(Style::new().fg(theme.foreground).bold());
        }
        if let Some(text) = root.get_widget_mut(handles.line3_id) {
            text.set_content(next.2.clone());
            text.set_style(Style::new().fg(theme.context_usage_color(chrome.context_pct)));
        }
    }
}

impl DelegateWidget for FooterPane {
    tuie::delegate_widget!(root);
}

/// Builds the two-line status footer for the tuie shell.
pub fn build_footer_widget(chrome: &ShellChromeData, theme: Theme) -> Box<Pane> {
    let mut pane = FooterPane::new(theme);
    pane.sync(chrome, theme);
    pane.root
}

/// Builds the activity line shown between transcript and prompt while busy.
pub fn build_activity_widget(chrome: &ShellChromeData, theme: Theme) -> Box<dyn Widget> {
    let mut pane = ActivityPane::new(theme);
    pane.sync(chrome, theme);
    pane.root as Box<dyn Widget>
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentMode;
    use crate::shell::ShellChromeData;
    use crate::theme::Theme;
    use tuie::emulator::Emulator;

    fn sample_chrome() -> ShellChromeData {
        ShellChromeData {
            running: true,
            sidebar_open: false,
            palette_open: false,
            activity_visible: true,
            activity_label: "Thinking".into(),
            activity_cancel_requested: false,
            model_name: "gpt-4".into(),
            provider: "openai".into(),
            thinking_level: "medium".into(),
            supports_images: true,
            cost_usd: 1.25,
            tokens_used: 42_000,
            context_pct: 12.5,
            context_limit: 128_000,
            project_dir: "/home/user/elph".into(),
            session_id: "abc123".into(),
            mode: AgentMode::Ask,
            turn: 7,
            branch: "feat/tui".into(),
            git_additions: 5,
            git_deletions: 2,
        }
    }

    #[test]
    fn footer_includes_git_stats_and_branch() {
        let theme = Theme::dark();
        let chrome = sample_chrome();
        let mut footer = build_footer_widget(&chrome, theme);
        let term = Emulator::new(&mut *footer, Vec2::new(80, 6));
        let snap = term.get_snapshot_text();
        assert!(snap.contains("gpt-4"));
        assert!(snap.contains("openai"));
        assert!(snap.contains("IMG"));
        assert!(snap.contains("feat/tui"));
        assert!(snap.contains("+5 -2"));
        assert!(snap.contains("turn: 7"));
    }

    #[test]
    fn footer_no_model_shows_placeholder() {
        let theme = Theme::dark();
        let mut chrome = sample_chrome();
        chrome.model_name.clear();
        chrome.provider.clear();
        let mut footer = build_footer_widget(&chrome, theme);
        let term = Emulator::new(&mut *footer, Vec2::new(60, 4));
        assert!(term.get_snapshot_text().contains("No model selected"));
    }

    #[test]
    fn activity_shows_cancelling_suffix() {
        let theme = Theme::dark();
        let mut chrome = sample_chrome();
        chrome.activity_cancel_requested = true;
        let mut activity = build_activity_widget(&chrome, theme);
        let term = Emulator::new(&mut *activity, Vec2::new(50, 2));
        assert!(term.get_snapshot_text().contains("cancelling"));
    }

    #[test]
    fn activity_sync_skips_unchanged_label() {
        let theme = Theme::dark();
        let chrome = sample_chrome();
        let mut pane = ActivityPane::new(theme);
        pane.sync(&chrome, theme);
        let cached = pane.cached.clone();
        pane.sync(&chrome, theme);
        assert_eq!(pane.cached, cached);
    }

    #[test]
    fn footer_sync_updates_on_turn_change() {
        let theme = Theme::dark();
        let mut chrome = sample_chrome();
        let mut pane = FooterPane::new(theme);
        pane.sync(&chrome, theme);
        chrome.turn = 8;
        pane.sync(&chrome, theme);
        assert!(pane.cached.1.contains("turn: 8"));
    }
}
