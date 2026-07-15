//! TUI demo - basic chat layout
//!
//! Mirrors `elph/src/tui.rs` shell layout.
//! Color reference: https://www.ditig.com/256-colors-cheat-sheet
//!
//! ```bash
//! cargo run -p elph-tui --example chat_layout
//! ```

use anyhow::Result;
use chrono::Local;
use elph_tui::{
    Textarea, active_sticky_user_message_index, layout_sticky_header, layout_transcript_rows, scroll_view_down,
    scroll_view_up, transcript_text_width,
};
use iocraft::prelude::*;
use std::time::Duration;

/// Agent permission / interaction mode (mirrors `elph::types::AgentMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AgentMode {
    #[default]
    Build,
    Plan,
    Ask,
    Brave,
}

impl AgentMode {
    fn footer_label(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Plan => "plan",
            Self::Ask => "ask",
            Self::Brave => "brave",
        }
    }

    fn label_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Plan => (6, 182, 212),
            Self::Ask => (59, 130, 246),
            Self::Brave => (239, 68, 68),
            Self::Build => (107, 114, 128),
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Build => Self::Plan,
            Self::Plan => Self::Ask,
            Self::Ask => Self::Brave,
            Self::Brave => Self::Build,
        }
    }
}

/// Reasoning / thinking level (mirrors `elph::types::ThinkingLevel`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ThinkingLevel {
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ThinkingLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Off => Self::Minimal,
            Self::Minimal => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Xhigh,
            Self::Xhigh => Self::Off,
        }
    }
}

fn rgb_color((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb { r, g, b }
}

const LOREM_IPSUM: &str = "Lorem ipsum odor amet, consectetuer adipiscing elit. \
Lobortis hendrerit nec ipsum dapibus quam. Donec malesuada tincidunt elementum \
mollis vehicula quisque purus. Est volutpat integer, donec sagittis placerat \
fermentum phasellus ipsum sollicitudin. Tempus laoreet ad tempus aptent proin \
per donec lectus. Quisque auctor urna; phasellus urna tortor ligula. Class \
pharetra bibendum tristique, quisque consectetur placerat potenti. Imperdiet ut \
torquent vestibulum eleifend bibendum et. Dictumst vulputate interdum iaculis \
at conubia venenatis.";

#[derive(Clone)]
struct TranscriptMessage {
    content: String,
    style: TranscriptStyle,
}

#[derive(Clone, Copy)]
enum TranscriptStyle {
    Dim,
    User,
    Assistant,
    Error,
    PlainDim,
    PlainUser,
    Tool,
}

impl TranscriptStyle {
    fn is_user(self) -> bool {
        matches!(self, Self::User | Self::PlainUser)
    }

    fn bubble_padding_rows(self) -> u16 {
        self.padding().saturating_mul(2)
    }

    fn text_color(self) -> Color {
        match self {
            Self::Dim | Self::PlainDim => Color::DarkGrey,
            Self::User | Self::PlainUser | Self::Tool => Color::White,
            Self::Assistant => Color::DarkGreen,
            Self::Error => Color::DarkRed,
        }
    }

    fn background_color(self) -> Color {
        match self {
            Self::Dim | Self::User | Self::Assistant | Self::Error => Color::Rgb { r: 48, g: 48, b: 48 },
            Self::PlainDim | Self::PlainUser => Color::Reset,
            Self::Tool => Color::Rgb { r: 0, g: 95, b: 175 },
        }
    }

    fn padding(self) -> u16 {
        match self {
            Self::PlainDim | Self::PlainUser => 0,
            _ => 1,
        }
    }
}

fn seed_transcript_messages() -> Vec<TranscriptMessage> {
    vec![
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::Dim,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::User,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::Assistant,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::Error,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::PlainDim,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::PlainUser,
        },
        TranscriptMessage {
            content: "read_file : /U/a/b/c/d/project-dir/examples/chat_layout.rs".to_string(),
            style: TranscriptStyle::Tool,
        },
    ]
}

fn transcript_message_bubble(screen_width: u16, message: &TranscriptMessage) -> AnyElement<'static> {
    let style = message.style;
    element! {
        View(
            width: screen_width - 3,
            background_color: style.background_color(),
            margin_bottom: 0,
            padding: style.padding(),
        ) {
            Text(color: style.text_color(), wrap: TextWrap::Wrap, content: message.content.clone())
        }
    }
    .into()
}

