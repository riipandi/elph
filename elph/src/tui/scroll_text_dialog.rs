//! Reusable modal for scrolling long plain text (system prompt, help dumps, logs, …).
//!
//! Lives in the Elph TUI shell layer (not `elph-tui`) so app-specific focus and
//! prompt lifecycle stay colocated with other overlays.
//!
//! Scroll position and the scrollbar thumb share one source of truth: the inner
//! [`ScrollView`] (handle + built-in track). An external thumb was previously
//! driven by a separate height estimate and desynced to `offset = 0`.

use elph_tui::components::theme::UiTheme;
use elph_tui::components::{DialogChrome, DialogHeader, DialogShellOverlay};
use elph_tui::components::{dialog_body_min_height, dialog_max_content_height};
use iocraft::prelude::*;

use crate::tui::focus::ShellFocus;

/// Default dialog width as a percentage of the terminal width (`80%`).
pub const DEFAULT_SCROLL_TEXT_WIDTH_PCT: u8 = 80;
/// Clamp range for width percent (inclusive).
pub const MIN_SCROLL_TEXT_WIDTH_PCT: u8 = 20;
pub const MAX_SCROLL_TEXT_WIDTH_PCT: u8 = 100;
/// Floor width on wide terminals so the dialog stays readable.
const MIN_DIALOG_WIDTH: u16 = 40;
const SCREEN_WIDTH_MARGIN: u16 = 2;
const SCREEN_HEIGHT_MARGIN: u16 = 4;
const DEFAULT_SCROLL_STEP: u16 = 3;

/// Open scroll-text viewer session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingScrollTextDialog {
    pub title: String,
    pub text: String,
    /// Outer width as % of terminal width (default [`DEFAULT_SCROLL_TEXT_WIDTH_PCT`]).
    pub width_pct: u8,
    /// Optional explicit body height in rows. When `None`, height is auto-computed
    /// from screen size (maximized). When `Some`, the dialog fits exactly that many rows.
    pub body_height: Option<u16>,
}

impl PendingScrollTextDialog {
    pub fn open(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self::open_with_width(title, text, DEFAULT_SCROLL_TEXT_WIDTH_PCT)
    }

    pub fn open_with_width(title: impl Into<String>, text: impl Into<String>, width_pct: u8) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
            width_pct: clamp_scroll_text_width_pct(width_pct),
            body_height: None,
        }
    }
}

/// Arguments for [`open_scroll_text_dialog`].
pub struct OpenScrollTextDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingScrollTextDialog>>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub title: String,
    pub text: String,
    /// Width percent; use [`DEFAULT_SCROLL_TEXT_WIDTH_PCT`] when omitted by callers.
    pub width_pct: u8,
    /// Optional explicit body height in rows. When `None`, height auto-computes from screen.
    pub body_height: Option<u16>,
}

pub fn open_scroll_text_dialog(args: OpenScrollTextDialogArgs<'_>) {
    let mut pending = if args.width_pct == DEFAULT_SCROLL_TEXT_WIDTH_PCT {
        PendingScrollTextDialog::open(args.title, args.text)
    } else {
        PendingScrollTextDialog::open_with_width(args.title, args.text, args.width_pct)
    };
    pending.body_height = args.body_height;
    args.pending.set(Some(pending));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

/// How [`close_scroll_text_dialog`] restores the prompt after dismiss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollTextClosePrompt {
    /// Leave the draft / live prompt as-is.
    #[default]
    Keep,
    /// Clear draft, live draft, and force the editor to wipe residual text.
    Clear,
}

/// Arguments for [`close_scroll_text_dialog`].
pub struct CloseScrollTextDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingScrollTextDialog>>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub force_editor_clear: Option<&'a mut Ref<bool>>,
    pub prompt: ScrollTextClosePrompt,
}

