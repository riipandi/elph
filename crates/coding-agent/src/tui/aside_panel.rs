//! Inline `/aside` side-question panel above the prompt (Grok `/btw` layout).
//!
//! Loading shows a spinner; Done/Error show the answer until **Esc** dismisses.
//! On dismiss of Done, the shell may push a sticky transcript meta card.
//! The panel is a pure UI surface: it never touches the harness turn, the
//! session tree, or the prompt queues.

use elph_tui::components::progress_indicator::SpinnerLoaderView;
use elph_tui::components::scroll_bar::scroll_indicator_label;
use elph_tui::components::theme::UiTheme;
use elph_tui::word_wrap::wrap_text_to_lines;
use iocraft::prelude::*;

use crate::tui::inline_dialog::{InlineDialogShell, inline_body_width};

/// Max body lines for the Done answer (Grok `DONE_MAX_BODY_LINES`).
pub const ASIDE_MAX_BODY_LINES: usize = 12;

/// State of the `/aside` inline panel.
#[derive(Debug, Clone)]
pub enum AsidePanelState {
    Loading {
        request_id: u64,
        question: String,
    },
    Done {
        request_id: u64,
        question: String,
        answer: String,
        scroll_offset: usize,
    },
    Error {
        request_id: u64,
        question: String,
        error: String,
    },
}

impl AsidePanelState {
    pub fn request_id(&self) -> u64 {
        match self {
            Self::Loading { request_id, .. } | Self::Done { request_id, .. } | Self::Error { request_id, .. } => {
                *request_id
            }
        }
    }

    pub fn question(&self) -> &str {
        match self {
            Self::Loading { question, .. } | Self::Done { question, .. } | Self::Error { question, .. } => {
                question.as_str()
            }
        }
    }

    pub fn loading(request_id: u64, question: impl Into<String>) -> Self {
        Self::Loading {
            request_id,
            question: question.into(),
        }
    }

    pub fn done(request_id: u64, question: impl Into<String>, answer: impl Into<String>) -> Self {
        Self::Done {
            request_id,
            question: question.into(),
            answer: answer.into(),
            scroll_offset: 0,
        }
    }

    pub fn error(request_id: u64, question: impl Into<String>, error: impl Into<String>) -> Self {
        Self::Error {
            request_id,
            question: question.into(),
            error: error.into(),
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        if let Self::Done { scroll_offset, .. } = self {
            *scroll_offset = scroll_offset.saturating_sub(n);
        }
    }

    pub fn scroll_down(&mut self, n: usize, max_offset: usize) {
        if let Self::Done { scroll_offset, .. } = self {
            *scroll_offset = (*scroll_offset + n).min(max_offset);
        }
    }

    pub fn max_scroll_offset(&self, content_width: usize) -> usize {
        match self {
            Self::Done { answer, .. } => {
                let total = wrap_text_to_lines(answer, content_width.max(1)).len().max(1);
                total.saturating_sub(ASIDE_MAX_BODY_LINES)
            }
            _ => 0,
        }
    }

    /// Title line: `/aside <question>` truncated for width.
    pub fn title_for_width(&self, inner: u16) -> String {
        let prefix = "/aside ";
        let q = self.question();
        let full = format!("{prefix}{q}");
        let max = inner.saturating_sub(2).max(8) as usize;
        if full.chars().count() <= max {
            return full;
        }
        let keep = max.saturating_sub(1);
        let mut s: String = full.chars().take(keep).collect();
        s.push('…');
        s
    }
}

/// Dismiss helper: returns (request_id, optional notice for transcript).
pub fn dismiss_aside_panel(state: AsidePanelState) -> (u64, Option<String>) {
    let id = state.request_id();
    let notice = match state {
        AsidePanelState::Done { question, answer, .. } => {
            let q: String = question.chars().take(48).collect();
            let a: String = answer.chars().take(80).collect();
            Some(format!("/aside {q} — {a}"))
        }
        AsidePanelState::Error { question, error, .. } => {
            let q: String = question.chars().take(48).collect();
            Some(format!("/aside {q} — error: {error}"))
        }
        AsidePanelState::Loading { question, .. } => {
            let q: String = question.chars().take(48).collect();
            Some(format!("/aside {q} — cancelled"))
        }
    };
    (id, notice)
}

/// Props for [`AsidePanel`].
#[derive(Props)]
pub struct AsidePanelProps {
    pub screen_width: u16,
    pub state: AsidePanelState,
    /// When true, border uses focus accent (Done + scrollable).
    pub has_focus: bool,
    /// Bumped so spinner re-renders while loading.
    pub tick: u64,
}

impl Default for AsidePanelProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            state: AsidePanelState::loading(0, String::new()),
            has_focus: false,
            tick: 0,
        }
    }
}

