use super::agent_mode::AgentMode;
use super::paste_guard::PasteGuard;
use super::prompt_buffer::{PromptBuffer, wrapped_row_count};
use super::prompt_display::PromptDisplay;
use super::prompt_edit::{
    char_left, char_right, delete_char_backward, delete_char_forward, delete_to_line_end, delete_to_line_start,
    delete_word_backward, delete_word_forward, line_end, line_start, word_left, word_right,
};
use super::prompt_keys::{EditAction, edit_action, is_clear_key, is_mode_cycle_key, is_newline_key, is_submit_key};
use super::prompt_paste::{
    CollapsedPaste, PendingPaste, expand_paste_markers, finalize_pending_paste, paste_block_range, remove_paste_block,
    should_collapse_paste,
};
use crate::theme::Theme;
use iocraft::prelude::*;
use std::time::Instant;

const PROMPT_PREFIX: &str = "> ";
const MIN_INPUT_LINES: u16 = 1;
const MAX_INPUT_LINES: u16 = 5;
/// Fallback before the text field has been measured.
const FALLBACK_TEXT_WIDTH: u16 = 40;
/// Horizontal space taken by app padding, border padding, prefix, and border glyphs.
const HORIZONTAL_CHROME: u16 = 8;

#[derive(Default, Props)]
pub struct PromptInputProps {
    /// Prompt text state (see iocraft `form` example).
    pub value: Option<State<String>>,

    /// Model name shown below the input (e.g. `claude-fable-5`).
    pub model_name: String,

    /// Current agent mode; tints the prompt border and mode label in the footer.
    pub mode: AgentMode,

    /// Whether the text field accepts keyboard input.
    pub has_focus: bool,

    /// Called when Enter is pressed to send/submit the prompt.
    pub on_submit: HandlerMut<'static, String>,

    /// Called when the user cycles agent mode (Tab).
    pub on_mode_change: HandlerMut<'static, AgentMode>,

    /// Bumped by the parent to reset the field after Ctrl+C / SIGINT clear.
    pub reset_nonce: Option<State<u32>>,

    /// Active color palette.
    pub theme: Theme,
}