fn transcript_sticky_bubble(
    screen_width: u16,
    message: &TranscriptMessage,
    display_content: &str,
) -> AnyElement<'static> {
    let style = message.style;
    element! {
        View(
            width: screen_width - 3,
            background_color: style.background_color(),
            margin_bottom: 0,
            padding: style.padding(),
        ) {
            Text(color: style.text_color(), wrap: TextWrap::NoWrap, content: display_content.to_string())
        }
    }
    .into()
}

fn transcript_sticky_overlay(
    screen_width: u16,
    height: u16,
    message: &TranscriptMessage,
    display_content: &str,
    truncated: bool,
) -> AnyElement<'static> {
    let bubble = transcript_sticky_bubble(screen_width, message, display_content);
    element! {
        View(
            position: Position::Absolute,
            top: 0,
            left: 0,
            width: screen_width,
            height: height,
            overflow: Overflow::Hidden,
            background_color: Color::Reset,
            padding_left: 1,
            padding_right: 1,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
        ) {
            #(bubble)
            #(if truncated {
                Some(element! {
                    Text(
                        color: Color::DarkGrey,
                        wrap: TextWrap::NoWrap,
                        content: "  ⋯ full prompt in transcript",
                    )
                })
            } else {
                None
            })
        }
    }
    .into()
}

fn is_quit_command(text: &str) -> bool {
    matches!(text.trim(), ":q" | ":q!")
}

#[derive(Default, Props)]
struct HeaderProps {
    screen_width: u16,
    session_label: String,
}

#[component]
fn Header(props: &HeaderProps) -> impl Into<AnyElement<'static>> {
    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            background_color: Color::Reset,
            border_style: BorderStyle::Single,
            border_edges: Edges::Top,
            border_color: Color::Rgb { r: (88), g: (88), b: (88) },
            position: Position::Relative,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
            margin_bottom: 0,
        ) {
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: props.session_label.clone())
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: "$0.00 | 0k | 0.0% (262k)")
        }
    }
}

#[derive(Clone, Default, Props)]
struct TranscriptPanelProps {
    screen_width: u16,
    messages: Vec<TranscriptMessage>,
    sticky_scroll: bool,
}

const TRANSCRIPT_SCROLL_STEP: i32 = 2;
const STICKY_MIN_SCROLL_ROWS: u16 = 3;

#[component]
fn TranscriptPanel(props: &TranscriptPanelProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let scroll_handle = hooks.use_ref_default::<ScrollViewHandle>();
    let mut panel_viewport = hooks.use_ref(|| 0u16);
    let handle = scroll_handle.read();
    let scroll_viewport = handle.viewport_height().max(1);
    let min_content_height = scroll_viewport;
    let row_layouts = layout_transcript_rows(
        &props.messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
        transcript_text_width(props.screen_width),
        1,
    );
    let is_user: Vec<_> = props.messages.iter().map(|m| m.style.is_user()).collect();
    let sticky_idx = props
        .sticky_scroll
        .then(|| {
            active_sticky_user_message_index(
                &row_layouts,
                &is_user,
                handle.scroll_offset(),
                handle.is_auto_scroll_pinned(),
            )
        })
        .flatten();
    let panel_height = {
        let mut outer = panel_viewport.write();
        if sticky_idx.is_none() {
            *outer = scroll_viewport;
            scroll_viewport
        } else {
            (*outer).max(scroll_viewport).max(1)
        }
    };
    let sticky_header = sticky_idx.and_then(|idx| {
        layout_sticky_header(
            &props.messages[idx].content,
            transcript_text_width(props.screen_width),
            props.messages[idx].style.bubble_padding_rows(),
            panel_height,
            STICKY_MIN_SCROLL_ROWS,
        )
    });
    let sticky_rows = sticky_header.as_ref().map(|h| h.height).unwrap_or(0);
    let bubbles: Vec<_> = props
        .messages
        .iter()
        .map(|message| transcript_message_bubble(props.screen_width, message))
        .collect();

    hooks.use_terminal_events({
        let mut scroll_handle = scroll_handle;
        move |event| {
            let TerminalEvent::Key(KeyEvent {
                code, kind, modifiers, ..
            }) = event
            else {
                return;
            };
            if kind == KeyEventKind::Release || !modifiers.contains(KeyModifiers::SHIFT) {
                return;
            }
            match code {
                KeyCode::Up => scroll_view_up(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP),
                KeyCode::Down => scroll_view_down(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP),
                _ => {}
            }
        }
    });

    element! {
        View(
            width: props.screen_width,
            flex_grow: 1f32,
            flex_shrink: 1f32,
            min_height: 0,
            overflow: Overflow::Hidden,
            border_style: BorderStyle::Single,
            border_edges: Edges::Top,
            border_color: Color::Rgb { r: (88), g: (88), b: (88) },
            margin_bottom: 1,
        ) {
            View(
                width: 100pct,
                height: 100pct,
                position: Position::Relative,
                overflow: Overflow::Hidden,
            ) {
                View(
                    position: Position::Absolute,
                    top: sticky_rows as i32,
                    left: 0,
                    width: 100pct,
                    bottom: 0,
                    overflow: Overflow::Hidden,
                ) {
                    ScrollView(
                        handle: Some(scroll_handle),
                        scroll_step: TRANSCRIPT_SCROLL_STEP as u16,
                        scrollbar: true,
                        scrollbar_thumb_color: Color::Rgb { r: (88), g: (88), b: (88) },
                        scrollbar_track_color: Color::Rgb { r: (48), g: (48), b: (48) },
                        keyboard_scroll: Some(false),
                        auto_scroll: true,
                    ) {
                        View(
                            width: props.screen_width,
                            min_height: min_content_height,
                            background_color: Color::Reset,
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::End,
                            align_items: AlignItems::Baseline,
                            padding_top: 0,
                            padding_bottom: 0,
                            padding_left: 1,
                            padding_right: 1,
                            gap: 1,
                        ) {
                            #(bubbles)
                        }
                    }
                }
                #(if let (Some(idx), Some(header)) = (sticky_idx, sticky_header.as_ref()) {
                    Some(transcript_sticky_overlay(
                        props.screen_width,
                        header.height,
                        &props.messages[idx],
                        &header.display_text,
                        header.truncated,
                    ))
                } else {
                    None
                })
            }
        }
    }
}

