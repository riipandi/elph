//! System-prompt viewer built on the reusable [`crate::tui::scroll_text_dialog`].

use iocraft::prelude::*;

use crate::tui::focus::ShellFocus;
use crate::tui::scroll_text_dialog::{
    CloseScrollTextDialogArgs, DEFAULT_SCROLL_TEXT_WIDTH_PCT, OpenScrollTextDialogArgs, PendingScrollTextDialog,
    ScrollTextClosePrompt, close_scroll_text_dialog, open_scroll_text_dialog, scroll_text_dialog_chrome,
};

/// Default header for `/system-prompt`.
pub const SYSTEM_PROMPT_DIALOG_TITLE: &str = "System prompt";

/// Open system-prompt session (title is fixed; body is the compiled prompt).
pub type PendingSystemPromptDialog = PendingScrollTextDialog;

/// Arguments for [`open_system_prompt_dialog`].
pub struct OpenSystemPromptDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingSystemPromptDialog>>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub text: String,
    /// Width as % of terminal. Defaults to [`DEFAULT_SCROLL_TEXT_WIDTH_PCT`] when `None`.
    pub width_pct: Option<u8>,
}

pub fn open_system_prompt_dialog(args: OpenSystemPromptDialogArgs<'_>) {
    open_scroll_text_dialog(OpenScrollTextDialogArgs {
        pending: args.pending,
        shell_focus: args.shell_focus,
        title: SYSTEM_PROMPT_DIALOG_TITLE.to_string(),
        text: args.text,
        width_pct: args.width_pct.unwrap_or(DEFAULT_SCROLL_TEXT_WIDTH_PCT),
        body_height: None,
        show_copy: true,
    });
}

pub fn close_system_prompt_dialog(
    pending: &mut Ref<Option<PendingSystemPromptDialog>>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    force_editor_clear: &mut Ref<bool>,
) {
    close_scroll_text_dialog(CloseScrollTextDialogArgs {
        pending,
        draft,
        live_draft,
        shell_focus,
        force_editor_clear: Some(force_editor_clear),
        // Slash-opened viewer should not leave `/system-prompt` in the input.
        prompt: ScrollTextClosePrompt::Clear,
    });
}

/// Layout helpers (pass `pending.width_pct` from the open session).
pub fn system_prompt_dialog_chrome(
    screen_width: u16,
    screen_height: u16,
    width_pct: u8,
) -> (elph_tui::components::DialogChrome, u16) {
    scroll_text_dialog_chrome(screen_width, screen_height, width_pct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::scroll_text_dialog::{scroll_text_dialog_width, scroll_text_scrollbar_visible};

    #[test]
    fn width_and_chrome_use_percent() {
        assert_eq!(scroll_text_dialog_width(100, DEFAULT_SCROLL_TEXT_WIDTH_PCT), 78);
        let (chrome, body_height) = system_prompt_dialog_chrome(100, 40, 80);
        assert!(chrome.slim_header);
        assert_eq!(chrome.width, 78);
        assert!(body_height >= 16);
    }

    #[test]
    fn scrollbar_visibility_matches_generic_helper() {
        assert!(!scroll_text_scrollbar_visible(10, 12));
        assert!(scroll_text_scrollbar_visible(20, 12));
    }

    #[test]
    fn open_title_is_system_prompt() {
        assert_eq!(SYSTEM_PROMPT_DIALOG_TITLE, "System prompt");
    }
}