#[component]
pub fn PromptInput(mut hooks: Hooks, props: &mut PromptInputProps) -> impl Into<AnyElement<'static>> {
    let Some(mut value) = props.value else {
        panic!("value is required");
    };
    let theme = props.theme;
    let mode_color = theme.mode_accent(props.mode);
    let model_status = format!("{} • ", props.model_name);
    let mode_label = props.mode.label();
    let (terminal_width, _) = hooks.use_terminal_size();
    let fallback_width = terminal_width
        .saturating_sub(HORIZONTAL_CHROME)
        .max(FALLBACK_TEXT_WIDTH);
    let measured_width = hooks.use_state(move || fallback_width);
    let current = value.read().clone();
    let text_width = measured_width.get().max(1);
    let mut stable_height = hooks.use_state(|| MIN_INPUT_LINES);
    let mut cursor_offset = hooks.use_state(|| 0usize);
    let mut vertical_col_pref = hooks.use_state(|| None::<u16>);
    let computed_height = visual_line_count(&current, text_width);
    let input_height = stable_height.get().max(computed_height).min(MAX_INPUT_LINES);
    let mut field_clear_generation = hooks.use_state(|| 0u32);
    let mut paste_guard = hooks.use_ref(PasteGuard::default);
    let mut pending_paste = hooks.use_ref(|| None::<PendingPaste>);
    let mut collapsed_pastes = hooks.use_ref(Vec::<CollapsedPaste>::new);
    let mut on_submit = props.on_submit.take();
    let mut on_mode_change = props.on_mode_change.take();
    let current_mode = props.mode;
    let has_focus = props.has_focus;
    let reset_dep = props.reset_nonce.map(|nonce| nonce.get()).unwrap_or(0);

    let is_empty = current.is_empty();
    hooks.use_effect(
        move || {
            if is_empty {
                stable_height.set(MIN_INPUT_LINES);
            } else {
                stable_height.set(computed_height.max(MIN_INPUT_LINES));
            }
        },
        (is_empty, computed_height),
    );

    hooks.use_effect(
        move || {
            if reset_dep == 0 {
                return;
            }
            stable_height.set(MIN_INPUT_LINES);
            cursor_offset.set(0);
            vertical_col_pref.set(None);
            pending_paste.write().take();
            collapsed_pastes.write().clear();
        },
        reset_dep,
    );

    let clear_generation = field_clear_generation.get();
    hooks.use_effect(
        move || {
            if clear_generation == 0 {
                return;
            }
            cursor_offset.set(0);
            vertical_col_pref.set(None);
            pending_paste.write().take();
            collapsed_pastes.write().clear();
        },
        clear_generation,
    );

    hooks.use_terminal_events(move |event| {
        if !has_focus {
            return;
        }

        let TerminalEvent::Key(KeyEvent {
            code, kind, modifiers, ..
        }) = event
        else {
            return;
        };

        if kind == KeyEventKind::Release {
            return;
        }

        let wrap_width = text_width.saturating_sub(1).max(1) as usize;
        let now = Instant::now();

        let mut text = value.read().clone();
        let mut cursor = cursor_offset.get().min(text.len());

        if is_press(kind) && should_finalize_paste(code) {
            finalize_pending_paste_input(
                &mut pending_paste.write(),
                &mut collapsed_pastes.write(),
                wrap_width,
                &mut paste_guard.write(),
                &mut value,
                &mut cursor_offset,
            );
            text = value.read().clone();
            cursor = cursor_offset.get().min(text.len());
        }

        if is_press(kind) && is_pasted_tab_key(code, modifiers) {
            if should_absorb_tab_as_paste(pending_paste.read().as_ref(), &paste_guard.read(), &text, now) {
                apply_pasted_char(
                    '\t',
                    &mut text,
                    &mut cursor,
                    &mut paste_guard.write(),
                    &mut pending_paste.write(),
                    &mut value,
                    &mut cursor_offset,
                    now,
                );
                vertical_col_pref.set(None);
                return;
            }
        }

        if is_mode_cycle_key(code, modifiers) && is_press(kind) {
            on_mode_change(current_mode.next());
            return;
        }

        if is_clear_key(code, modifiers) && is_press(kind) {
            if !text.is_empty() {
                value.set(String::new());
                cursor_offset.set(0);
                vertical_col_pref.set(None);
                stable_height.set(MIN_INPUT_LINES);
                field_clear_generation.set(field_clear_generation.get().wrapping_add(1));
                pending_paste.write().take();
                collapsed_pastes.write().clear();
            }
            return;
        }

        if let Some(action) = edit_action(code, modifiers)
            && is_press(kind)
        {
            let (next, new_cursor) = match action {
                EditAction::DeleteToLineStart => delete_to_line_start(&text, cursor),
                EditAction::DeleteToLineEnd => delete_to_line_end(&text, cursor),
                EditAction::DeleteWordBackward => delete_word_backward(&text, cursor),
                EditAction::DeleteWordForward => delete_word_forward(&text, cursor),
                EditAction::DeleteCharBackward => delete_char_backward(&text, cursor),
                EditAction::DeleteCharForward => delete_char_forward(&text, cursor),
                EditAction::LineStart => (text.clone(), line_start(&text, cursor)),
                EditAction::LineEnd => (text.clone(), line_end(&text, cursor)),
                EditAction::WordLeft => (text.clone(), word_left(&text, cursor)),
                EditAction::WordRight => (text.clone(), word_right(&text, cursor)),
                EditAction::CharLeft => (text.clone(), char_left(&text, cursor)),
                EditAction::CharRight => (text.clone(), char_right(&text, cursor)),
            };
            if next != text {
                value.set(next);
            }
            cursor_offset.set(new_cursor);
            vertical_col_pref.set(None);
            return;
        }

        if (is_newline_key(code, modifiers) || is_pasted_newline_char(code)) && is_press(kind) {
            vertical_col_pref.set(None);
            if is_pasted_newline_char(code) {
                apply_pasted_char(
                    '\n',
                    &mut text,
                    &mut cursor,
                    &mut paste_guard.write(),
                    &mut pending_paste.write(),
                    &mut value,
                    &mut cursor_offset,
                    now,
                );
                return;
            }
            let (next, new_cursor) = insert_newline_at_cursor(&text, cursor);
            if next != text {
                value.set(next);
            }
            cursor_offset.set(new_cursor);
            return;
        }

        if is_submit_key(code, modifiers) && is_press(kind) {
            if !text.is_empty() && !paste_guard.write().should_block_submit(now) {
                let submitted = expand_paste_markers(&text, &collapsed_pastes.read());
                on_submit(submitted);
                value.set(String::new());
                stable_height.set(MIN_INPUT_LINES);
                field_clear_generation.set(field_clear_generation.get().wrapping_add(1));
                cursor_offset.set(0);
                vertical_col_pref.set(None);
                pending_paste.write().take();
                collapsed_pastes.write().clear();
            }
            return;
        }

        if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
            return;
        }

        let mut clear_vertical_pref = true;

        match code {
            KeyCode::Char(ch) => {
                apply_pasted_char(
                    ch,
                    &mut text,
                    &mut cursor,
                    &mut paste_guard.write(),
                    &mut pending_paste.write(),
                    &mut value,
                    &mut cursor_offset,
                    now,
                );
            }
            KeyCode::Backspace => {
                let pastes = collapsed_pastes.read().clone();
                if let Some(range) = paste_block_range(&text, cursor.saturating_sub(1), &pastes) {
                    let (next, removed_idx) = remove_paste_block(&text, range.clone(), &pastes);
                    if let Some(idx) = removed_idx {
                        collapsed_pastes.write().remove(idx);
                    }
                    value.set(next);
                    cursor_offset.set(range.start);
                } else if cursor > 0 {
                    let (next, new_cursor) = delete_char_backward(&text, cursor);
                    value.set(next);
                    cursor_offset.set(new_cursor);
                }
            }
            KeyCode::Delete => {
                let pastes = collapsed_pastes.read().clone();
                if let Some(range) = paste_block_range(&text, cursor, &pastes) {
                    let (next, removed_idx) = remove_paste_block(&text, range.clone(), &pastes);
                    if let Some(idx) = removed_idx {
                        collapsed_pastes.write().remove(idx);
                    }
                    value.set(next);
                    cursor_offset.set(range.start);
                } else if cursor < text.len() {
                    let (next, new_cursor) = delete_char_forward(&text, cursor);
                    value.set(next);
                    cursor_offset.set(new_cursor);
                }
            }
            KeyCode::Left => {
                cursor = cursor_offset.get();
                let buffer = PromptBuffer::new(&text, wrap_width);
                cursor_offset.set(buffer.left_of_offset(cursor));
            }
            KeyCode::Right => {
                cursor = cursor_offset.get();
                let buffer = PromptBuffer::new(&text, wrap_width);
                cursor_offset.set(buffer.right_of_offset(cursor));
            }
            KeyCode::Up => {
                cursor = cursor_offset.get();
                let buffer = PromptBuffer::new(&text, wrap_width);
                clear_vertical_pref = false;
                if vertical_col_pref.get().is_none() {
                    let (_, col) = buffer.row_column_for_offset(cursor);
                    vertical_col_pref.set(Some(col));
                }
                cursor_offset.set(buffer.above_offset(cursor, vertical_col_pref.get()));
            }
            KeyCode::Down => {
                cursor = cursor_offset.get();
                let buffer = PromptBuffer::new(&text, wrap_width);
                clear_vertical_pref = false;
                if vertical_col_pref.get().is_none() {
                    let (_, col) = buffer.row_column_for_offset(cursor);
                    vertical_col_pref.set(Some(col));
                }
                cursor_offset.set(buffer.below_offset(cursor, vertical_col_pref.get()));
            }
            KeyCode::Home => {
                cursor = cursor_offset.get();
                let buffer = PromptBuffer::new(&text, wrap_width);
                cursor_offset.set(buffer.row_start_offset(cursor));
            }
            KeyCode::End => {
                cursor = cursor_offset.get();
                let buffer = PromptBuffer::new(&text, wrap_width);
                cursor_offset.set(buffer.row_end_offset(cursor));
            }
            _ => {
                clear_vertical_pref = false;
            }
        }

        if clear_vertical_pref {
            vertical_col_pref.set(None);
        }
    });

    element! {
        View(
            width: 100pct,
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
        ) {
            View(
                border_style: BorderStyle::Round,
                border_color: mode_color,
                width: 100pct,
                overflow: Overflow::Hidden,
                padding_left: 1,
                padding_right: 1,
            ) {
                View(
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    width: 100pct,
                    height: input_height,
                    overflow: Overflow::Hidden,
                ) {
                    View(width: 2, height: input_height, justify_content: JustifyContent::FlexStart) {
                        Text(color: theme.prompt_prefix, content: PROMPT_PREFIX)
                    }
                    View(flex_grow: 1.0, width: 100pct, height: input_height) {
                        PromptDisplay(
                            value: current,
                            cursor_offset: cursor_offset.get(),
                            height: input_height,
                            has_focus: props.has_focus,
                            theme,
                            collapsed_pastes: collapsed_pastes.read().clone(),
                            measured_width: Some(measured_width),
                        )
                    }
                }
            }
            View(
                width: 100pct,
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::FlexEnd,
            ) {
                Text(color: theme.muted, content: model_status)
                Text(color: mode_color, content: mode_label)
            }
        }
    }
}