#[derive(Default, Props)]
struct StatusRowProps {
    screen_width: u16,
    time_label: String,
}

#[component]
fn StatusRow(props: &StatusRowProps) -> impl Into<AnyElement<'static>> {
    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
        ) {
            View(
                width: props.screen_width / 2,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Start,
                padding: 0,
            ) {
                Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: props.time_label.clone())
            }
            View(
                width: props.screen_width / 2,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::End,
                padding: 0,
            ) {
                Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: "Enter send · Shift+Enter/Ctrl+J newline · Shift+↑↓ scroll · Ctrl+D quit")
            }
        }
    }
}

fn editor_max_height(screen_height: u16) -> u16 {
    (screen_height / 4).clamp(4, 12)
}

#[derive(Clone, Copy, Default, Props)]
struct EditorProps {
    screen_width: u16,
    screen_height: u16,
    agent_mode: AgentMode,
    draft: Option<State<String>>,
    suppress_enter_newline: Option<Ref<bool>>,
}

#[component]
fn Editor(props: &EditorProps) -> impl Into<AnyElement<'static>> {
    let label_color = rgb_color(props.agent_mode.label_rgb());

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            border_style: BorderStyle::Round,
            border_color: Color::Rgb { r: (108), g: (108), b: (108) },
            position: Position::Relative,
            align_items: AlignItems::FlexStart,
            margin_bottom: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 1,
            padding_right: 1,
            overflow: Overflow::Hidden,
        ) {
            Textarea(
                width: props.screen_width.saturating_sub(2),
                min_height: 1u16,
                max_height: Some(editor_max_height(props.screen_height)),
                show_border: Some(false),
                has_focus: true,
                value: props.draft,
                suppress_enter_newline: props.suppress_enter_newline,
                text_color: Some(Color::Grey),
                cursor_color: Some(Color::DarkGrey),
            )
            View(
                position: Position::Absolute,
                right: 1,
                bottom: 0,
                margin_bottom: -1,
                background_color: Color::Reset,
            ) {
                Text(
                    color: label_color,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                    content: format!(" {} ", props.agent_mode.footer_label()),
                )
            }
        }
    }
}

#[derive(Clone, Default, Props)]
struct FooterLeftProps {
    width: u16,
    project_label: String,
}

#[component]
fn FooterLeft(props: &FooterLeftProps) -> impl Into<AnyElement<'static>> {
    element! {
        View(
            width: props.width,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Start,
            padding: 0,
        ) {
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: props.project_label.clone())
        }
    }
}

#[derive(Clone, Default, Props)]
struct FooterRightProps {
    width: u16,
    model_label: String,
    thinking_level: ThinkingLevel,
}

#[component]
fn FooterRight(props: &FooterRightProps) -> impl Into<AnyElement<'static>> {
    let footer_right = format!("IMG | {} | {}", props.model_label, props.thinking_level.label());

    element! {
        View(
            width: props.width,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::End,
            padding: 0,
        ) {
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: footer_right)
        }
    }
}

#[derive(Clone, Default, Props)]
struct FooterProps {
    screen_width: u16,
    project_label: String,
    model_label: String,
    thinking_level: ThinkingLevel,
}

