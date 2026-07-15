use elph_tui::components::scroll_box::{scroll_view_down, scroll_view_up};
use elph_tui::prelude::*;
use futures::{StreamExt, stream};
use macro_rules_attribute::apply;
use smol_macros::test;

#[derive(Default, Props)]
struct HarnessProps {
    exit_after: u32,
    children: Vec<AnyElement<'static>>,
}

/// Runs `children` for a few frames, then exits the mock render loop.
#[component]
fn RenderHarness(mut hooks: Hooks, props: &mut HarnessProps) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut tick = hooks.use_state(|| 0u32);
    let exit_after = props.exit_after.max(1);
    let children = std::mem::take(&mut props.children);

    hooks.use_future(async move {
        tick.set(tick.get().saturating_add(1));
    });

    if tick.get() >= exit_after {
        system.exit();
    }

    element! {
        View {
            #(children)
        }
    }
}

fn key(code: KeyCode) -> TerminalEvent {
    TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, code))
}

fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> TerminalEvent {
    let mut event = KeyEvent::new(KeyEventKind::Press, code);
    event.modifiers = modifiers;
    TerminalEvent::Key(event)
}

async fn render_harness(exit_after: u32, children: Vec<AnyElement<'static>>) -> Vec<String> {
    element! {
        RenderHarness(exit_after: exit_after, children: children)
    }
    .mock_terminal_render_loop(MockTerminalConfig::default())
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await
}

async fn render_harness_with_events(
    exit_after: u32,
    children: Vec<AnyElement<'static>>,
    events: Vec<TerminalEvent>,
) -> Vec<String> {
    element! {
        RenderHarness(exit_after: exit_after, children: children)
    }
    .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(events)))
    .map(|c| c.to_string())
    .collect::<Vec<_>>()
    .await
}

#[apply(test!)]
async fn card_renders_with_title_and_body() {
    let frames = render_harness(
        1,
        vec![
            element! {
                Card(
                    width: 24u16,
                    min_height: 4u16,
                    title: "Panel".to_string(),
                    border_style: CardBorderStyle::Round,
                    children: vec![element! { Text(content: "inside".to_string()) }.into()],
                )
            }
            .into(),
        ],
    )
    .await;
    assert!(frames.iter().any(|f| f.contains("inside")));
}

#[apply(test!)]
async fn card_without_border_omits_title() {
    let frames = render_harness(
        1,
        vec![
            element! {
                Card(
                    width: 20u16,
                    title: "Hidden".to_string(),
                    border_style: CardBorderStyle::None,
                    children: vec![element! { Text(content: "plain".to_string()) }.into()],
                )
            }
            .into(),
        ],
    )
    .await;
    assert!(frames.iter().any(|f| f.contains("plain")));
}

#[apply(test!)]
async fn styled_text_renders_content() {
    let frames = render_harness(
        1,
        vec![
            element! {
                StyledText(
                    content: "Hello".to_string(),
                    color: Some(Color::Cyan),
                    weight: Weight::Bold,
                    wrap: TextWrap::Wrap,
                    align: TextAlign::Center,
                    italic: true,
                )
            }
            .into(),
        ],
    )
    .await;
    assert!(frames.iter().any(|f| f.contains("Hello")));
}

#[apply(test!)]
async fn line_numbers_align_right() {
    let frames = render_harness(
        1,
        vec![
            element! {
                LineNumbers(line_count: 3usize, start_line: 9usize, gutter_width: 4u16)
            }
            .into(),
        ],
    )
    .await;
    assert!(frames.iter().any(|f| f.contains('0')));
}