fn is_press(kind: KeyEventKind) -> bool {
    kind == KeyEventKind::Press
}

fn is_pasted_newline_char(code: KeyCode) -> bool {
    matches!(code, KeyCode::Char('\n') | KeyCode::Char('\r'))
}

fn is_pasted_tab_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    modifiers.is_empty() && matches!(code, KeyCode::Tab | KeyCode::BackTab)
}

fn should_finalize_paste(code: KeyCode) -> bool {
    !matches!(code, KeyCode::Char(_) | KeyCode::Tab | KeyCode::BackTab)
}

fn should_absorb_tab_as_paste(
    pending: Option<&PendingPaste>,
    paste_guard: &PasteGuard,
    text: &str,
    now: Instant,
) -> bool {
    if paste_guard.is_in_burst(now) || paste_guard.is_paste_likely(now) {
        return true;
    }

    let Some(run) = pending else {
        return false;
    };

    if run.tab_follows_paste(now) {
        return true;
    }

    should_collapse_paste(run.slice(text))
}

fn apply_pasted_char(
    ch: char,
    text: &mut String,
    cursor: &mut usize,
    paste_guard: &mut PasteGuard,
    pending: &mut Option<PendingPaste>,
    value: &mut State<String>,
    cursor_offset: &mut State<usize>,
    now: Instant,
) {
    let cursor_before = *cursor;
    text.insert(*cursor, ch);
    *cursor += ch.len_utf8();
    paste_guard.record_change(value.read().len(), text.len(), now);
    match pending.as_mut() {
        Some(run) => run.extend(*cursor, now),
        None => *pending = Some(PendingPaste::new(cursor_before, *cursor, now)),
    }
    value.set(text.clone());
    cursor_offset.set(*cursor);
}

