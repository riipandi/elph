use elph_tui::{
    ChatStreamState, Theme, TranscriptEntry, is_pinned_to_bottom, render_chat_stream, render_chat_stream_with_agent,
};
use slt::{Event, KeyCode, KeyModifiers, TestBackend};

#[test]
fn chat_stream_renders_messages() {
    let mut backend = TestBackend::new(60, 12);
    let mut state = ChatStreamState::with_messages(vec!["hello".to_string(), "world".to_string()]);
    let theme = Theme::dark();

    backend.render(|ui| {
        render_chat_stream(ui, &mut state, theme);
    });

    backend.assert_contains("hello");
    backend.assert_contains("world");
}

#[test]
fn streaming_pins_to_tail() {
    let mut backend = TestBackend::new(40, 8);
    let mut state = ChatStreamState::new();
    state.pin_to_tail();
    state.entries = (0..40)
        .map(|i| TranscriptEntry::user(format!("line {i}")))
        .chain([TranscriptEntry::assistant_streaming("streaming tail")])
        .collect();

    let theme = Theme::dark();
    for _ in 0..3 {
        backend.render(|ui| {
            render_chat_stream_with_agent(ui, &mut state, theme, true);
        });
    }

    assert!(state.auto_scroll);
    assert!(is_pinned_to_bottom(&state.scroll));
}

#[test]
fn wide_table_scrolls_horizontally() {
    let mut backend = TestBackend::new(24, 10);
    let wide = "| Col A | Col B | Col C | Col D | Col E | Col F |".to_string();
    let mut state = ChatStreamState::new();
    state.entries = vec![TranscriptEntry::assistant(format!(
        "{wide}\n|-------|-------|-------|-------|-------|-------|\n| one | two | three | four | five | six |"
    ))];

    let theme = Theme::dark();
    backend.render(|ui| {
        render_chat_stream(ui, &mut state, theme);
    });
    backend.render(|ui| {
        render_chat_stream(ui, &mut state, theme);
    });
    backend.assert_contains("Col A");

    backend.run_with_events(vec![Event::key_mod(KeyCode::Right, KeyModifiers::SHIFT)], |ui| {
        render_chat_stream(ui, &mut state, theme);
    });
    backend.render(|ui| {
        render_chat_stream(ui, &mut state, theme);
    });

    assert!(
        state.scroll_h.offset_x > 0,
        "shift+right should scroll wide content horizontally"
    );
}
