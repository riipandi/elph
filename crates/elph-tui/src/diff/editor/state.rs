use crate::diff::autocomplete::{AutocompletePopup, AutocompleteProvider};
use crate::diff::component::LineComponent;
use crate::diff::paste::{
    CollapsedPaste, adjust_pastes_for_delete, expand_paste_markers, normalize_paste_text, reconcile_paste_offsets,
    remove_paste_block_and_adjust, shift_paste_offsets_for_insert, should_collapse_paste,
};

use super::{Editor, EditorSnapshot};

impl Editor {
    pub fn set_autocomplete_provider(&mut self, provider: Box<dyn AutocompleteProvider>) {
        self.autocomplete_provider = Some(provider);
    }

    pub fn take_autocomplete_popup(&mut self) -> Option<AutocompletePopup> {
        self.pending_autocomplete.take()
    }

    pub(super) fn token_before_cursor(&self) -> String {
        let before = &self.text[..self.cursor.min(self.text.len())];
        before.split_whitespace().next_back().unwrap_or(before).to_string()
    }

    pub(super) fn open_path_autocomplete(&mut self) {
        let Some(provider) = self.autocomplete_provider.as_ref() else {
            return;
        };
        let token = self.token_before_cursor();
        let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let paths = provider.complete_path(&token, &cwd);
        if !paths.is_empty() {
            self.pending_autocomplete = Some(AutocompletePopup::paths(paths));
        }
    }

    pub(super) fn open_slash_autocomplete(&mut self) {
        let Some(provider) = self.autocomplete_provider.as_ref() else {
            return;
        };
        let token = self.token_before_cursor();
        let filter = token.trim_start_matches('/');
        self.pending_autocomplete = Some(AutocompletePopup::slash_commands(provider.slash_commands(), filter));
    }

    pub fn with_max_visible_rows(mut self, rows: usize) -> Self {
        self.max_visible_rows = rows.max(1);
        self
    }

    pub fn set_padding_x(&mut self, padding_x: u16) {
        self.padding_x = padding_x;
        LineComponent::invalidate(self);
    }

    pub fn set_disable_submit(&mut self, disabled: bool) {
        self.disable_submit = disabled;
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn get_expanded_text(&self) -> String {
        expand_paste_markers(&self.text, &self.pastes)
    }

    pub fn set_cursor(&mut self, offset: usize) {
        self.cursor = offset.min(self.text.len());
        LineComponent::invalidate(self);
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.undo.clear();
        self.push_undo();
        self.text = text.into();
        self.cursor = self.text.len().min(self.cursor);
        self.notify_change();
        LineComponent::invalidate(self);
    }

    pub fn insert_text_at_cursor(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.push_undo();
        self.insert_normalized(normalize_paste_text(text), 40);
        self.notify_change();
        LineComponent::invalidate(self);
    }

    pub(super) fn notify_change(&mut self) {
        if let Some(cb) = &mut self.on_change {
            cb(&self.text);
        }
    }

    fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            text: self.text.clone(),
            cursor: self.cursor,
        }
    }

    pub(super) fn push_undo(&mut self) {
        const MAX_UNDO_DEPTH: usize = 200;
        if self.undo.len() >= MAX_UNDO_DEPTH {
            self.undo.clear();
        }
        self.undo.push(self.snapshot());
    }

    pub(super) fn restore(&mut self, snap: EditorSnapshot) {
        self.text = snap.text;
        self.cursor = snap.cursor.min(self.text.len());
        self.notify_change();
        LineComponent::invalidate(self);
    }

    pub(super) fn insert_normalized(&mut self, normalized: String, preview_width: usize) {
        if should_collapse_paste(&normalized) {
            let collapsed = CollapsedPaste::new(normalized, preview_width, self.cursor);
            shift_paste_offsets_for_insert(&mut self.pastes, self.cursor, collapsed.summary.len());
            self.text.insert_str(self.cursor, &collapsed.summary);
            self.cursor += collapsed.summary.len();
            self.pastes.push(collapsed);
        } else {
            shift_paste_offsets_for_insert(&mut self.pastes, self.cursor, normalized.len());
            self.text.insert_str(self.cursor, &normalized);
            self.cursor += normalized.len();
        }
    }

    pub(super) fn kill_and_apply(
        &mut self,
        deleted: &str,
        prepend: bool,
        accumulate: bool,
        next: String,
        cursor: usize,
    ) {
        self.kill_ring.push(deleted, prepend, accumulate);
        self.text = next;
        self.cursor = cursor;
        if !self.pastes.is_empty() {
            reconcile_paste_offsets(&self.text, &mut self.pastes);
        }
        self.notify_change();
        LineComponent::invalidate(self);
    }

    pub(super) fn delete_paste_block_at(&mut self, range: std::ops::Range<usize>) -> bool {
        let Some(next) = remove_paste_block_and_adjust(&self.text, range.clone(), &mut self.pastes) else {
            return false;
        };
        self.text = next;
        self.cursor = range.start;
        self.last_yank_len = 0;
        self.notify_change();
        LineComponent::invalidate(self);
        true
    }

    pub(super) fn delete_scalar_range(&mut self, range: std::ops::Range<usize>, next: String, cursor: usize) {
        adjust_pastes_for_delete(&mut self.pastes, range);
        self.text = next;
        self.cursor = cursor;
        self.notify_change();
        LineComponent::invalidate(self);
    }

    pub(super) fn invalidate(&mut self) {
        LineComponent::invalidate(self);
    }
}