fn finalize_pending_paste_input(
    pending: &mut Option<PendingPaste>,
    pastes: &mut Vec<CollapsedPaste>,
    wrap_width: usize,
    paste_guard: &mut PasteGuard,
    value: &mut State<String>,
    cursor_offset: &mut State<usize>,
) {
    let Some(run) = pending.take() else {
        return;
    };

    let mut text = value.read().clone();
    let mut cursor = cursor_offset.get().min(text.len());
    let collapsed = should_collapse_paste(run.slice(&text));
    let before = text.clone();
    let cursor_before = cursor;
    *pending = finalize_pending_paste(Some(run), &mut text, &mut cursor, wrap_width, pastes, true);
    if collapsed {
        paste_guard.clear();
    }
    if text != before || cursor != cursor_before {
        value.set(text);
        cursor_offset.set(cursor);
    }
}

/// Returns `true` when the cursor sits on a trailing empty line created by a single newline.
fn is_on_trailing_empty_line(text: &str, cursor: usize) -> bool {
    let cursor = cursor.min(text.len());
    cursor > 0 && cursor == text.len() && text.as_bytes().get(cursor - 1) == Some(&b'\n')
}

/// Inserts a newline at the cursor and moves the cursor to the following line.
fn insert_newline_at_cursor(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    if is_on_trailing_empty_line(text, cursor) {
        return (text.to_string(), cursor);
    }

    let mut next = text.to_string();
    next.insert(cursor, '\n');
    (next, cursor + 1)
}

/// Visible height for the prompt field: grows with content up to [`MAX_INPUT_LINES`].
fn visual_line_count(value: &str, width: u16) -> u16 {
    if value.is_empty() {
        return MIN_INPUT_LINES;
    }

    let wrap_width = width.max(1).saturating_sub(1) as usize;
    let lines = logical_line_segments(value)
        .into_iter()
        .map(|line| wrapped_line_count(line, wrap_width))
        .sum::<u16>();

    lines.clamp(MIN_INPUT_LINES, MAX_INPUT_LINES)
}

