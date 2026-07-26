//! Reusable modal for scrolling long plain text (system prompt, help dumps, logs, …).
//!
//! Lives in the Elph TUI shell layer (not `elph-tui`) so app-specific focus and
//! prompt lifecycle stay colocated with other overlays.

use elph_tui::components::theme::UiTheme;
use elph_tui::components::{
    DialogChrome, DialogHeader, DialogShellOverlay, VerticalScrollbar, dialog_body_min_height,
    dialog_max_content_height,
};
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

/// Whether the custom track should be painted (content taller than the viewport).
pub fn scroll_text_scrollbar_visible(content_height: u16, viewport_height: u16) -> bool {
    content_height > viewport_height && viewport_height > 0
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
    /// Bumped by the shell after imperative scroll so the scrollbar re-renders.
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

/// Centered slim-header dialog with a scrollable plain-text body and optional scrollbar.
#[component]
pub fn ScrollTextDialogOverlay(
    props: &mut ScrollTextDialogOverlayProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let _ = props.scroll_tick;
    let _hooks = hooks;
    let theme = props.theme.unwrap_or_default();
    let body_width = props.chrome.inner_body_width();
    let header = DialogHeader::title(props.title.clone());

    let (scroll_offset, content_height, viewport_height) = props
        .scroll_handle
        .as_ref()
        .map(|handle| {
            let guard = handle.read();
            (
                guard.scroll_offset().max(0) as u16,
                // Prefer measured content height; fall back so we don't paint a
                // full-height thumb before the first measure settles.
                guard.content_height().max(1),
                guard.viewport_height().max(props.body_height).max(1),
            )
        })
        .unwrap_or((0, 1, props.body_height.max(1)));

    let show_scrollbar = scroll_text_scrollbar_visible(content_height, viewport_height);
    let scroll_width = if show_scrollbar {
        body_width.saturating_sub(1).max(1)
    } else {
        body_width.max(1)
    };
    // Shell owns ↑/↓ / PgUp / PgDn via the shared handle; avoid double-stepping.
    // Mouse wheel is owned by this dialog while focused (transcript mouse is disabled).
    let keyboard_scroll = false;
    let mouse_scroll = props.has_focus;

    let scrollbar: Option<AnyElement<'static>> = if show_scrollbar {
        Some(
            element! {
                VerticalScrollbar(
                    viewport_height: viewport_height,
                    content_height: content_height,
                    scroll_offset: scroll_offset,
                    theme: Some(theme),
                )
            }
            .into(),
        )
    } else {
        None
    };

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
                flex_direction: FlexDirection::Row,
                gap: 0,
                flex_shrink: 0f32,
                align_items: AlignItems::FlexStart,
                overflow: Overflow::Hidden,
            ) {
                View(
                    width: scroll_width,
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
                            scrollbar: Some(false),
                        ) {
                            Text(
                                content: props.text.clone(),
                                color: theme.text_primary,
                                wrap: TextWrap::Wrap,
                            )
                        }
                    }
                }
                #(scrollbar)
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
    fn pending_stores_title_and_text() {
        let pending = PendingScrollTextDialog::open("Help", "body");
        assert_eq!(pending.title, "Help");
        assert_eq!(pending.text, "body");
    }
}