pub fn close_scroll_text_dialog(args: CloseScrollTextDialogArgs<'_>) {
    args.pending.write().take();
    if matches!(args.prompt, ScrollTextClosePrompt::Clear) {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
        if let Some(force) = args.force_editor_clear {
            force.set(true);
        }
    }
    args.shell_focus.set(ShellFocus::Prompt);
}

/// Clamp a width percent into the supported range.
pub fn clamp_scroll_text_width_pct(width_pct: u8) -> u8 {
    width_pct.clamp(MIN_SCROLL_TEXT_WIDTH_PCT, MAX_SCROLL_TEXT_WIDTH_PCT)
}

/// Outer dialog width from terminal size and a width percent (`20`–`100`).
pub fn scroll_text_dialog_width(screen_width: u16, width_pct: u8) -> u16 {
    let pct = clamp_scroll_text_width_pct(width_pct) as u32;
    let usable = screen_width.saturating_sub(SCREEN_WIDTH_MARGIN).max(1) as u32;
    let mut width = (usable * pct / 100).max(1) as u16;
    // Prefer a readable floor when the terminal is wide enough.
    if usable as u16 >= MIN_DIALOG_WIDTH {
        width = width.max(MIN_DIALOG_WIDTH).min(usable as u16);
    } else {
        width = width.min(usable as u16);
    }
    width
}

/// Slim-header chrome and body viewport height for the scroll-text viewer.
pub fn scroll_text_dialog_chrome(screen_width: u16, screen_height: u16, width_pct: u8) -> (DialogChrome, u16) {
    let outer = scroll_text_dialog_width(screen_width, width_pct);
    let chrome = DialogChrome {
        width: outer,
        slim_header: true,
        padding_horizontal: 1,
        ..DialogChrome::default()
    };
    let max_body = dialog_max_content_height(screen_height, &chrome, SCREEN_HEIGHT_MARGIN);
    let body_height = dialog_body_min_height(max_body);
    (
        DialogChrome {
            min_content_height: body_height,
            ..chrome
        },
        body_height,
    )
}

/// Whether a scrollbar is needed for the estimated content size.
pub fn scroll_text_scrollbar_visible(content_height: u16, viewport_height: u16) -> bool {
    content_height > viewport_height && viewport_height > 0
}

/// Estimate wrapped terminal rows for plain text at a fixed column width.
pub fn estimate_scroll_text_lines(text: &str, width: u16) -> u16 {
    let w = width.max(1) as usize;
    let mut rows = 0usize;
    for line in text.split('\n') {
        let n = line.chars().count();
        rows = rows.saturating_add(if n == 0 { 1 } else { n.div_ceil(w) });
    }
    rows.max(1) as u16
}

/// Whether to enable the built-in track for this body (stable: text-based, not live measure).
pub fn scroll_text_needs_scrollbar(text: &str, body_width: u16, body_height: u16) -> bool {
    // Built-in track consumes 1 column; estimate against the narrower wrap width.
    let wrap_w = body_width.saturating_sub(1).max(1);
    let lines = estimate_scroll_text_lines(text, wrap_w);
    scroll_text_scrollbar_visible(lines, body_height.max(1))
}

#[derive(Props)]
pub struct ScrollTextDialogOverlayProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub title: String,
    pub text: String,
    pub body_height: u16,
    pub chrome: DialogChrome,
    pub scroll_handle: Option<Ref<ScrollViewHandle>>,
    /// Bumped by the shell after imperative scroll (kept for parent re-layout).
    pub scroll_tick: u32,
    pub has_focus: bool,
    pub theme: Option<UiTheme>,
    /// Click on header `[esc]` (keyboard Esc is still handled by the shell).
    pub on_esc: HandlerMut<'static, ()>,
}

impl Default for ScrollTextDialogOverlayProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            title: String::new(),
            text: String::new(),
            body_height: 12,
            chrome: DialogChrome::default(),
            scroll_handle: None,
            scroll_tick: 0,
            has_focus: false,
            theme: None,
            on_esc: HandlerMut::default(),
        }
    }
}

