//! Terminal input for [`Textarea`] — one handler, direct state mutation.

mod paste_burst;
mod submit;
mod wire_edit;

use std::time::Instant;

use iocraft::prelude::*;

use super::state::TextareaState;
use crate::paste::PasteBurstState;
use crate::text_editing::{
    is_cursor_navigation_key, is_plain_submit_enter, is_slash_palette_capture_key, is_transcript_scroll_key,
};

/// Raw key bursts shorter than this are treated as fast typing, not paste streams.
const PASTE_STREAM_SUBMIT_BLOCK_MIN_CHARS: usize = 8;

/// Outcome of one terminal event.
#[derive(Debug, PartialEq, Eq)]
pub enum TextareaInputResult {
    /// Text or cursor changed — caller should bump render generation.
    Changed,
    /// Plain Enter submit — caller invokes `on_submit` with the draft.
    Submit(String),
    /// Event consumed without mutation.
    Consumed,
    /// Not handled.
    Ignored,
}

/// Per-frame input configuration.
pub struct TextareaInputContext<'a> {
    pub has_focus: bool,
    pub input_width: u16,
    pub submit_on_enter: bool,
    pub suppress_enter_newline: Option<Ref<bool>>,
    pub slash_palette_active: Option<Ref<bool>>,
    pub pending_esc: &'a mut bool,
    pub paste_burst: &'a mut PasteBurstState,
    pub last_key_at: &'a mut Option<Instant>,
    pub on_escape: &'a mut HandlerMut<'static, ()>,
}

/// Handle one terminal event against live editor state.
pub fn handle_textarea_terminal_event(
    event: TerminalEvent,
    state: &mut TextareaState,
    ctx: TextareaInputContext<'_>,
) -> TextareaInputResult {
    if !ctx.has_focus {
        return TextareaInputResult::Ignored;
    }

    if let TerminalEvent::Paste(data) = event {
        return paste_burst::handle_bracketed_paste(&data, state, ctx.paste_burst, ctx.last_key_at);
    }

    let TerminalEvent::Key(KeyEvent {
        code, kind, modifiers, ..
    }) = event
    else {
        return TextareaInputResult::Ignored;
    };

    if kind != KeyEventKind::Release
        && code == KeyCode::Esc
        && modifiers.is_empty()
        && !*ctx.pending_esc
        && !ctx.on_escape.is_default()
    {
        (ctx.on_escape)(());
        return TextareaInputResult::Consumed;
    }

    if ctx.slash_palette_active.is_some_and(|active| active.get())
        && is_slash_palette_capture_key(code, kind, modifiers)
    {
        return TextareaInputResult::Consumed;
    }

    let now = Instant::now();
    if kind != KeyEventKind::Release && code != KeyCode::Enter {
        if let Some(mut suppress) = ctx.suppress_enter_newline {
            suppress.set(false);
        }
        // Intentional typing after paste — allow Enter submit again.
        ctx.paste_burst.suppress_submit_until = None;
    }

    let in_burst = crate::text_editing::key_event_in_paste_burst(*ctx.last_key_at, now);
    let plain_submit_enter = is_plain_submit_enter(true, ctx.submit_on_enter, code, kind, modifiers);
    let raw_paste_blocks_submit = raw_paste_stream_blocks_submit(ctx.paste_burst, in_burst);
    // Long raw paste streams treat plain Enter as pasted newline, not submit — do not merge early.
    let continues_burst = in_burst
        && crate::paste::raw_burst_accepts_key(code, kind, modifiers, true)
        && (!plain_submit_enter || raw_paste_blocks_submit);
    let merged_idle_burst =
        ctx.paste_burst.active && !continues_burst && paste_burst::merge_idle_burst(ctx.paste_burst, state);

    if is_transcript_scroll_key(code, kind, modifiers) {
        return if merged_idle_burst {
            TextareaInputResult::Changed
        } else {
            TextareaInputResult::Consumed
        };
    }

    if ctx.paste_burst.suppress_raw_keys_until.is_some_and(|t| now < t)
        && kind != KeyEventKind::Release
        && is_paste_echo_key(code, modifiers)
    {
        return if merged_idle_burst {
            TextareaInputResult::Changed
        } else {
            TextareaInputResult::Consumed
        };
    }

    if plain_submit_enter
        && let Some(result) = submit::handle_enter_key(submit::EnterKey {
            code,
            kind,
            modifiers,
            now,
            state,
            submit_on_enter: ctx.submit_on_enter,
            suppress_enter_newline: ctx.suppress_enter_newline,
            raw_paste_burst_active: raw_paste_blocks_submit,
            suppress_submit_until: ctx.paste_burst.suppress_submit_until,
        })
    {
        return result;
    }

    if let Some(result) = paste_burst::handle_raw_burst_key(paste_burst::RawBurstKey {
        code,
        kind,
        modifiers,
        now,
        in_burst,
        state,
        burst: ctx.paste_burst,
        last_key_at: ctx.last_key_at,
    }) {
        return result;
    }

    if kind != KeyEventKind::Release && !is_cursor_navigation_key(code, kind, modifiers) {
        *ctx.last_key_at = Some(now);
    }

    if wire_edit::apply_wire_edit(code, kind, modifiers, state, ctx.input_width, ctx.pending_esc) {
        return TextareaInputResult::Changed;
    }

    if state.input_basic_key(code, kind, modifiers, ctx.input_width) {
        return TextareaInputResult::Changed;
    }

    if merged_idle_burst {
        return TextareaInputResult::Changed;
    }

    TextareaInputResult::Ignored
}

