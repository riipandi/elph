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

const MIN_DIALOG_WIDTH: u16 = 72;
const MAX_DIALOG_WIDTH: u16 = 120;
const SCREEN_WIDTH_MARGIN: u16 = 4;
const SCREEN_HEIGHT_MARGIN: u16 = 4;
const DEFAULT_SCROLL_STEP: u16 = 3;

/// Open scroll-text viewer session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingScrollTextDialog {
    pub title: String,
    pub text: String,
}

impl PendingScrollTextDialog {
    pub fn open(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
        }
    }
}

/// Arguments for [`open_scroll_text_dialog`].
pub struct OpenScrollTextDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingScrollTextDialog>>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub title: String,
    pub text: String,
}

pub fn open_scroll_text_dialog(args: OpenScrollTextDialogArgs<'_>) {
    args.pending
        .set(Some(PendingScrollTextDialog::open(args.title, args.text)));
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

/// Responsive outer width: wide on large terminals, still inset on small ones.
pub fn scroll_text_dialog_width(screen_width: u16) -> u16 {
    let usable = screen_width.saturating_sub(SCREEN_WIDTH_MARGIN).max(1);
    if usable <= MIN_DIALOG_WIDTH {
        return usable;
    }
    usable.min(MAX_DIALOG_WIDTH)
}

/// Slim-header chrome and body viewport height for the scroll-text viewer.
pub fn scroll_text_dialog_chrome(screen_width: u16, screen_height: u16) -> (DialogChrome, u16) {
    let outer = scroll_text_dialog_width(screen_width);
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
        }
    }
}

/// Centered slim-header dialog with a scrollable plain-text body.
///
/// Scrollbar is the **built-in** [`ScrollView`] track so thumb position always
/// matches the same offset the content uses (shell `scroll_view_*` + mouse wheel).
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
                        Text(
                            content: props.text.clone(),
                            color: theme.text_primary,
                            wrap: TextWrap::Wrap,
                        )
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
    fn dialog_width_scales_with_terminal() {
        assert_eq!(scroll_text_dialog_width(80), 76);
        assert_eq!(scroll_text_dialog_width(140), 120);
        assert_eq!(scroll_text_dialog_width(60), 56);
    }

    #[test]
    fn chrome_uses_slim_header_and_tall_body() {
        let (chrome, body_height) = scroll_text_dialog_chrome(100, 40);
        assert!(chrome.slim_header);
        assert_eq!(chrome.width, 96);
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
    fn pending_stores_title_and_text() {
        let pending = PendingScrollTextDialog::open("Help", "body");
        assert_eq!(pending.title, "Help");
        assert_eq!(pending.text, "body");
    }
}