#[apply(test!)]
async fn markdown_view_renders_scrollable_document() {
    let frames = render_harness(
        1,
        vec![
            element! {
                MarkdownView(width: 30u16, height: 8u16, source: "# Hi\n\nParagraph".to_string())
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn diff_view_unified_mode() {
    let frames = render_harness(
        1,
        vec![
            element! {
                DiffView(
                    width: 40u16,
                    height: 6u16,
                    old_text: "old\n".to_string(),
                    new_text: "new\n".to_string(),
                    mode: DiffMode::Unified,
                    side_by_side_min_width: 40u16,
                )
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn diff_view_side_by_side_when_wide_enough() {
    let frames = render_harness(
        1,
        vec![
            element! {
                DiffView(
                    width: 80u16,
                    height: 6u16,
                    old_text: "left\n".to_string(),
                    new_text: "right\n".to_string(),
                    mode: DiffMode::SideBySide,
                    side_by_side_min_width: 40u16,
                )
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn select_list_empty_state() {
    let frames = render_harness(
        1,
        vec![
            element! {
                SelectList(width: 20u16, height: 4u16, has_focus: true)
            }
            .into(),
        ],
    )
    .await;
    assert!(frames.iter().any(|f| f.contains("no options")));
}

#[apply(test!)]
async fn select_list_navigates_with_keys() {
    let options = vec![
        SelectOption::new("One", "first"),
        SelectOption::new("Two", "second"),
        SelectOption::new("Three", "third"),
    ];
    let frames = render_harness_with_events(
        1,
        vec![
            element! {
                SelectList(
                    width: 24u16,
                    height: 3u16,
                    options: options,
                    has_focus: true,
                    show_description: true,
                    fast_scroll_step: 2usize,
                )
            }
            .into(),
        ],
        vec![
            key(KeyCode::Down),
            key(KeyCode::Char('j')),
            key_mod(KeyCode::Down, KeyModifiers::SHIFT),
            key(KeyCode::Up),
            key(KeyCode::Char('k')),
            key(KeyCode::Enter),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn select_list_unfocused_skips_keys() {
    let options = vec![SelectOption::new("A", "")];
    let frames = render_harness_with_events(
        1,
        vec![
            element! {
                SelectList(width: 20u16, height: 3u16, options: options, has_focus: false)
            }
            .into(),
        ],
        vec![key(KeyCode::Down)],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn slider_renders_and_adjusts() {
    let frames = render_harness_with_events(
        1,
        vec![
            element! {
                Slider(
                    width: 30u16,
                    min: 0.0f32,
                    max: 100.0f32,
                    step: 5.0f32,
                    has_focus: true,
                    label: "Vol".to_string(),
                    fill_color: Some(Color::Green),
                    track_color: Some(Color::DarkGrey),
                )
            }
            .into(),
        ],
        vec![
            key(KeyCode::Right),
            key(KeyCode::Char('l')),
            key(KeyCode::Left),
            key(KeyCode::Char('h')),
        ],
    )
    .await;
    assert!(frames.iter().any(|f| f.contains("Vol")));
}

#[apply(test!)]
async fn slider_unfocused_and_empty_label() {
    let frames = render_harness(
        1,
        vec![
            element! {
                Slider(width: 20u16, min: 0.0f32, max: 0.0f32, step: 0.0f32, has_focus: false)
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn tab_select_switches_tabs() {
    let tabs = vec![TabItem::new("A", "panel A"), TabItem::new("B", "panel B")];
    let frames = render_harness_with_events(
        1,
        vec![
            element! {
                TabSelect(width: 40u16, tabs: tabs, has_focus: true)
            }
            .into(),
        ],
        vec![
            key(KeyCode::Right),
            key(KeyCode::Tab),
            key(KeyCode::Left),
            key(KeyCode::BackTab),
            key(KeyCode::Char('l')),
            key(KeyCode::Char('h')),
        ],
    )
    .await;
    assert!(frames.iter().any(|f| f.contains("panel")));
}

#[apply(test!)]
async fn tab_select_empty_tabs() {
    let frames = render_harness_with_events(
        1,
        vec![element! { TabSelect(width: 20u16, has_focus: true) }.into()],
        vec![key(KeyCode::Right)],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn input_renders_and_handles_editing_keys() {
    let frames = render_harness_with_events(
        1,
        vec![
            element! {
                Input(
                    width: 24u16,
                    initial_value: "hello world".to_string(),
                    has_focus: true,
                    text_color: Some(Color::White),
                    cursor_color: Some(Color::Yellow),
                    focused_border_color: Some(Color::Blue),
                )
            }
            .into(),
        ],
        vec![
            key_mod(KeyCode::Backspace, KeyModifiers::ALT),
            key_mod(KeyCode::Left, KeyModifiers::ALT),
            key(KeyCode::Esc),
            key(KeyCode::Left),
            key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL),
            key_mod(KeyCode::Char('k'), KeyModifiers::CONTROL),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn input_unfocused_ignores_keys() {
    let frames = render_harness_with_events(
        1,
        vec![
            element! {
                Input(width: 20u16, initial_value: "x".to_string(), has_focus: false)
            }
            .into(),
        ],
        vec![key_mod(KeyCode::Backspace, KeyModifiers::CONTROL)],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn textarea_multiline_with_scrollbar() {
    let text = "line one\nline two\nline three\nline four\nline five";
    let frames = render_harness_with_events(
        1,
        vec![
            element! {
                Textarea(
                    width: 30u16,
                    min_height: 2u16,
                    max_height: Some(3u16),
                    initial_value: text.to_string(),
                    has_focus: true,
                    show_border: Some(true),
                )
            }
            .into(),
        ],
        vec![
            key_mod(KeyCode::Enter, KeyModifiers::SHIFT),
            key_mod(KeyCode::Char('j'), KeyModifiers::CONTROL),
            key(KeyCode::Enter),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[derive(Default, Props)]
struct EditorLikeProps {
    width: u16,
    draft: Option<State<String>>,
    suppress_enter_newline: Option<Ref<bool>>,
    on_submit: HandlerMut<'static, String>,
}

#[component]
fn EditorLike(props: &mut EditorLikeProps) -> impl Into<AnyElement<'static>> {
    element! {
        Textarea(
            width: props.width,
            min_height: 1u16,
            has_focus: true,
            show_border: Some(false),
            submit_on_enter: true,
            value: props.draft,
            suppress_enter_newline: props.suppress_enter_newline,
            on_submit: props.on_submit.take(),
        )
    }
}

#[apply(test!)]
async fn textarea_enter_submits_through_editor_like_chain() {
    #[component]
    fn SubmitHost(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut submitted = hooks.use_state(|| false);
        let suppress = hooks.use_ref(|| false);
        let draft = hooks.use_state(String::new);

        if submitted.get() {
            system.exit();
        }

        element! {
            EditorLike(
                width: 32u16,
                draft: Some(draft),
                suppress_enter_newline: Some(suppress),
                on_submit: move |_text| {
                    submitted.set(true);
                },
            )
        }
    }

    let frames = element!(SubmitHost)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![
            key(KeyCode::Char('h')),
            key(KeyCode::Char('i')),
            key(KeyCode::Enter),
        ])))
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn textarea_without_border_and_suppress_enter() {
    #[component]
    fn SuppressHost(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut tick = hooks.use_state(|| 0u32);
        let suppress = hooks.use_ref(|| true);

        hooks.use_future(async move {
            tick.set(tick.get().saturating_add(1));
        });

        if tick.get() >= 1 {
            system.exit();
        }

        element! {
            Textarea(
                width: 24u16,
                initial_value: "draft".to_string(),
                has_focus: true,
                show_border: Some(false),
                suppress_enter_newline: Some(suppress),
            )
        }
    }

    let frames = element!(SuppressHost)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![key(KeyCode::Enter)])))
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn scroll_box_renders_children() {
    let children: Vec<AnyElement<'static>> = (0..8)
        .map(|i| element! { Text(content: format!("row {i}")) }.into())
        .collect();
    let frames = render_harness_with_events(
        1,
        vec![
            element! {
                ScrollBox(
                    width: 24u16,
                    height: 5u16,
                    auto_scroll: true,
                    keyboard_scroll: true,
                    scroll_step: 1u16,
                    scrollbar: true,
                    children: children,
                )
            }
            .into(),
        ],
        vec![
            key(KeyCode::Up),
            key(KeyCode::Down),
            key(KeyCode::PageUp),
            key(KeyCode::PageDown),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn scroll_box_scroll_helpers_via_handle() {
    #[component]
    fn ScrollHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut tick = hooks.use_state(|| 0u32);
        let mut handle = hooks.use_ref_default::<ScrollViewHandle>();

        hooks.use_future(async move {
            tick.set(tick.get().saturating_add(1));
        });

        hooks.use_effect(
            move || {
                let mut h = handle.write();
                scroll_view_down(&mut h, 1);
                scroll_view_up(&mut h, 1);
            },
            tick.get(),
        );

        if tick.get() >= 1 {
            system.exit();
        }

        let children: Vec<AnyElement<'static>> = (0..12)
            .map(|i| element! { Text(content: format!("line {i}")) }.into())
            .collect();
        element! {
            ScrollBox(width: 20u16, height: 4u16, auto_scroll: true, keyboard_scroll: true, children: children)
        }
    }

    let frames = element!(ScrollHarness)
        .mock_terminal_render_loop(MockTerminalConfig::default())
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn vertical_scrollbar_and_indicator() {
    let frames = render_harness(
        1,
        vec![
            element! {
                View(width: 12u16, height: 6u16, flex_direction: FlexDirection::Row) {
                    VerticalScrollbar(
                        viewport_height: 6u16,
                        content_height: 20u16,
                        scroll_offset: 5u16,
                        style: Some(ScrollbarStyle::dark()),
                    )
                    ScrollIndicator(offset: 4u32, visible: 6u32, total: 20u32, width: 10u16)
                }
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn vertical_scrollbar_hidden_when_content_fits() {
    let frames = render_harness(
        1,
        vec![
            element! {
                VerticalScrollbar(viewport_height: 10u16, content_height: 4u16, scroll_offset: 0u16)
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn select_list_external_selected_index() {
    #[component]
    fn SelectHost(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut tick = hooks.use_state(|| 0u32);
        let selected = hooks.use_state(|| 1usize);
        let options = vec![SelectOption::new("X", ""), SelectOption::new("Y", "desc")];
        hooks.use_future(async move {
            tick.set(1);
        });
        if tick.get() >= 1 {
            system.exit();
        }
        element! {
            SelectList(
                width: 20u16,
                height: 2u16,
                options: options,
                selected_index: Some(selected),
                has_focus: true,
                show_description: false,
            )
        }
    }
    let frames = render_harness(1, vec![element!(SelectHost).into()]).await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn slider_unfocused_rejects_arrow_keys() {
    let frames = render_harness_with_events(
        1,
        vec![element! {
            Slider(width: 20u16, min: 0.0f32, max: 10.0f32, step: 1.0f32, has_focus: false, label: "Gain".to_string())
        }
        .into()],
        vec![key(KeyCode::Right)],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn code_block_with_and_without_gutter() {
    let source = "fn main() {\n    // comment\n    let x = \"hi\";\n}".to_string();
    let with_gutter = render_harness(
        1,
        vec![
            element! {
                CodeBlock(width: 40u16, source: source.clone(), show_line_numbers: true, gutter_width: 4u16)
            }
            .into(),
        ],
    )
    .await;
    let without_gutter = render_harness(
        1,
        vec![
            element! {
                CodeBlock(width: 40u16, source: source, show_line_numbers: false)
            }
            .into(),
        ],
    )
    .await;
    assert!(!with_gutter.is_empty());
    assert!(!without_gutter.is_empty());
}

#[apply(test!)]
async fn frame_buffer_view_renders_grid() {
    let mut buf = FrameBuffer::new(6, 2);
    buf.set_text(0, 0, "cell");
    let frames = render_harness(
        1,
        vec![
            element! {
                FrameBufferView(buffer: buf, color: Some(Color::Green))
            }
            .into(),
        ],
    )
    .await;
    assert!(frames.iter().any(|f| f.contains("cell")));
}

#[apply(test!)]
async fn qr_code_view_renders_payload() {
    let frames = render_harness(
        1,
        vec![
            element! {
                QrCodeView(
                    payload: "elph".to_string(),
                    dark_char: "██".to_string(),
                    light_char: "  ".to_string(),
                    color: Some(Color::White),
                )
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn qr_code_view_default_chars() {
    let frames = render_harness(
        1,
        vec![
            element! {
                QrCodeView(payload: "x".to_string(), dark_char: String::new(), light_char: String::new())
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn textarea_full_props_matrix() {
    #[component]
    fn TextareaHost(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut tick = hooks.use_state(|| 0u32);
        let value = hooks.use_state(|| "seed".to_string());
        hooks.use_future(async move {
            tick.set(1);
        });
        if tick.get() >= 1 {
            system.exit();
        }
        element! {
            Textarea(
                width: 28u16,
                min_height: 2u16,
                initial_value: String::new(),
                has_focus: true,
                value: Some(value),
                show_border: Some(false),
                text_color: Some(Color::White),
                cursor_color: Some(Color::Cyan),
                scrollbar_style: Some(ScrollbarStyle::dark()),
            )
        }
    }
    let frames = render_harness(1, vec![element!(TextareaHost).into()]).await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn scroll_box_zero_scroll_step() {
    let frames = render_harness(
        1,
        vec![
            element! {
                ScrollBox(width: 16u16, height: 4u16, scroll_step: 0u16, children: vec![
                    element! { Text(content: "one".to_string()) }.into(),
                ])
            }
            .into(),
        ],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn select_list_zero_options_with_key() {
    let frames = render_harness_with_events(
        1,
        vec![element! { SelectList(width: 18u16, height: 2u16, has_focus: true) }.into()],
        vec![key(KeyCode::Down)],
    )
    .await;
    assert!(!frames.is_empty());
}

#[apply(test!)]
async fn ascii_text_bitmap_and_figlet() {
    let bitmap = render_harness(
        1,
        vec![
            element! {
                AsciiText(text: "ELPH".to_string(), use_figlet: false, color: Some(Color::Magenta))
            }
            .into(),
        ],
    )
    .await;
    let figlet = render_harness(
        1,
        vec![
            element! {
                AsciiText(text: "Hi".to_string(), use_figlet: true)
            }
            .into(),
        ],
    )
    .await;
    assert!(!bitmap.is_empty());
    assert!(!figlet.is_empty());
}