/// Inline panel above status row / prompt for `/aside`.
#[component]
pub fn AsidePanel(props: &AsidePanelProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let theme = UiTheme::default();
    let inner = inline_body_width(props.screen_width);
    let title = props.state.title_for_width(inner);
    let content_w = inner.max(1) as usize;

    let body: AnyElement<'static> = match &props.state {
        AsidePanelState::Loading { .. } => element! {
            View(
                width: inner,
                flex_direction: FlexDirection::Row,
                flex_shrink: 0f32,
                height: 1,
            ) {
                SpinnerLoaderView(color: Some(theme.warning), active: true, theme: Some(theme))
                Text(
                    content: " Answering…".to_string(),
                    color: theme.warning,
                    wrap: TextWrap::NoWrap,
                )
            }
        }
        .into(),
        AsidePanelState::Error { error, .. } => {
            let lines = wrap_text_to_lines(error, content_w);
            let max = ASIDE_MAX_BODY_LINES.min(lines.len().max(1));
            let mut rows: Vec<AnyElement<'static>> = Vec::new();
            for line in lines.into_iter().take(max) {
                rows.push(
                    element! {
                        Text(
                            content: line,
                            color: theme.error,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                    .into(),
                );
            }
            element! {
                View(width: inner, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                    #(rows)
                }
            }
            .into()
        }
        AsidePanelState::Done {
            answer, scroll_offset, ..
        } => {
            let max_body = ASIDE_MAX_BODY_LINES;
            let lines = wrap_text_to_lines(answer, inner as usize);
            let total = lines.len().max(1);
            let max_off = total.saturating_sub(max_body);
            let offset = (*scroll_offset).min(max_off);
            let end = (offset + max_body).min(total);
            let mut rows: Vec<AnyElement<'static>> = Vec::new();
            for line in lines.into_iter().skip(offset).take(end.saturating_sub(offset)) {
                rows.push(
                    element! {
                        Text(content: line, color: theme.text_primary, wrap: TextWrap::NoWrap)
                    }
                    .into(),
                );
            }
            if rows.is_empty() {
                rows.push(
                    element! {
                        Text(content: String::new(), color: theme.text_muted, wrap: TextWrap::NoWrap)
                    }
                    .into(),
                );
            }
            // Bottom line: [Esc] on the left, ↑/↓ position indicator on the right
            // (dimmed at the edges). Both share one line so the arrows sit at the
            // bottom-right, aligned with [Esc], and never clip the text column.
            let mut right: Vec<AnyElement<'static>> = Vec::new();
            if total > max_body {
                let can_up = offset > 0;
                let can_down = offset < max_off;
                let up_color = if can_up { theme.text_secondary } else { theme.text_muted };
                let down_color = if can_down { theme.text_secondary } else { theme.text_muted };
                let pos = scroll_indicator_label(offset as u32, max_body as u32, total as u32);
                right.push(
                    element! { Text(content: "↑".to_string(), color: up_color, wrap: TextWrap::NoWrap) }.into(),
                );
                right.push(
                    element! { Text(content: " ".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap) }.into(),
                );
                right.push(
                    element! { Text(content: "↓".to_string(), color: down_color, wrap: TextWrap::NoWrap) }.into(),
                );
                right.push(
                    element! { Text(content: format!("  {pos}"), color: theme.text_muted, wrap: TextWrap::NoWrap) }.into(),
                );
            }
            // One blank row separates the answer from the footer line so they don't touch.
            rows.push(
                element! { Text(content: String::new(), color: theme.text_muted, wrap: TextWrap::NoWrap) }.into(),
            );
            rows.push(
                element! {
                    View(width: inner, flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, flex_shrink: 0f32) {
                        Text(content: "[Esc] dismiss".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
                        View(flex_direction: FlexDirection::Row, flex_shrink: 0f32) {
                            #(right)
                        }
                    }
                }
                .into(),
            );
            element! {
                View(width: inner, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                    #(rows)
                }
            }
            .into()
        }
    };
    let footer = match &props.state {
        AsidePanelState::Loading { .. } | AsidePanelState::Error { .. } => Some("[Esc] dismiss".to_string()),
        AsidePanelState::Done { .. } => {
            // The scrollable case already shows [Esc] + the ↑/↓ position indicator on
            // one combined body line, so the footer is left empty to avoid duplication.
            None
        }
    };

    // Force re-render path for spinner: use tick in a dim marker when loading.
    let _ = props.tick;

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: title,
            has_focus: props.has_focus,
            footer_hint: footer,
        ) {
            #(body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_scroll_for_long_answer() {
        let answer = (0..30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let state = AsidePanelState::done(1, "q", answer);
        assert!(state.max_scroll_offset(40) > 0);
    }

    #[test]
    fn title_truncates_long_question() {
        let state = AsidePanelState::loading(1, "x".repeat(200));
        let t = state.title_for_width(20);
        assert!(t.starts_with("/aside "));
        assert!(t.ends_with('…') || t.chars().count() <= 20);
    }

    #[test]
    fn dismiss_done_yields_notice() {
        let (_, notice) = dismiss_aside_panel(AsidePanelState::done(1, "why?", "because"));
        assert!(notice.unwrap().contains("why?"));
    }
}
