//! `@` mention parsing and palette snapshot derivation.

pub const MAX_VISIBLE_ROWS: u16 = 8;
pub const SEARCH_LIMIT: usize = 200;
pub const FAST_SCROLL_STEP: usize = 5;

/// One selectable row in the `@` file picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePickerOption {
    pub path: String,
    pub is_directory: bool,
}

/// Active `@` token at the editor cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveMention {
    pub start: usize,
    pub query: String,
    pub end: usize,
}

/// Render-ready file picker snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePickerSnapshot {
    pub visible: bool,
    pub query: String,
    pub options: Vec<FilePickerOption>,
    pub list_height: u16,
    pub match_count: usize,
    pub file_count: usize,
    pub dir_count: usize,
}

impl Default for FilePickerSnapshot {
    fn default() -> Self {
        Self::hidden()
    }
}

impl FilePickerSnapshot {
    pub fn hidden() -> Self {
        Self {
            visible: false,
            query: String::new(),
            options: Vec::new(),
            list_height: 0,
            match_count: 0,
            file_count: 0,
            dir_count: 0,
        }
    }

    pub fn should_render(&self) -> bool {
        self.visible && !self.options.is_empty()
    }
}

/// Locate the `@` mention token containing `cursor` (byte index).
pub fn active_mention_at_cursor(text: &str, cursor: usize) -> Option<ActiveMention> {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        return None;
    }
    let before = &text[..cursor];
    let word_start = before.rfind(char::is_whitespace).map(|index| index + 1).unwrap_or(0);
    let word = &text[word_start..cursor];
    if !word.starts_with('@') {
        return None;
    }
    let query = &text[word_start + 1..cursor];
    if query.contains(char::is_whitespace) {
        return None;
    }
    Some(ActiveMention {
        start: word_start,
        query: query.to_string(),
        end: cursor,
    })
}

pub fn complete_mention(draft: &str, mention: &ActiveMention, path: &str) -> String {
    let mut out = String::with_capacity(draft.len() + path.len() + 2);
    out.push_str(&draft[..mention.start]);
    out.push('@');
    out.push_str(path);
    out.push(' ');
    out.push_str(&draft[mention.end..]);
    out
}

/// Byte cursor after inserting `path` for an active `@` mention (after trailing space).
pub fn cursor_after_mention_complete(mention: &ActiveMention, path: &str) -> usize {
    mention.start + 1 + path.len() + 1
}

/// Draft after dismissing the picker — keeps `@`, removes the in-progress filter query.
pub fn dismiss_mention_keep_at(draft: &str, mention: &ActiveMention) -> String {
    let mut out = String::with_capacity(draft.len().saturating_sub(mention.query.len()));
    out.push_str(&draft[..mention.start]);
    out.push('@');
    out.push_str(&draft[mention.end..]);
    out
}

/// Byte cursor after dismissing the picker (caret immediately after `@`).
pub fn cursor_after_mention_dismiss(mention: &ActiveMention) -> usize {
    mention.start + 1
}

pub fn build_snapshot(
    draft: &str,
    cursor: usize,
    screen_height: u16,
    show_hidden: bool,
    index: Option<&elph_agent::tools::fff_picker::MentionSearchIndex>,
) -> FilePickerSnapshot {
    let cursor = mention_cursor_for_picker(draft, cursor);
    let Some(mention) = active_mention_at_cursor(draft, cursor) else {
        return FilePickerSnapshot::hidden();
    };

    let hits = index
        .map(|idx| idx.fuzzy_search_paths(&mention.query, SEARCH_LIMIT, show_hidden))
        .unwrap_or_default();
    let mut file_count = 0usize;
    let mut dir_count = 0usize;
    let mut options = Vec::with_capacity(hits.len());
    for hit in hits {
        if hit.is_directory {
            dir_count += 1;
        } else {
            file_count += 1;
        }
        options.push(FilePickerOption {
            path: hit.path,
            is_directory: hit.is_directory,
        });
    }
    let match_count = options.len();
    let list_height = mention_list_height(match_count, screen_height);

    FilePickerSnapshot {
        visible: true,
        query: mention.query,
        options,
        list_height,
        match_count,
        file_count,
        dir_count,
    }
}

/// Path inserted into the draft when a picker row is accepted.
pub fn mention_completion_path(option: &FilePickerOption) -> String {
    if option.is_directory {
        elph_agent::tools::fff_picker::format_directory_path(&option.path)
    } else {
        option.path.clone()
    }
}

pub fn selected_completion_path(options: &[FilePickerOption], selected_index: usize) -> Option<String> {
    if options.is_empty() {
        return None;
    }
    let index = selected_index.min(options.len() - 1);
    options.get(index).map(mention_completion_path)
}

