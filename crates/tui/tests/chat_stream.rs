use elph_tui::ChatStream;
use futures::{StreamExt, stream};
use iocraft::prelude::*;

fn sample_messages(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("message {i}")).collect()
}

#[component]
fn Harness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut should_exit = hooks.use_state(|| false);

    hooks.use_terminal_events(move |event| {
        let TerminalEvent::Key(KeyEvent {
            code: KeyCode::Char('q'),
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return;
        };
        should_exit.set(true);
    });

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 30, height: 6, padding: 1) {
            ChatStream(
                messages: sample_messages(12),
                auto_scroll: false,
                line_scroll_step: 1u16,
                page_scroll_step: 0u16,
            )
        }
    }
}

#[tokio::test]
async fn chat_stream_scrolls_with_keyboard() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Down)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Down)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('q'))),
    ]);

    let output = element!(Harness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let last = output.last().expect("expected frames");
    assert!(last.contains("message 2"), "expected scrolled content, got: {last}");
    assert!(
        !last.contains("message 0"),
        "expected top message scrolled away, got: {last}"
    );
}

#[component]
fn FastScrollHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut should_exit = hooks.use_state(|| false);

    hooks.use_terminal_events(move |event| {
        let TerminalEvent::Key(KeyEvent {
            code: KeyCode::Char('q'),
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return;
        };
        should_exit.set(true);
    });

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 30, height: 6, padding: 1) {
            ChatStream(
                messages: sample_messages(12),
                auto_scroll: false,
                line_scroll_step: 3u16,
                page_scroll_step: 0u16,
            )
        }
    }
}

#[tokio::test]
async fn chat_stream_respects_line_scroll_step() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Down)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('q'))),
    ]);

    let output = element!(FastScrollHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let last = output.last().expect("expected frames");
    assert!(last.contains("message 3"), "expected 3-line step scroll, got: {last}");
    assert!(
        !last.contains("message 0"),
        "expected faster scroll away from top, got: {last}"
    );
}