/// Whether body text is mostly `Label: value` rows (session info, structured dumps).
pub fn text_looks_like_key_value_lines(text: &str) -> bool {
    let mut total = 0usize;
    let mut kv = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        total += 1;
        if split_key_value_line(trimmed).is_some() {
            kv += 1;
        }
    }
    total > 0 && kv * 2 >= total
}

/// Split a single `Label: value` line. Label must be short and free of path noise.
fn split_key_value_line(line: &str) -> Option<(&str, &str)> {
    let (label, value) = line.split_once(':')?;
    let label = label.trim();
    let value = value.trim_start();
    if label.is_empty() || label.chars().count() > 40 {
        return None;
    }
    // Avoid treating URLs / times as key-value (`http://…`, `12:30`).
    if label.chars().any(|ch| matches!(ch, '/' | '\\' | '@')) {
        return None;
    }
    if label.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if value.starts_with("//") {
        return None;
    }
    // Require a space after `:` (or empty value) so `http:…` / bare schemes don't match.
    let after_colon = line.split_once(':').map(|(_, rest)| rest).unwrap_or("");
    if !after_colon.is_empty() && !after_colon.starts_with(char::is_whitespace) {
        return None;
    }
    Some((label, value))
}

fn render_scroll_text_body(text: &str, body_width: u16, theme: UiTheme) -> AnyElement<'static> {
    if text_looks_like_key_value_lines(text) {
        let rows: Vec<AnyElement<'static>> = text
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    return element! {
                        View(width: body_width, height: 1u16, flex_shrink: 0f32) {}
                    }
                    .into();
                }
                if let Some((label, value)) = split_key_value_line(line.trim_end()) {
                    element! {
                        View(
                            width: body_width,
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            flex_shrink: 0f32,
                        ) {
                            Text(
                                content: format!("{label}: "),
                                color: theme.text_muted,
                                weight: Weight::Normal,
                                wrap: TextWrap::NoWrap,
                            )
                            Text(
                                content: value.to_string(),
                                color: theme.text_primary,
                                weight: Weight::Bold,
                                wrap: TextWrap::Wrap,
                            )
                        }
                    }
                    .into()
                } else {
                    element! {
                        Text(
                            content: line.to_string(),
                            color: theme.text_secondary,
                            wrap: TextWrap::Wrap,
                        )
                    }
                    .into()
                }
            })
            .collect();
        element! {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(rows)
            }
        }
        .into()
    } else {
        element! {
            Text(
                content: text.to_string(),
                color: theme.text_primary,
                wrap: TextWrap::Wrap,
            )
        }
        .into()
    }
}