/// Collapses consecutive trailing empty lines down to a single cursor row.
fn logical_line_segments(value: &str) -> Vec<&str> {
    let mut segments: Vec<&str> = value.split('\n').collect();
    while segments.len() > 1 && segments.last() == Some(&"") && segments[segments.len() - 2].is_empty() {
        segments.pop();
    }
    segments
}

fn wrapped_line_count(line: &str, wrap_width: usize) -> u16 {
    wrapped_row_count(line, wrap_width).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn insert_newline_at_cursor_from_middle_of_line() {
        let (text, cursor) = insert_newline_at_cursor("hello world", 5);
        assert_eq!(text, "hello\n world");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn insert_newline_at_cursor_when_cursor_already_at_end() {
        let (text, cursor) = insert_newline_at_cursor("hello", 5);
        assert_eq!(text, "hello\n");
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn insert_newline_on_trailing_empty_line_is_noop() {
        let (text, cursor) = insert_newline_at_cursor("a\n", 2);
        assert_eq!(text, "a\n");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn visual_line_count_defaults_to_one() {
        assert_eq!(visual_line_count("", 40), 1);
        assert_eq!(visual_line_count("hello", 40), 1);
    }

    #[test]
    fn visual_line_count_grows_with_newlines() {
        assert_eq!(visual_line_count("a\nb", 40), 2);
        assert_eq!(visual_line_count("a\nb\nc", 40), 3);
    }

    #[test]
    fn visual_line_count_collapses_double_trailing_newlines() {
        assert_eq!(visual_line_count("a\n\n", 40), 2);
    }

    #[test]
    fn visual_line_count_wraps_long_lines() {
        assert_eq!(visual_line_count(&"a".repeat(20), 10), 3);
    }

    #[test]
    fn absorbs_tab_during_continued_paste_burst() {
        let t0 = Instant::now();
        let pending = PendingPaste::new(0, 1, t0);
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1, t0);
        assert!(should_absorb_tab_as_paste(
            Some(&pending),
            &guard,
            "{",
            t0 + Duration::from_millis(5)
        ));
        assert!(should_absorb_tab_as_paste(
            Some(&pending),
            &guard,
            "{",
            t0 + Duration::from_millis(100)
        ));
    }

    #[test]
    fn absorbs_tab_after_paste_gap_when_content_is_multiline() {
        let t0 = Instant::now();
        let text = "{\n";
        let pending = PendingPaste::new(0, text.len(), t0);
        let guard = PasteGuard::default();
        let later = t0 + Duration::from_millis(200);
        assert!(should_absorb_tab_as_paste(Some(&pending), &guard, text, later));
    }

    #[test]
    fn absorbs_tab_when_paste_likely_after_rapid_inserts() {
        let t0 = Instant::now();
        let text = "{\"name\":";
        let pending = PendingPaste::new(0, text.len(), t0 + Duration::from_millis(150));
        let mut guard = PasteGuard::default();
        for len in 1..=text.len() {
            guard.record_change(len - 1, len, t0 + Duration::from_millis(len as u64));
        }
        let tab_at = t0 + Duration::from_millis(200);
        assert!(should_absorb_tab_as_paste(Some(&pending), &guard, text, tab_at));
    }

    #[test]
    fn manual_tab_after_pause_still_cycles_mode() {
        let t0 = Instant::now();
        let text = "hello";
        let pending = PendingPaste::new(0, text.len(), t0);
        let mut guard = PasteGuard::default();
        for len in 1..=text.len() {
            guard.record_change(len - 1, len, t0 + Duration::from_secs(1) * len as u32);
        }
        let tab_at = t0 + Duration::from_secs(10);
        assert!(!should_absorb_tab_as_paste(Some(&pending), &guard, text, tab_at));
    }

    #[test]
    fn pasted_newline_char_is_detected() {
        assert!(is_pasted_newline_char(KeyCode::Char('\n')));
        assert!(is_pasted_newline_char(KeyCode::Char('\r')));
        assert!(!is_pasted_newline_char(KeyCode::Char('a')));
    }

    #[test]
    fn visual_line_count_wraps_tab_indented_json() {
        let json = "{\n\t\"name\": \"elph\"\n}";
        assert!(visual_line_count(json, 20) >= 3);
    }

    #[test]
    fn visual_line_count_caps_at_five_lines() {
        let value = "line\n".repeat(7);
        assert_eq!(visual_line_count(value.trim_end(), 40), MAX_INPUT_LINES);
    }
}
