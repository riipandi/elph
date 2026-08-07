//! `/rename` inline dialog — free-text session title editor (ask-user style input).

use elph_tui::components::{DialogUserInputContent, UiTheme};
use iocraft::prelude::*;

use crate::tui::focus::ShellFocus;
use crate::tui::inline_dialog::{InlineDialogShell, inline_body_width};

/// Open rename session dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRenameDialog {
    /// Prefill (current title or slash args).
    pub initial: String,
    /// Prompt draft stashed while the dialog is open.
    pub stashed_prompt_draft: Option<String>,
}

/// Arguments for [`open_rename_dialog`].
pub struct OpenRenameDialogArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingRenameDialog>>,
    pub value: &'a mut State<String>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub initial: String,
}

pub fn open_rename_dialog(args: OpenRenameDialogArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }
    args.value.set(args.initial.clone());
    args.pending.set(Some(PendingRenameDialog {
        initial: args.initial,
        stashed_prompt_draft: stashed,
    }));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

pub fn close_rename_dialog(
    pending: &mut Ref<Option<PendingRenameDialog>>,
    value: &mut State<String>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    restore_stash: bool,
) {
    let stashed = pending.write().take().and_then(|p| p.stashed_prompt_draft);
    value.set(String::new());
    if restore_stash {
        if let Some(text) = stashed {
            draft.set(text.clone());
            live_draft.set(text);
        } else {
            draft.set(String::new());
            live_draft.set(String::new());
        }
    } else {
        draft.set(String::new());
        live_draft.set(String::new());
    }
    shell_focus.set(ShellFocus::Prompt);
}

/// Props for [`RenameDialogBar`].
#[derive(Props)]
pub struct RenameDialogBarProps {
    pub screen_width: u16,
    pub has_focus: bool,
    pub value: Option<State<String>>,
    pub on_submit: HandlerMut<'static, ()>,
    pub on_cancel: HandlerMut<'static, ()>,
}

impl Default for RenameDialogBarProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            has_focus: false,
            value: None,
            on_submit: HandlerMut::default(),
            on_cancel: HandlerMut::default(),
        }
    }
}

#[component]
pub fn RenameDialogBar(props: &mut RenameDialogBarProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);
    let body = element! {
        View(
            width: body_width,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            // One blank row under the header divider so the field is not flush to the title.
            View(width: body_width, padding_top: 1, flex_shrink: 0f32) {
                DialogUserInputContent(
                    width: body_width,
                    question: String::new(),
                    placeholder: "Session title…".to_string(),
                    value: props.value,
                    has_focus: props.has_focus,
                    theme: Some(theme),
                    section_gap: Some(0),
                    show_prompt: false,
                    show_footer_hint: false,
                    show_placeholder_when_focused: true,
                    dialog_chrome: true,
                    compact: true,
                    on_submit: props.on_submit.take(),
                    on_cancel: props.on_cancel.take(),
                )
            }
        }
    };

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: "Rename session".to_string(),
            has_focus: props.has_focus,
            footer_hint: Some("Enter save · Esc cancel".to_string()),
        ) {
            #(body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_holds_initial_title() {
        let pending = PendingRenameDialog {
            initial: "Fix auth".into(),
            stashed_prompt_draft: None,
        };
        assert_eq!(pending.initial, "Fix auth");
    }
}