/// Centered slim-header dialog with a scrollable plain-text body.
///
/// Scrollbar is the **built-in** [`ScrollView`] track so thumb position always
/// matches the same offset the content uses (shell `scroll_view_*` + mouse wheel).
///
/// When most lines look like `Label: value` (e.g. `/session`), labels use muted
/// color and values use bold primary text for easier scanning.
#[component]
pub fn ScrollTextDialogOverlay(
    props: &mut ScrollTextDialogOverlayProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let _ = props.scroll_tick;
    let _hooks = hooks;
    let theme = props.theme.unwrap_or_default();
    let body_width = props.chrome.inner_body_width().max(1);
    let header = DialogHeader::title(props.title.clone());
    let needs_scrollbar = scroll_text_needs_scrollbar(&props.text, body_width, props.body_height);
    let on_esc = props.on_esc.take();
    let body = render_scroll_text_body(&props.text, body_width, theme);

    // Shell owns ↑/↓ / PgUp / PgDn via the shared handle (keyboard_scroll off → no double step).
    // Mouse wheel stays on this ScrollView while the dialog has focus.
    let keyboard_scroll = false;
    let mouse_scroll = props.has_focus;

    // Cohesive dialog palette: accent thumb, muted-but-readable track.
    let thumb = theme.warning;
    let track = theme.text_muted;

    element! {
        DialogShellOverlay(
            screen_width: props.screen_width,
            screen_height: props.screen_height,
            chrome: props.chrome.clone(),
            header: header,
            theme: Some(theme),
            on_esc: on_esc,
        ) {
            View(
                width: body_width,
                height: props.body_height,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                View(width: 100pct, height: 100pct, overflow: Overflow::Hidden) {
                    ScrollView(
                        handle: props.scroll_handle,
                        auto_scroll: false,
                        keyboard_scroll: Some(keyboard_scroll),
                        mouse_scroll: Some(mouse_scroll),
                        scroll_step: Some(DEFAULT_SCROLL_STEP),
                        scrollbar: Some(needs_scrollbar),
                        scrollbar_thumb_color: Some(thumb),
                        scrollbar_track_color: Some(track),
                    ) {
                        #(body)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_value_detection_for_session_info() {
        let session = "Title: Fix login\nSession ID: abc\nModel: openai/gpt-4o\nContext: 75K / 500K tokens (15%)";
        assert!(text_looks_like_key_value_lines(session));
        assert!(!text_looks_like_key_value_lines("plain paragraph\nwithout labels"));
        assert_eq!(split_key_value_line("Title: Fix login"), Some(("Title", "Fix login")));
        assert!(split_key_value_line("http://example.com").is_none());
    }

    #[test]
    fn dialog_width_uses_percent_of_terminal() {
        // usable = 100 - 2 = 98; 80% → 78, floored up to MIN_DIALOG_WIDTH (40)
        assert_eq!(scroll_text_dialog_width(100, 80), 78);
        // 100% of usable
        assert_eq!(scroll_text_dialog_width(100, 100), 98);
        // 50% of 98 = 49, still above min floor
        assert_eq!(scroll_text_dialog_width(100, 50), 49);
        // Narrow terminal: no floor above usable
        assert_eq!(scroll_text_dialog_width(30, 80), 22); // usable 28 * 80% = 22
    }

    #[test]
    fn chrome_uses_slim_header_and_tall_body() {
        let (chrome, body_height) = scroll_text_dialog_chrome(100, 40, 80);
        assert!(chrome.slim_header);
        assert_eq!(chrome.width, 78);
        assert!(body_height >= 16);
    }

    #[test]
    fn scrollbar_hidden_when_content_fits() {
        assert!(!scroll_text_scrollbar_visible(10, 12));
        assert!(!scroll_text_scrollbar_visible(12, 12));
        assert!(scroll_text_scrollbar_visible(20, 12));
        assert!(!scroll_text_scrollbar_visible(20, 0));
    }

    #[test]
    fn estimate_lines_wraps_and_counts_newlines() {
        assert_eq!(estimate_scroll_text_lines("hello", 10), 1);
        assert_eq!(estimate_scroll_text_lines("hello world!!", 5), 3);
        assert_eq!(estimate_scroll_text_lines("a\nb\nc", 80), 3);
        let long = "x".repeat(200);
        let lines = estimate_scroll_text_lines(&long, 20);
        assert!(lines >= 10);
        assert!(scroll_text_scrollbar_visible(lines, 8));
    }

    #[test]
    fn needs_scrollbar_uses_stable_text_estimate() {
        let short = "one line";
        assert!(!scroll_text_needs_scrollbar(short, 40, 12));
        let long = "line\n".repeat(40);
        assert!(scroll_text_needs_scrollbar(&long, 40, 12));
    }

    #[test]
    fn pending_stores_title_text_and_default_width() {
        let pending = PendingScrollTextDialog::open("Help", "body");
        assert_eq!(pending.title, "Help");
        assert_eq!(pending.text, "body");
        assert_eq!(pending.width_pct, DEFAULT_SCROLL_TEXT_WIDTH_PCT);
        let wide = PendingScrollTextDialog::open_with_width("Help", "body", 95);
        assert_eq!(wide.width_pct, 95);
    }
}
