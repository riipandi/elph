use super::agent_mode::AgentMode;
use super::paste_guard::PasteGuard;
use super::prompt_edit::{
    char_left, char_right, delete_char_backward, delete_char_forward, delete_to_line_end, delete_to_line_start,
    delete_word_backward, delete_word_forward, line_end, line_start, word_left, word_right,
};
use super::prompt_keys::{EditAction, edit_action, is_newline_key, is_submit_key};
use crate::theme::Theme;
use iocraft::prelude::*;
use std::time::Instant;
use unicode_width::UnicodeWidthChar;

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

#[derive(Default, Props)]
struct PromptTextFieldProps {
    value: String,
    height: u16,
    has_focus: bool,
    handle: Option<Ref<TextInputHandle>>,
    on_change: HandlerMut<'static, String>,
    measured_width: Option<State<u16>>,
}

trait UseSize<'a> {
    fn use_size(&mut self) -> (u16, u16);
}

impl<'a> UseSize<'a> for Hooks<'a, '_> {
    fn use_size(&mut self) -> (u16, u16) {
        self.use_hook(UseSizeImpl::default).size
    }
}

#[derive(Default)]
struct UseSizeImpl {
    size: (u16, u16),
}

impl Hook for UseSizeImpl {
    fn pre_component_draw(&mut self, drawer: &mut ComponentDrawer) {
        let s = drawer.size();
        self.size = (s.width, s.height);
    }
}

