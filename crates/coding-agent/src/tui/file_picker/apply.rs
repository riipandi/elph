//! Apply `@` file picker key actions from the flushed editor buffer.

use elph_tui::PaletteKeyInput;

use super::FilePickerKeyAction;
use super::model::{dismiss_mention_keep_at, mention_cursor_for_picker, selected_completion_path};
use super::{
    active_mention_at_cursor, build_snapshot, cursor_after_mention_complete, cursor_after_mention_dismiss,
    resolve_key_action, sync_selection,
};
use crate::tui::focus::ShellFocus;
use elph_agent::tools::fff_picker::MentionSearchIndex;
use iocraft::prelude::*;

/// Mutable shell state touched by file picker palette keys.
pub struct FilePickerApplyContext<'a> {
    pub screen_height: u16,
    pub show_hidden: bool,
    pub mention_index: Option<&'a MentionSearchIndex>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub live_cursor: &'a mut Ref<usize>,
    pub file_picker_index: &'a mut State<usize>,
    pub file_picker_query: &'a mut Ref<String>,
    pub file_picker_active: &'a mut Ref<bool>,
    pub file_picker_suppressed: &'a mut Ref<bool>,
    pub file_picker_key_handled: &'a mut Ref<bool>,
    pub suppress_enter_newline: &'a mut Ref<bool>,
    pub force_palette_sync: &'a mut Ref<bool>,
    pub shell_focus: &'a mut State<ShellFocus>,
}

/// Resolve and apply one file picker key against the flushed editor buffer.
pub fn apply_file_picker_key(input: PaletteKeyInput, ctx: &mut FilePickerApplyContext<'_>) {
    ctx.file_picker_key_handled.set(false);

    let cursor = mention_cursor_for_picker(&input.draft, input.cursor);
    let mut index = ctx.file_picker_index.get();
    let snapshot = build_snapshot(&input.draft, cursor, ctx.screen_height, ctx.show_hidden, ctx.mention_index);
    {
        let mut query = ctx.file_picker_query.write();
        sync_selection(&mut query, &mut index, &snapshot);
    }
    if index != ctx.file_picker_index.get() {
        ctx.file_picker_index.set(index);
    }

    let Some(action) = resolve_key_action(&input.draft, cursor, &snapshot, index, input.code, input.modifiers) else {
        return;
    };

    match action {
        FilePickerKeyAction::CompleteDraft {
            text: completed,
            suppress_enter_newline: suppress_enter,
        } => {
            if let Some(mention) = active_mention_at_cursor(&input.draft, cursor)
                && let Some(path) = selected_completion_path(&snapshot.options, index)
            {
                ctx.live_cursor.set(cursor_after_mention_complete(&mention, &path));
            } else {
                ctx.live_cursor.set(completed.len());
            }
            ctx.draft.set(completed.clone());
            ctx.live_draft.set(completed);
            ctx.suppress_enter_newline.set(suppress_enter);
            ctx.force_palette_sync.set(true);
            ctx.file_picker_suppressed.set(false);
            ctx.file_picker_active.set(false);
            ctx.file_picker_query.write().clear();
            ctx.file_picker_index.set(0);
            ctx.shell_focus.set(ShellFocus::Prompt);
            ctx.file_picker_key_handled.set(true);
        }
        FilePickerKeyAction::MoveSelection(next) => {
            ctx.file_picker_index.set(next);
            ctx.file_picker_key_handled.set(true);
        }
        FilePickerKeyAction::ClearFilter => {
            if let Some(mention) = active_mention_at_cursor(&input.draft, cursor) {
                let next = dismiss_mention_keep_at(&input.draft, &mention);
                ctx.live_cursor.set(cursor_after_mention_dismiss(&mention));
                ctx.draft.set(next.clone());
                ctx.live_draft.set(next);
                ctx.force_palette_sync.set(true);
            }
            ctx.file_picker_suppressed.set(false);
            ctx.file_picker_index.set(0);
            ctx.file_picker_query.write().clear();
            ctx.shell_focus.set(ShellFocus::Prompt);
            ctx.file_picker_key_handled.set(true);
        }
        FilePickerKeyAction::Dismiss => {
            if let Some(mention) = active_mention_at_cursor(&input.draft, cursor) {
                let next = dismiss_mention_keep_at(&input.draft, &mention);
                ctx.live_cursor.set(cursor_after_mention_dismiss(&mention));
                ctx.draft.set(next.clone());
                ctx.live_draft.set(next);
                ctx.force_palette_sync.set(true);
            }
            ctx.file_picker_suppressed.set(true);
            ctx.file_picker_active.set(false);
            ctx.file_picker_index.set(0);
            ctx.file_picker_query.write().clear();
            ctx.shell_focus.set(ShellFocus::Prompt);
            ctx.file_picker_key_handled.set(true);
        }
        FilePickerKeyAction::ToggleHiddenFiles => {}
    }
}