fn raw_paste_stream_blocks_submit(burst: &PasteBurstState, in_burst: bool) -> bool {
    burst.active && in_burst && burst.buffer.len() >= PASTE_STREAM_SUBMIT_BLOCK_MIN_CHARS
}

fn is_paste_echo_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META)
        && matches!(code, KeyCode::Char(_) | KeyCode::Tab)
}

#[cfg(test)]
mod tests {
    const ELPH_PASTE: &str = "**Elph** is a Rust workspace for AI agent applications: a coding agent CLI, shared agent runtime libraries, and terminal UI components. It is a port of the [pi](https://pi.dev) TypeScript ecosystem to Rust, with additional MCP (Model Context Protocol) support, WASM extensions, and an iocraft-based interactive TUI.";
    const ELPH_PASTE_WAKA: &str = "**Elph** is a Rust workspace for AI agent applications: a coding agent CLI, shared agent runtime libraries, and terminal UI components. It is a port of the [pi](https://pi.dev) TypeScript ecosystem to Rust, with additional MCP (Model Context Protocol) support, WASM extensions, wakakakkadkasdkask wakakakkadkasdkask wakakakkadkasdkask";
    const BULLET_PASTE: &str = "- **TOON Encoding** — Optional structured-data encoding for tool results (reduces token usage on tabular payloads).\n- **MCP** — Model Context Protocol client supporting stdio, streamable HTTP, and SSE transports with OAuth 2.1 and AES-256-GCM credential encryption.\n- **Agent** — `elph::agent` wraps `elph-agent`'s `AgentHarness` with session orchestration for the coding use case.\n- **AgentHarness** — Stateful, session-backed agent runner with hooks, compaction, branching, and plan mode.";

    use super::*;
    use std::thread;
    use std::time::Duration;