#[component]
fn PromptTextField(mut hooks: Hooks, props: &mut PromptTextFieldProps) -> impl Into<AnyElement<'static>> {
    let (width, _) = hooks.use_size();
    let Some(mut measured_width) = props.measured_width else {
        panic!("measured_width is required");
    };

    hooks.use_effect(
        move || {
            if width > 0 && measured_width.get() != width {
                measured_width.set(width);
            }
        },
        width,
    );

    element! {
        View(width: 100pct, height: props.height) {
            TextInput(
                has_focus: props.has_focus,
                value: props.value.clone(),
                on_change: props.on_change.take(),
                multiline: true,
                handle: props.handle,
            )
        }
    }
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
    let mut input_handle = hooks.use_ref_default::<TextInputHandle>();
    let mut last_cursor = hooks.use_ref(|| 0usize);
    let computed_height = visual_line_count(&current, text_width);
    let input_height = stable_height.get().max(computed_height).min(MAX_INPUT_LINES);
    let mut append_at_end = hooks.use_ref(|| false);
    let mut cursor_tick = hooks.use_state(|| 0u32);
    let mut suppress_text_input = hooks.use_state(|| false);
    let mut field_clear_generation = hooks.use_state(|| 0u32);

    let mut on_change_ref = hooks.use_ref(HandlerMut::default);
    let mut paste_guard = hooks.use_ref(PasteGuard::default);
    let mut on_submit = props.on_submit.take();
    let has_focus = props.has_focus;
    let _cursor_sync = cursor_tick.get();
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
            last_cursor.set(0);
            append_at_end.set(false);
            suppress_text_input.set(false);
            input_handle.write().set_cursor_offset(0);
        },
        reset_dep,
    );

    let clear_generation = field_clear_generation.get();
    hooks.use_effect(
        move || {
            if clear_generation == 0 {
                return;
            }
            last_cursor.set(0);
            append_at_end.set(false);
            suppress_text_input.set(false);
            input_handle.write().set_cursor_offset(0);
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

        if let Some(action) = edit_action(code, modifiers)
            && is_press(kind)
        {
            let text = value.read().clone();
            let cursor = input_handle.read().cursor_offset().min(text.len());
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
                last_cursor.set(new_cursor);
                input_handle.write().set_cursor_offset(new_cursor);
                suppress_text_input.set(true);
            } else if new_cursor != cursor {
                last_cursor.set(new_cursor);
                input_handle.write().set_cursor_offset(new_cursor);
                cursor_tick.set(cursor_tick.get().wrapping_add(1));
            }
            return;
        }

        if is_newline_key(code, modifiers) && is_press(kind) {
            suppress_text_input.set(true);
            let text = value.read().clone();
            let cursor = newline_cursor(
                &text,
                input_handle.read().cursor_offset(),
                last_cursor.get(),
                append_at_end.get(),
            );
            append_at_end.set(false);
            let (next, new_cursor) = insert_newline_at_cursor(&text, cursor);
            if next != text {
                value.set(next);
                last_cursor.set(new_cursor);
                input_handle.write().set_cursor_offset(new_cursor);
                cursor_tick.set(cursor_tick.get().wrapping_add(1));
            }
            return;
        }

        let text = value.read().clone();
        if is_submit_key(code, modifiers)
            && is_press(kind)
            && !text.is_empty()
            && !paste_guard.write().should_block_submit(Instant::now())
        {
            on_submit(text);
            value.set(String::new());
            stable_height.set(MIN_INPUT_LINES);
            field_clear_generation.set(field_clear_generation.get().wrapping_add(1));
            last_cursor.set(0);
            append_at_end.set(false);
            input_handle.write().set_cursor_offset(0);
            cursor_tick.set(cursor_tick.get().wrapping_add(1));
        }
    });

    on_change_ref.set(HandlerMut::from(move |next: String| {
        let prev = value.read().clone();
        let cursor = input_handle.read().cursor_offset();
        if suppress_text_input.get() {
            suppress_text_input.set(false);
            if !(is_first_char_into_empty(&prev, &next) || is_single_char_growth(&prev, &next)) {
                return;
            }
        }
        if should_ignore_text_input_change(&prev, &next, cursor) {
            return;
        }
        paste_guard
            .write()
            .record_change(prev.len(), next.len(), Instant::now());
        let new_cursor = cursor_after_text_change(&prev, &next, cursor);
        append_at_end.set(next.len() > prev.len() && new_cursor == next.len());
        last_cursor.set(new_cursor);
        value.set(next);
        input_handle.write().set_cursor_offset(new_cursor);
    }));

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
                padding_left: 1,
                padding_right: 1,
            ) {
                View(
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    width: 100pct,
                    height: input_height,
                ) {
                    View(width: 2, height: input_height, justify_content: JustifyContent::FlexStart) {
                        Text(color: theme.prompt_prefix, content: PROMPT_PREFIX)
                    }
                    View(flex_grow: 1.0, width: 100pct, height: input_height) {
                        PromptTextField(
                            value: current,
                            height: input_height,
                            has_focus: props.has_focus,
                            handle: Some(input_handle),
                            on_change: move |next: String| on_change_ref.write()(next),
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

/// Cursor for newline insertion when [`TextInputHandle`] may lag after append-at-end typing.
fn newline_cursor(text: &str, handle_cursor: usize, last_cursor: usize, append_at_end: bool) -> usize {
    let handle_cursor = handle_cursor.min(text.len());
    if handle_cursor > 0 {
        return handle_cursor;
    }
    if append_at_end {
        return last_cursor.min(text.len());
    }
    handle_cursor
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

/// Computes the cursor offset after `TextInput` changes the value.
fn cursor_after_text_change(prev: &str, next: &str, prev_cursor: usize) -> usize {
    if next.len() > prev.len() {
        let inserted = next.len() - prev.len();
        prev_cursor.min(prev.len()) + inserted
    } else {
        prev_cursor.min(next.len())
    }
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
    if line.is_empty() {
        return 1;
    }

    let mut lines = 0_u16;
    let mut current_width = 0_usize;

    for ch in line.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if current_width > 0 && current_width + ch_width > wrap_width {
            lines += 1;
            current_width = 0;
        }
        current_width += ch_width;
    }

    lines + 1
}

/// Returns `true` when `TextInput` inserted a single `\n` from Enter (handled by `PromptInput`).
fn is_text_input_newline_insertion(prev: &str, next: &str, cursor: usize) -> bool {
    if next.len() != prev.len() + 1 {
        return false;
    }

    let cursor = cursor.min(prev.len());
    prev.get(..cursor) == next.get(..cursor)
        && next.as_bytes().get(cursor) == Some(&b'\n')
        && prev.get(cursor..) == next.get(cursor + 1..)
}

/// Returns `true` for the first typed character into a cleared prompt.
fn is_first_char_into_empty(prev: &str, next: &str) -> bool {
    prev.is_empty() && next.chars().count() == 1 && !next.starts_with('\n')
}

/// Returns `true` when `TextInput` appended exactly one character to `prev`.
fn is_single_char_growth(prev: &str, next: &str) -> bool {
    next.len() == prev.len() + 1 && next.starts_with(prev)
}

/// Returns `true` when a `TextInput` on_change should be dropped.
fn should_ignore_text_input_change(prev: &str, next: &str, cursor: usize) -> bool {
    if is_first_char_into_empty(prev, next) {
        return false;
    }
    if is_text_input_newline_insertion(prev, next, cursor) || is_redundant_newline_insertion(prev, next, cursor) {
        return true;
    }
    // Stale TextInput buffer replayed after submit cleared the field.
    prev.is_empty() && (next == "\n" || next.len() > 1)
}

/// Returns `true` when `TextInput` echoes a newline that `PromptInput` already handled.
fn is_redundant_newline_insertion(prev: &str, next: &str, cursor: usize) -> bool {
    if is_text_input_newline_insertion(prev, next, cursor) {
        return true;
    }

    let cursor = cursor.min(prev.len());
    next.len() == prev.len() + 1
        && next.as_bytes().get(cursor) == Some(&b'\n')
        && is_on_trailing_empty_line(prev, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn insert_newline_at_cursor_when_handle_is_synced() {
        let (text, cursor) = insert_newline_at_cursor("a", 1);
        assert_eq!(text, "a\n");
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn insert_newline_on_trailing_empty_line_is_noop() {
        let (text, cursor) = insert_newline_at_cursor("a\n", 2);
        assert_eq!(text, "a\n");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn newline_cursor_prefers_handle_when_synced() {
        assert_eq!(newline_cursor("hello", 3, 5, true), 3);
    }

    #[test]
    fn newline_cursor_falls_back_after_append_at_end() {
        assert_eq!(newline_cursor("a", 0, 1, true), 1);
    }

    #[test]
    fn newline_cursor_respects_navigation_to_start() {
        assert_eq!(newline_cursor("hello", 0, 5, false), 0);
    }

    #[test]
    fn insert_newline_at_cursor_on_last_line() {
        let (text, cursor) = insert_newline_at_cursor("line1\nline2", 11);
        assert_eq!(text, "line1\nline2\n");
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn insert_newline_at_cursor_on_earlier_line() {
        let (text, cursor) = insert_newline_at_cursor("line1\nline2", 3);
        assert_eq!(text, "lin\ne1\nline2");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn cursor_after_append_from_start() {
        assert_eq!(cursor_after_text_change("", "a", 0), 1);
    }

    #[test]
    fn cursor_after_append_from_end() {
        assert_eq!(cursor_after_text_change("hello", "hello!", 5), 6);
    }

    #[test]
    fn detects_text_input_newline_insertion() {
        assert!(is_text_input_newline_insertion("hello", "hello\n", 5));
        assert!(is_text_input_newline_insertion("a\nb", "a\n\nb", 2));
        assert!(!is_text_input_newline_insertion("hello", "hello!", 5));
        assert!(!is_text_input_newline_insertion("hello", "hello\n\n", 5));
    }

    #[test]
    fn detects_redundant_newline_on_trailing_empty_line() {
        assert!(is_redundant_newline_insertion("a\n", "a\n\n", 2));
    }

    #[test]
    fn first_char_into_empty_is_never_ignored() {
        assert!(is_first_char_into_empty("", "a"));
        assert!(!should_ignore_text_input_change("", "a", 0));
    }

    #[test]
    fn stale_text_input_echo_after_clear_is_ignored() {
        assert!(should_ignore_text_input_change("", "\n", 0));
        assert!(should_ignore_text_input_change("", "hello\n", 0));
    }

    #[test]
    fn suppress_text_input_does_not_block_first_char_after_clear() {
        assert!(!should_ignore_text_input_change("", "n", 0));
    }

    #[test]
    fn single_char_growth_after_newline_is_allowed() {
        assert!(is_single_char_growth("a\n", "a\nb"));
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
    fn visual_line_count_caps_at_five_lines() {
        let value = "line\n".repeat(7);
        assert_eq!(visual_line_count(value.trim_end(), 40), MAX_INPUT_LINES);
    }
}
