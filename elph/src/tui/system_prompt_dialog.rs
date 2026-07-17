//! Modal dialog for viewing the compiled system prompt.

use elph_tui::components::{
    DialogChrome, DialogHeader, DialogShellOverlay, VerticalScrollbar, dialog_body_min_height,
    dialog_max_content_height,
};
use iocraft::prelude::*;

use crate::tui::focus::ShellFocus;
use crate::tui::theme::TEXT_FG;

const MIN_DIALOG_WIDTH: u16 = 72;
const MAX_DIALOG_WIDTH: u16 = 120;
const SCREEN_WIDTH_MARGIN: u16 = 4;
const SCREEN_HEIGHT_MARGIN: u16 = 4;

#[derive(Debug, Clone)]
pub struct PendingSystemPromptDialog {
    pub text: String,
}

impl PendingSystemPromptDialog {
    pub fn open(text: String) -> Self {
        Self { text }
    }
}

/// Arguments for [`open_system_prompt_dialog`].
pub struct OpenSystemPromptDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingSystemPromptDialog>>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub text: String,
}

pub fn open_system_prompt_dialog(args: OpenSystemPromptDialogArgs<'_>) {
    args.pending.set(Some(PendingSystemPromptDialog::open(args.text)));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

pub fn close_system_prompt_dialog(
    pending: &mut Ref<Option<PendingSystemPromptDialog>>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    force_editor_clear: &mut Ref<bool>,
) {
    pending.write().take();
    draft.set(String::new());
    live_draft.set(String::new());
    force_editor_clear.set(true);
    shell_focus.set(ShellFocus::Prompt);
}

/// Responsive outer width: wide on large terminals, still inset on small ones.
pub fn system_prompt_dialog_width(screen_width: u16) -> u16 {
    let usable = screen_width.saturating_sub(SCREEN_WIDTH_MARGIN).max(1);
    if usable <= MIN_DIALOG_WIDTH {
        return usable;
    }
    usable.min(MAX_DIALOG_WIDTH)
}

pub fn system_prompt_dialog_chrome(screen_width: u16, screen_height: u16) -> (DialogChrome, u16) {
    let outer = system_prompt_dialog_width(screen_width);
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

#[derive(Props)]
pub struct SystemPromptDialogOverlayProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub text: String,
    pub body_height: u16,
    pub chrome: DialogChrome,
    pub scroll_handle: Option<Ref<ScrollViewHandle>>,
    pub scroll_tick: u32,
    pub has_focus: bool,
}

impl Default for SystemPromptDialogOverlayProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            text: String::new(),
            body_height: 12,
            chrome: DialogChrome::default(),
            scroll_handle: None,
            scroll_tick: 0,
            has_focus: false,
        }
    }
}

#[component]
pub fn SystemPromptDialogOverlay(
    props: &mut SystemPromptDialogOverlayProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let _ = props.scroll_tick;
    let body_width = props.chrome.inner_body_width();
    let scroll_width = body_width.saturating_sub(1).max(1);
    let header = DialogHeader::title("System prompt");

    let (scroll_offset, content_height, viewport_height) = props
        .scroll_handle
        .as_ref()
        .map(|handle| {
            let guard = handle.read();
            (
                guard.scroll_offset().max(0) as u16,
                guard.content_height().max(props.body_height),
                guard.viewport_height().max(props.body_height),
            )
        })
        .unwrap_or((0, props.body_height, props.body_height));

    let _hooks = hooks;
    let keyboard_scroll = props.has_focus;
    let scroll_step = 3u16;

    element! {
        DialogShellOverlay(
            screen_width: props.screen_width,
            screen_height: props.screen_height,
            chrome: props.chrome.clone(),
            header: header,
        ) {
            View(
                width: body_width,
                height: props.body_height,
                flex_direction: FlexDirection::Row,
                gap: 0,
                flex_shrink: 0f32,
                align_items: AlignItems::FlexStart,
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
                            scroll_step: Some(scroll_step),
                            scrollbar: Some(false),
                        ) {
                            Text(content: props.text.clone(), color: TEXT_FG, wrap: TextWrap::Wrap)
                        }
                    }
                }
                VerticalScrollbar(
                    viewport_height: viewport_height,
                    content_height: content_height,
                    scroll_offset: scroll_offset,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_width_scales_with_terminal() {
        assert_eq!(system_prompt_dialog_width(80), 76);
        assert_eq!(system_prompt_dialog_width(140), 120);
        assert_eq!(system_prompt_dialog_width(60), 56);
    }

    #[test]
    fn chrome_uses_slim_header_and_tall_body() {
        let (chrome, body_height) = system_prompt_dialog_chrome(100, 40);
        assert!(chrome.slim_header);
        assert_eq!(chrome.width, 96);
        assert!(body_height >= 16);
    }
}
