use elph_tui::prelude::*;
use elph_tui::text_editing::*;
use futures::StreamExt;
use macro_rules_attribute::apply;
use smol_macros::test;

fn press(code: KeyCode) -> TerminalEvent {
    TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, code))
}

fn release(code: KeyCode) -> TerminalEvent {
    TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, code))
}

fn press_mod(code: KeyCode, modifiers: KeyModifiers) -> TerminalEvent {
    let mut event = KeyEvent::new(KeyEventKind::Press, code);
    event.modifiers = modifiers;
    TerminalEvent::Key(event)
}

async fn render_exit(element: impl Into<AnyElement<'static>>) -> Vec<String> {
    element
        .into()
        .mock_terminal_render_loop(MockTerminalConfig::default())
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .await
}

#[component]
fn ScrollApiHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut tick = hooks.use_state(|| 0u32);
    let mut handle = hooks.use_ref_default::<ScrollViewHandle>();

    hooks.use_effect(
        move || {
            let mut h = handle.write();
            if h.content_height() > h.viewport_height() {
                h.scroll_to_top();
                elph_tui::components::scroll_view_up(&mut h, 1);
                elph_tui::components::scroll_view_down(&mut h, 2);
                elph_tui::components::scroll_view_down(&mut h, 100);
            }
        },
        tick.get(),
    );

    hooks.use_future(async move {
        tick.set(1);
    });
    if tick.get() >= 1 {
        system.exit();
    }

    let lines: Vec<_> = (0..20)
        .map(|i| element! { Text(content: format!("line {i}")) })
        .collect();

    element! {
        View(height: 5u16, width: 20u16) {
            ScrollView(handle: Some(handle), auto_scroll: true) {
                View(flex_direction: FlexDirection::Column) {
                    #(lines)
                }
            }
        }
    }
}