/// Title line for the floating `@` picker chrome.
pub fn file_picker_title(query: &str, file_count: usize, dir_count: usize, show_hidden: bool) -> String {
    let hidden_hint = if show_hidden {
        "hidden shown · Ctrl+. hide"
    } else {
        "Ctrl+. show hidden"
    };
    format!("@{query} · {file_count} files · {dir_count} folders ({hidden_hint})")
}

/// Whether the `@` file picker should stay open for this draft and cursor.
pub fn mention_picker_visible(draft: &str, cursor: usize) -> bool {
    active_mention_at_cursor(draft, mention_cursor_for_picker(draft, cursor)).is_some()
}

/// Cursor used for `@` picker lookup — falls back to EOF when the live caret is stale.
pub fn mention_cursor_for_picker(draft: &str, cursor: usize) -> usize {
    let cursor = cursor.min(draft.len());
    if active_mention_at_cursor(draft, cursor).is_some() {
        return cursor;
    }
    if active_mention_at_cursor(draft, draft.len()).is_some() {
        return draft.len();
    }
    cursor
}

/// True when the `@` picker is open — checks live cursor and EOF (stale cursor safe).
pub fn file_picker_open(draft: &str, cursor: usize) -> bool {
    mention_picker_visible(draft, cursor)
}

fn mention_list_height(option_count: usize, screen_height: u16) -> u16 {
    if option_count == 0 {
        return 1;
    }
    let cap = if screen_height < 24 {
        4
    } else if screen_height < 36 {
        6
    } else {
        MAX_VISIBLE_ROWS as usize
    };
    option_count.min(cap).max(1).min(MAX_VISIBLE_ROWS as usize) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_active_mention_after_at() {
        let text = "see @src/ma";
        let mention = active_mention_at_cursor(text, text.len()).expect("mention");
        assert_eq!(mention.start, 4);
        assert_eq!(mention.query, "src/ma");
    }

    #[test]
    fn no_mention_when_cursor_before_at() {
        assert!(active_mention_at_cursor("@foo", 0).is_none());
    }

    #[test]
    fn complete_mention_inserts_path_and_trailing_space() {
        let text = "fix @rs";
        let mention = active_mention_at_cursor(text, text.len()).expect("mention");
        let completed = complete_mention(text, &mention, "src/main.rs");
        assert_eq!(completed, "fix @src/main.rs ");
    }

    #[test]
    fn hidden_snapshot_without_index() {
        let snapshot = build_snapshot("hello", 5, 40, true, None);
        assert!(!snapshot.visible);
    }

    #[test]
    fn picker_hides_after_mention_complete_cursor() {
        let draft = "fix @ma";
        let mention = active_mention_at_cursor(draft, draft.len()).expect("mention");
        let completed = complete_mention(draft, &mention, "src/main.rs");
        let cursor = cursor_after_mention_complete(&mention, "src/main.rs");
        assert_eq!(completed, "fix @src/main.rs ");
        assert_eq!(cursor, completed.len());
        assert!(active_mention_at_cursor(&completed, cursor).is_none());
    }

    #[test]
    fn mention_cursor_for_picker_falls_back_to_eof() {
        let draft = "fix @main";
        assert_eq!(mention_cursor_for_picker(draft, 4), draft.len());
        assert_eq!(mention_cursor_for_picker(draft, draft.len()), draft.len());
    }

    #[test]
    fn file_picker_open_with_stale_cursor_before_eof_mention() {
        let draft = "fix @main";
        assert!(mention_picker_visible(draft, 4));
        assert!(file_picker_open(draft, 4));
    }

    #[test]
    fn picker_hides_after_mention_dismiss_cursor() {
        let draft = "fix @ma";
        let mention = active_mention_at_cursor(draft, draft.len()).expect("mention");
        let cursor = cursor_after_mention_dismiss(&mention);
        let dismissed = dismiss_mention_keep_at(draft, &mention);
        assert_eq!(dismissed, "fix @");
        assert_eq!(cursor, 5);
        assert_eq!(&dismissed[mention.start..mention.start + 1], "@");
    }

    #[test]
    fn mention_completion_path_adds_directory_suffix() {
        let option = FilePickerOption {
            path: "src".into(),
            is_directory: true,
        };
        assert_eq!(mention_completion_path(&option), "src/");
    }

    #[test]
    fn file_picker_title_includes_file_and_folder_counts() {
        let title = file_picker_title("ma", 3, 2, false);
        assert_eq!(title, "@ma · 3 files · 2 folders (Ctrl+. show hidden)");
    }
}