    fn key_press(code: KeyCode) -> TerminalEvent {
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, code))
    }

    fn shift_enter() -> TerminalEvent {
        let mut event = KeyEvent::new(KeyEventKind::Press, KeyCode::Enter);
        event.modifiers = KeyModifiers::SHIFT;
        TerminalEvent::Key(event)
    }

    fn test_context<'a>(
        esc: &'a mut bool,
        burst: &'a mut PasteBurstState,
        last: &'a mut Option<Instant>,
        submit_on_enter: bool,
        on_escape: &'a mut HandlerMut<'static, ()>,
    ) -> TextareaInputContext<'a> {
        TextareaInputContext {
            has_focus: true,
            input_width: 20,
            submit_on_enter,
            suppress_enter_newline: None,
            slash_palette_active: None,
            pending_esc: esc,
            paste_burst: burst,
            last_key_at: last,
            on_escape,
        }
    }

    #[test]
    fn plain_enter_submits_non_empty_draft() {
        let mut state = TextareaState::from_text("hi".into());
        state.cursor = 2;
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();
        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Enter), &mut state, ctx),
            TextareaInputResult::Submit("hi".into())
        );
    }

    #[test]
    fn typing_appends_characters() {
        let mut state = TextareaState::from_text("ab".into());
        state.cursor = 2;
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();
        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Char('c')), &mut state, ctx),
            TextareaInputResult::Changed
        );
        assert_eq!(state.text, "abc");
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn plain_escape_invokes_blur_handler() {
        let mut state = TextareaState::from_text("hi".into());
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let blurred = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let blurred_capture = std::sync::Arc::clone(&blurred);
        let mut on_escape = HandlerMut::from(move |_| {
            blurred_capture.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Esc), &mut state, ctx),
            TextareaInputResult::Consumed
        );
        assert!(blurred.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(state.text, "hi");
    }

    #[test]
    fn shift_enter_inserts_newline() {
        let mut state = TextareaState::from_text("hi".into());
        state.cursor = 2;
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();
        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(shift_enter(), &mut state, ctx),
            TextareaInputResult::Changed
        );
        assert_eq!(state.text, "hi\n");
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn rapid_paste_stream_coalesces_until_idle() {
        let paste = "# Elph — OpenWiki Quickstart";
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        for (i, ch) in paste.chars().enumerate() {
            let ctx = TextareaInputContext {
                has_focus: true,
                input_width: 40,
                submit_on_enter: false,
                suppress_enter_newline: None,
                slash_palette_active: None,
                pending_esc: &mut esc,
                paste_burst: &mut burst,
                last_key_at: &mut last,
                on_escape: &mut on_escape,
            };
            let result = handle_textarea_terminal_event(key_press(KeyCode::Char(ch)), &mut state, ctx);
            if i == 0 {
                assert_eq!(result, TextareaInputResult::Changed);
                assert_eq!(state.text, "#");
            } else {
                assert_eq!(result, TextareaInputResult::Consumed, "char {i}: {ch}");
            }
        }

        thread::sleep(Duration::from_millis(110));
        let ctx = TextareaInputContext {
            has_focus: true,
            input_width: 40,
            submit_on_enter: false,
            suppress_enter_newline: None,
            slash_palette_active: None,
            pending_esc: &mut esc,
            paste_burst: &mut burst,
            last_key_at: &mut last,
            on_escape: &mut on_escape,
        };
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Char('!')), &mut state, ctx),
            TextareaInputResult::Changed
        );
        assert_eq!(state.text, format!("{paste}!"));
        assert!(!burst.active);
    }

    fn shift_key_press(code: KeyCode) -> TerminalEvent {
        let mut event = KeyEvent::new(KeyEventKind::Press, code);
        event.modifiers = KeyModifiers::SHIFT;
        TerminalEvent::Key(event)
    }

    #[test]
    fn shift_arrow_scroll_does_not_move_textarea_cursor() {
        let mut state = TextareaState::from_text("alpha\nbeta".into());
        state.cursor = 7;
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();
        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(shift_key_press(KeyCode::Up), &mut state, ctx),
            TextareaInputResult::Consumed
        );
        assert_eq!(state.cursor, 7);
    }

    #[test]
    fn arrow_moves_cursor() {
        let mut state = TextareaState::from_text("hi".into());
        state.cursor = 2;
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();
        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Left), &mut state, ctx),
            TextareaInputResult::Changed
        );
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn bracketed_paste_leaves_cursor_at_eof() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();
        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(TerminalEvent::Paste(ELPH_PASTE.into()), &mut state, ctx),
            TextareaInputResult::Changed
        );
        assert_eq!(state.text, ELPH_PASTE);
        assert_eq!(state.cursor, ELPH_PASTE.len());
    }

    #[test]
    fn bracketed_paste_ignores_terminal_echo_keys() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        handle_textarea_terminal_event(TerminalEvent::Paste(ELPH_PASTE.into()), &mut state, ctx);

        for ch in ELPH_PASTE.chars() {
            let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
            assert_eq!(
                handle_textarea_terminal_event(key_press(KeyCode::Char(ch)), &mut state, ctx),
                TextareaInputResult::Consumed
            );
        }
        assert_eq!(state.text, ELPH_PASTE);
        assert_eq!(state.cursor, ELPH_PASTE.len());
    }

    #[test]
    fn long_raw_paste_stream_cursor_at_eof() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        for ch in ELPH_PASTE.chars() {
            let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
            handle_textarea_terminal_event(key_press(KeyCode::Char(ch)), &mut state, ctx);
        }
        thread::sleep(Duration::from_millis(110));
        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        handle_textarea_terminal_event(shift_key_press(KeyCode::Up), &mut state, ctx);
        assert_eq!(state.text, ELPH_PASTE);
        assert_eq!(state.cursor, ELPH_PASTE.len());
    }

    #[test]
    fn long_raw_paste_with_gaps_cursor_at_eof() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        for (i, ch) in ELPH_PASTE.chars().enumerate() {
            if i > 0 && i % 37 == 0 {
                thread::sleep(Duration::from_millis(110));
            }
            let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
            handle_textarea_terminal_event(key_press(KeyCode::Char(ch)), &mut state, ctx);
        }
        thread::sleep(Duration::from_millis(110));
        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        handle_textarea_terminal_event(shift_key_press(KeyCode::Up), &mut state, ctx);
        assert_eq!(state.text, ELPH_PASTE);
        assert_eq!(state.cursor, ELPH_PASTE.len());
    }

    fn assert_bracketed_paste_cursor(paste: &str) {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();
        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        handle_textarea_terminal_event(TerminalEvent::Paste(paste.into()), &mut state, ctx);
        assert_eq!(state.text, paste, "text mismatch for paste len {}", paste.len());
        assert_eq!(state.cursor, paste.len(), "cursor should be at EOF");
    }

    #[test]
    fn bracketed_multiline_bullet_paste_cursor_at_eof() {
        assert_bracketed_paste_cursor(BULLET_PASTE);
    }

    #[test]
    fn bracketed_elph_waka_paste_cursor_at_eof() {
        assert_bracketed_paste_cursor(ELPH_PASTE_WAKA);
    }

    #[test]
    fn rapid_typing_then_enter_submits() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        for ch in "hello".chars() {
            let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
            handle_textarea_terminal_event(key_press(KeyCode::Char(ch)), &mut state, ctx);
        }

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Enter), &mut state, ctx),
            TextareaInputResult::Submit("hello".into())
        );
    }

    #[test]
    fn idle_burst_after_typing_pause_then_enter_submits() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        for ch in "hello".chars() {
            let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
            handle_textarea_terminal_event(key_press(KeyCode::Char(ch)), &mut state, ctx);
        }

        thread::sleep(Duration::from_millis(110));

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Enter), &mut state, ctx),
            TextareaInputResult::Submit("hello".into())
        );
    }

    #[test]
    fn quick_enter_after_single_char_submits() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        handle_textarea_terminal_event(key_press(KeyCode::Char('x')), &mut state, ctx);

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Enter), &mut state, ctx),
            TextareaInputResult::Submit("x".into())
        );
    }

    #[test]
    fn bracketed_paste_enter_after_guard_submits() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        handle_textarea_terminal_event(TerminalEvent::Paste(ELPH_PASTE.into()), &mut state, ctx);

        thread::sleep(Duration::from_millis(160));

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Enter), &mut state, ctx),
            TextareaInputResult::Submit(ELPH_PASTE.into())
        );
    }

    #[test]
    fn long_raw_paste_trailing_enter_does_not_submit() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        for ch in ELPH_PASTE.chars() {
            let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
            handle_textarea_terminal_event(key_press(KeyCode::Char(ch)), &mut state, ctx);
        }

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Enter), &mut state, ctx),
            TextareaInputResult::Consumed
        );
        assert!(burst.active);
        assert!(burst.buffer.ends_with('\n'));

        thread::sleep(Duration::from_millis(110));
        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        handle_textarea_terminal_event(shift_key_press(KeyCode::Up), &mut state, ctx);
        assert_eq!(state.text, format!("{ELPH_PASTE}\n"));
        assert_eq!(state.cursor, ELPH_PASTE.len() + 1);
    }

    #[test]
    fn bracketed_paste_trailing_enter_does_not_submit() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        handle_textarea_terminal_event(TerminalEvent::Paste(ELPH_PASTE.into()), &mut state, ctx);

        let ctx = test_context(&mut esc, &mut burst, &mut last, true, &mut on_escape);
        assert_eq!(
            handle_textarea_terminal_event(key_press(KeyCode::Enter), &mut state, ctx),
            TextareaInputResult::Consumed
        );
        assert_eq!(state.text, ELPH_PASTE);
        assert_eq!(state.cursor, ELPH_PASTE.len());
    }

    #[test]
    fn bracketed_paste_delayed_echo_does_not_corrupt_text() {
        let mut state = TextareaState::default();
        let mut esc = false;
        let mut burst = PasteBurstState::default();
        let mut last = None;
        let mut on_escape = HandlerMut::default();

        let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
        handle_textarea_terminal_event(TerminalEvent::Paste(ELPH_PASTE.into()), &mut state, ctx);

        // Simulate terminal echo starting after the old 150ms guard but within scaled guard.
        thread::sleep(Duration::from_millis(220));

        for ch in ELPH_PASTE.chars() {
            let ctx = test_context(&mut esc, &mut burst, &mut last, false, &mut on_escape);
            assert_eq!(
                handle_textarea_terminal_event(key_press(KeyCode::Char(ch)), &mut state, ctx),
                TextareaInputResult::Consumed
            );
        }
        assert_eq!(state.text, ELPH_PASTE);
        assert_eq!(state.cursor, ELPH_PASTE.len());
    }
}