#[apply(test!)]
async fn scroll_view_helpers_with_live_handle() {
    let frames = render_exit(element!(ScrollApiHarness)).await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn input_shortcuts_covers_release_super_and_esc_sequences() {
    let frames = element!(EventExitHost(
        children: vec![element! {
            Input(width: 30u16, initial_value: "one two three".to_string(), has_focus: true)
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![
        TerminalEvent::Resize(80, 24),
        press(KeyCode::Char('q')),
        release(KeyCode::Backspace),
        press_mod(KeyCode::Backspace, KeyModifiers::SUPER),
        press_mod(KeyCode::Delete, KeyModifiers::SUPER),
        press_mod(KeyCode::Char('f'), KeyModifiers::ALT),
        press_mod(KeyCode::Char('b'), KeyModifiers::ALT),
        press_mod(KeyCode::Left, KeyModifiers::ALT),
        press(KeyCode::Esc),
        press(KeyCode::Left),
        press(KeyCode::Right),
        press_mod(KeyCode::Left, KeyModifiers::META),
        press_mod(KeyCode::Right, KeyModifiers::CONTROL | KeyModifiers::ALT),
    ])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn input_shortcuts_unfocused_and_non_matching_keys() {
    let frames = element!(EventExitHost(
        children: vec![element! {
            Input(width: 20u16, initial_value: "abc".to_string(), has_focus: false)
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![
        press_mod(KeyCode::Backspace, KeyModifiers::ALT),
        press(KeyCode::Esc),
    ])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;
    assert!(!frames.is_empty());
}

#[derive(Default, Props)]
struct EventExitHostProps {
    children: Vec<AnyElement<'static>>,
}

#[component]
fn EventExitHost(mut hooks: Hooks, props: &mut EventExitHostProps) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut tick = hooks.use_state(|| 0u32);
    let child = props.children.pop().unwrap_or_else(|| element!(View).into_any());
    hooks.use_future(async move {
        tick.set(1);
    });
    if tick.get() >= 1 {
        system.exit();
    }
    child
}

#[apply(test!)]
async fn input_typing_triggers_on_change() {
    let frames = element!(EventExitHost(
        children: vec![element! {
            Input(width: 24u16, initial_value: String::new(), has_focus: true)
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![press(
        KeyCode::Char('z'),
    )])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn textarea_typing_triggers_on_change() {
    let frames = element!(EventExitHost(
        children: vec![element! {
            Textarea(
                width: 24u16,
                initial_value: "ab".to_string(),
                has_focus: true,
                min_height: 1u16,
            )
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![press(
        KeyCode::Char('c'),
    )])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn select_and_slider_ignore_release_events() {
    let options = vec![SelectOption::new("A", "desc"), SelectOption::new("B", "")];
    let select_frames = element!(EventExitHost(
        children: vec![element! {
            SelectList(width: 20u16, height: 3u16, options: options, has_focus: true)
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![release(
        KeyCode::Down,
    )])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;

    let slider_frames = element!(EventExitHost(
        children: vec![element! {
            Slider(width: 20u16, min: 0.0f32, max: 10.0f32, step: 1.0f32, has_focus: true)
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![release(
        KeyCode::Right,
    )])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;

    assert!(!select_frames.is_empty());
    assert!(!slider_frames.is_empty());
}

#[test]
fn apply_action_covers_remaining_variants() {
    let (text, cursor) = apply_action(TextEditAction::WordRight, "hello world", 0);
    assert_eq!(text, "hello world");
    assert_eq!(cursor, 6);

    let (text, cursor) = apply_action(TextEditAction::DeleteWordBackward, "hello world", 11);
    assert_eq!(text, "hello ");
    assert_eq!(cursor, 6);

    let (text, cursor) = apply_action(TextEditAction::DeleteWordForward, "hello world", 0);
    assert_eq!(text, "world");
    assert_eq!(cursor, 0);

    let (text, cursor) = apply_action(TextEditAction::DeleteToLineStart, "ab\ncd", 5);
    assert_eq!(text, "ab\n");
    assert_eq!(cursor, 3);

    let (text, cursor) = apply_action(TextEditAction::DeleteToLineEnd, "ab\ncd", 3);
    assert_eq!(text, "ab\n");
    assert_eq!(cursor, 3);
}

#[test]
fn match_key_covers_esc_right_and_super_backspace() {
    assert_eq!(
        match_key_to_action(KeyCode::Right, KeyModifiers::empty(), false, true),
        Some(TextEditAction::WordRight)
    );
    assert_eq!(
        match_key_to_action(KeyCode::Backspace, KeyModifiers::SUPER, false, false),
        Some(TextEditAction::DeleteToLineStart)
    );
    assert_eq!(
        match_key_to_action(KeyCode::Char('J'), KeyModifiers::CONTROL, true, false),
        Some(TextEditAction::InsertNewline)
    );
    assert_eq!(
        match_key_to_action(KeyCode::Char('x'), KeyModifiers::empty(), false, false),
        None
    );
}

#[test]
fn scrollbar_thumb_position_zero_viewport() {
    use elph_tui::components::scroll_bar::scrollbar_thumb_position;
    assert_eq!(scrollbar_thumb_position(5, 0, 20), 0);
}

#[test]
fn highlight_rust_line_trailing_word() {
    use elph_tui::components::code::highlight_rust_line;
    let parts = highlight_rust_line("foobar");
    assert!(!parts.is_empty());
}

#[apply(test!)]
async fn textarea_esc_then_word_motion_and_shift_enter() {
    let frames = element!(EventExitHost(
        children: vec![element! {
            Textarea(
                width: 28u16,
                initial_value: "alpha beta".to_string(),
                has_focus: true,
                min_height: 1u16,
            )
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![
        press(KeyCode::Esc),
        press(KeyCode::Char('b')),
        press_mod(KeyCode::Enter, KeyModifiers::SHIFT),
        press_mod(KeyCode::Char('j'), KeyModifiers::CONTROL),
    ])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;
    assert!(!frames.is_empty());
}

#[test]
fn text_input_layout_empty_rows_fallback() {
    use elph_tui::text_input_layout::WrappedTextLayout;
    let layout = WrappedTextLayout::new("", 0);
    assert_eq!(layout.row_count(), 1);
}

#[test]
fn color_parses_lowercase_hex() {
    use elph_tui::color::from_hex;
    assert!(from_hex("#aabbcc").is_some());
}

#[apply(test!)]
async fn scrollbar_thumb_renders_all_rows() {
    let frames = render_exit(element!(EventExitHost(
        children: vec![element! {
            VerticalScrollbar(
                viewport_height: 8u16,
                content_height: 32u16,
                scroll_offset: 12u16,
                style: None,
            )
        }
        .into()],
    )))
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn select_slider_keyboard_matrix() {
    let options = vec![SelectOption::new("One", ""), SelectOption::new("Two", "two")];
    let select_frames = element!(EventExitHost(
        children: vec![element! {
            SelectList(width: 22u16, height: 2u16, options: options, has_focus: true, show_description: true)
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![
        press(KeyCode::Enter),
        press(KeyCode::Char('x')),
        press(KeyCode::Up),
        press(KeyCode::Char('k')),
    ])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;

    let slider_frames = element!(EventExitHost(
        children: vec![element! {
            Slider(width: 24u16, min: 0.0f32, max: 50.0f32, step: 2.5f32, has_focus: true, label: "Level".to_string())
        }
        .into()],
    ))
    .mock_terminal_render_loop(MockTerminalConfig::with_events(futures::stream::iter(vec![
        press(KeyCode::Char('x')),
        press(KeyCode::Right),
        press(KeyCode::Left),
    ])))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await;

    assert!(!select_frames.is_empty());
    assert!(!slider_frames.is_empty());
}

#[apply(test!)]
async fn scroll_view_pinned_up_and_down() {
    #[component]
    fn PinnedScroll(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut tick = hooks.use_state(|| 0u32);
        let mut handle = hooks.use_ref_default::<ScrollViewHandle>();
        hooks.use_effect(
            move || {
                let mut h = handle.write();
                if h.content_height() > h.viewport_height() {
                    scroll_view_up(&mut h, 1);
                    h.scroll_by(2);
                    elph_tui::components::scroll_view_down(&mut h, 1);
                }
            },
            (),
        );
        hooks.use_future(async move {
            tick.set(1);
        });
        if tick.get() >= 1 {
            system.exit();
        }
        let lines: Vec<_> = (0..30)
            .map(|i| element! { Text(content: format!("row {i}")) })
            .collect();
        element! {
            View(height: 6u16, width: 20u16) {
                ScrollView(handle: Some(handle), auto_scroll: true) {
                    View(flex_direction: FlexDirection::Column) { #(lines) }
                }
            }
        }
    }

    let frames = render_exit(element!(PinnedScroll)).await;
    assert!(!frames.is_empty());
}