#[component]
fn Footer(props: &FooterProps) -> impl Into<AnyElement<'static>> {
    let half = props.screen_width / 2;

    element! {
        View(
            width: props.screen_width,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
        ) {
            FooterLeft(width: half, project_label: props.project_label.clone())
            FooterRight(
                width: half,
                model_label: props.model_label.clone(),
                thinking_level: props.thinking_level,
            )
        }
    }
}

#[derive(Clone, Default, Props)]
struct PromptChromeProps {
    screen_width: u16,
    screen_height: u16,
    agent_mode: AgentMode,
    thinking_level: ThinkingLevel,
    project_label: String,
    model_label: String,
    draft: Option<State<String>>,
    suppress_enter_newline: Option<Ref<bool>>,
}

#[component]
fn PromptChrome(props: &PromptChromeProps) -> impl Into<AnyElement<'static>> {
    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            border_style: BorderStyle::None,
            align_items: AlignItems::FlexStart,
            flex_direction: FlexDirection::Column,
            margin_bottom: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
        ) {
            Editor(
                screen_width: props.screen_width,
                screen_height: props.screen_height,
                agent_mode: props.agent_mode,
                draft: props.draft,
                suppress_enter_newline: props.suppress_enter_newline,
            )
            Footer(
                screen_width: props.screen_width,
                project_label: props.project_label.clone(),
                model_label: props.model_label.clone(),
                thinking_level: props.thinking_level,
            )
        }
    }
}

#[component]
fn MainShell(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (screen_width, screen_height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut time = hooks.use_state(|| Local::now());
    let mut should_exit = hooks.use_state(|| false);
    let mut agent_mode = hooks.use_state(AgentMode::default);
    let mut thinking_level = hooks.use_state(|| ThinkingLevel::Xhigh);
    let mut draft = hooks.use_state(String::new);
    let mut messages = hooks.use_state(seed_transcript_messages);
    let mut suppress_enter_newline = hooks.use_ref(|| false);

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            time.set(Local::now());
        }
    });

    hooks.use_terminal_events(move |event| {
        let TerminalEvent::Key(KeyEvent {
            code, kind, modifiers, ..
        }) = event
        else {
            return;
        };
        if kind == KeyEventKind::Release {
            return;
        }

        match (modifiers, code) {
            (m, KeyCode::Tab) if !m.contains(KeyModifiers::SHIFT) => {
                agent_mode.set(agent_mode.get().next());
            }
            (m, KeyCode::BackTab) if m.contains(KeyModifiers::SHIFT) => {
                thinking_level.set(thinking_level.get().next());
            }
            (m, KeyCode::Char('d')) if m.contains(KeyModifiers::CONTROL) => should_exit.set(true),
            (m, KeyCode::Enter) if !m.contains(KeyModifiers::SHIFT) => {
                let text = draft.read().clone();
                if text.trim().is_empty() {
                    return;
                }
                if is_quit_command(&text) {
                    should_exit.set(true);
                    draft.set(String::new());
                    suppress_enter_newline.set(true);
                    return;
                }
                messages.set({
                    let mut list = messages.read().clone();
                    list.push(TranscriptMessage {
                        content: text,
                        style: TranscriptStyle::User,
                    });
                    list
                });
                draft.set(String::new());
                suppress_enter_newline.set(true);
            }
            _ => {}
        }
    });

    if should_exit.get() {
        system.exit();
    }

    let time_label = format!("Current Time: {}", time.get().format("%r"));

    element! {
        View(
            width: screen_width,
            height: screen_height,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Center,
            margin: 0,
            padding: 0,
        ) {
            Header(
                screen_width: screen_width,
                session_label: "Session: 019f631516e6g29o | turn: 0".to_string(),
            )
            TranscriptPanel(
                screen_width: screen_width,
                messages: messages.read().clone(),
                sticky_scroll: false, // Disable sticky scroll for the transcript panel
            )
            StatusRow(
                screen_width: screen_width,
                time_label: time_label,
            )
            PromptChrome(
                screen_width: screen_width,
                screen_height: screen_height,
                agent_mode: agent_mode.get(),
                thinking_level: thinking_level.get(),
                project_label: "~ my-project [branch-name]".to_string(),
                model_label: "anthropic/opus-4.8".to_string(),
                draft: Some(draft),
                suppress_enter_newline: Some(suppress_enter_newline),
            )
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(MainShell)
        .render_loop()
        .fullscreen()
        .enable_mouse_capture()
        .ignore_ctrl_c()
        .await?;
    Ok(())
}
