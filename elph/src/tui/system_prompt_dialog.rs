//! System-prompt viewer built on the reusable [`crate::tui::scroll_text_dialog`].

use iocraft::prelude::*;

use crate::tui::focus::ShellFocus;
use crate::tui::scroll_text_dialog::{
    CloseScrollTextDialogArgs, OpenScrollTextDialogArgs, PendingScrollTextDialog, ScrollTextClosePrompt,
    close_scroll_text_dialog, open_scroll_text_dialog,
};

/// Default header for `/system-prompt`.
pub const SYSTEM_PROMPT_DIALOG_TITLE: &str = "System prompt";

/// Open system-prompt session (title is fixed; body is the compiled prompt).
pub type PendingSystemPromptDialog = PendingScrollTextDialog;

/// Re-export layout helpers under the system-prompt names used by the shell.
pub use crate::tui::scroll_text_dialog::scroll_text_dialog_chrome as system_prompt_dialog_chrome;
pub use crate::tui::scroll_text_dialog::scroll_text_dialog_width as system_prompt_dialog_width;
pub use crate::tui::scroll_text_dialog::scroll_text_scrollbar_visible as system_prompt_scrollbar_visible;

/// Arguments for [`open_system_prompt_dialog`].
pub struct OpenSystemPromptDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingSystemPromptDialog>>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub text: String,
}

pub fn open_system_prompt_dialog(args: OpenSystemPromptDialogArgs<'_>) {
    open_scroll_text_dialog(OpenScrollTextDialogArgs {
        pending: args.pending,
        shell_focus: args.shell_focus,
        title: SYSTEM_PROMPT_DIALOG_TITLE.to_string(),
        text: args.text,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_and_chrome_delegate_to_scroll_text_dialog() {
        assert_eq!(system_prompt_dialog_width(80), 76);
        let (chrome, body_height) = system_prompt_dialog_chrome(100, 40);
        assert!(chrome.slim_header);
        assert!(body_height >= 16);
    }

    #[test]
    fn scrollbar_visibility_matches_generic_helper() {
        assert!(!system_prompt_scrollbar_visible(10, 12));
        assert!(system_prompt_scrollbar_visible(20, 12));
    }

    #[test]
    fn open_title_is_system_prompt() {
        assert_eq!(SYSTEM_PROMPT_DIALOG_TITLE, "System prompt");
    }
}
