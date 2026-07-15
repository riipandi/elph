//! TUI demo - basic chat layout
//!
//! Mirrors `elph/src/tui.rs` shell layout.
//! Color reference: https://www.ditig.com/256-colors-cheat-sheet
//!
//! ```bash
//! cargo run -p elph-tui --example chat_layout
//! ```

use anyhow::Result;
use elph_tui::{
    KittScannerView, Textarea, TranscriptRowLayout, active_sticky_user_message_index, layout_sticky_header,
    layout_transcript_rows_widths, rgb, scroll_view_down, scroll_view_up, transcript_bubble_inner_width,
};
use iocraft::prelude::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

    fn is_sticky_prompt(self) -> bool {
        matches!(self, Self::User)
    }

    fn bubble_padding_rows(self) -> u16 {
        self.padding().saturating_mul(2)
    }

    fn sticky_padding_top(self) -> u16 {
        self.padding()
    }

    fn sticky_padding_bottom(self) -> u16 {
        self.padding()
    }

    fn sticky_bubble_padding_rows(self) -> u16 {
        self.sticky_padding_top().saturating_add(self.sticky_padding_bottom())
    }

    fn horizontal_padding(self) -> u16 {
        self.padding()
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
            Text(color: style.text_color(), wrap: TextWrap::Wrap, content: message.content.as_str())
        }
    }
    .into()
}

fn transcript_sticky_overlay(height: u16, message: &TranscriptMessage, display_content: &str) -> AnyElement<'static> {
    let style = message.style;
    let pad_h = style.padding();
    element! {
        View(
            position: Position::Absolute,
            top: 0,
            left: 0,
            right: 1,
            height: height,
            overflow: Overflow::Hidden,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            padding_left: 1,
            padding_right: 1,
            padding_bottom: 1,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Baseline,
        ) {
            View(
                width: 100pct,
                background_color: style.background_color(),
                padding_top: style.sticky_padding_top(),
                padding_bottom: style.sticky_padding_bottom(),
                padding_left: pad_h,
                padding_right: pad_h,
                flex_shrink: 0f32,
                margin_bottom: 0,
            ) {
                Text(
                    color: style.text_color(),
                    wrap: TextWrap::NoWrap,
                    content: display_content.to_string(),
                )
            }
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
    messages: Option<State<Vec<TranscriptMessage>>>,
    messages_revision: u64,
    sticky_scroll: bool,
}

struct TranscriptRenderCache {
    revision: u64,
    screen_width: u16,
    row_layouts: Vec<TranscriptRowLayout>,
    is_sticky_prompt: Vec<bool>,
}

const TRANSCRIPT_SCROLL_STEP: i32 = 3;
const STICKY_MIN_SCROLL_ROWS: u16 = 3;

#[component]
fn TranscriptPanel(props: &TranscriptPanelProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let scroll_handle = hooks.use_ref_default::<ScrollViewHandle>();
    let mut render_cache = hooks.use_ref(|| None::<TranscriptRenderCache>);
    let scroll_generation = hooks.use_state(|| 0u32);
    let empty_messages = hooks.use_state(Vec::<TranscriptMessage>::new);
    let messages_state = props.messages.unwrap_or(empty_messages);
    let messages = messages_state.read();
    let _scroll_generation = scroll_generation.get();
    let cache_key = (props.messages_revision, props.screen_width);

    if render_cache
        .read()
        .as_ref()
        .is_none_or(|c| c.revision != cache_key.0 || c.screen_width != cache_key.1)
    {
        let texts: Vec<&str> = messages.iter().map(|m| m.content.as_str()).collect();
        let wrap_widths: Vec<u16> = messages
            .iter()
            .map(|m| transcript_bubble_inner_width(props.screen_width, m.style.horizontal_padding()))
            .collect();
        let row_layouts = layout_transcript_rows_widths(&texts, &wrap_widths, 1);
        let is_sticky_prompt: Vec<_> = messages.iter().map(|m| m.style.is_sticky_prompt()).collect();
        render_cache.set(Some(TranscriptRenderCache {
            revision: cache_key.0,
            screen_width: cache_key.1,
            row_layouts,
            is_sticky_prompt,
        }));
    }

    let cache = render_cache.read();
    let cached = cache.as_ref().expect("transcript render cache");
    let row_layouts = &cached.row_layouts;
    let is_sticky_prompt = &cached.is_sticky_prompt;
    let bubbles: Vec<_> = messages
        .iter()
        .map(|message| transcript_message_bubble(props.screen_width, message))
        .collect();

    let handle = scroll_handle.read();
    let scroll_viewport = handle.viewport_height().max(1);
    let min_content_height = scroll_viewport;
    let sticky_idx = props
        .sticky_scroll
        .then(|| {
            active_sticky_user_message_index(
                &row_layouts,
                is_sticky_prompt,
                handle.scroll_offset(),
                handle.is_auto_scroll_pinned(),
            )
        })
        .flatten();
    let panel_height = scroll_viewport;
    let sticky_header = sticky_idx.and_then(|idx| {
        if !messages[idx].style.is_sticky_prompt() {
            return None;
        }
        layout_sticky_header(
            &messages[idx].content,
            transcript_bubble_inner_width(props.screen_width, messages[idx].style.horizontal_padding()),
            messages[idx].style.sticky_bubble_padding_rows(),
            panel_height,
            STICKY_MIN_SCROLL_ROWS,
        )
    });
    let sticky_rows = sticky_header.as_ref().map(|h| h.height).unwrap_or(0);

    hooks.use_terminal_events({
        let mut scroll_handle = scroll_handle;
        let mut scroll_generation = scroll_generation;
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
            let scrolled = match code {
                KeyCode::Up => {
                    scroll_view_up(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP);
                    true
                }
                KeyCode::Down => {
                    scroll_view_down(&mut scroll_handle.write(), TRANSCRIPT_SCROLL_STEP);
                    true
                }
                _ => false,
            };
            if scrolled {
                scroll_generation.set(scroll_generation.get().wrapping_add(1));
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
                        padding_top: sticky_rows,
                        padding_bottom: 0,
                        padding_left: 1,
                        padding_right: 1,
                        gap: 1,
                    ) {
                        #(bubbles)
                    }
                }
                #(if let (Some(idx), Some(header)) = (sticky_idx, sticky_header.as_ref()) {
                    Some(transcript_sticky_overlay(
                        header.height,
                        &messages[idx],
                        &header.display_text,
                    ))
                } else {
                    None
                })
            }
        }
    }
}

const IDLE_ACTION_HINT: &str = "Enter to send · Ctrl+Q exit";

const TIPS: &[&str] = &[
    "Shift+↑↓ scrolls the transcript",
    "Ctrl+A cycles agent mode",
    "Shift+Tab cycles thinking level",
    "Ctrl+L opens the model picker",
    "Shift+Enter inserts a newline",
    "Click footer labels to change mode",
];

const BUSY_CANCEL_HINT: &str = "Esc cancel";

const ELAPSED_TICK_MS: u64 = 200;

#[derive(Props)]
struct StatusRowProps {
    screen_width: u16,
    busy: bool,
    activity_label: String,
    accent: Color,
}

impl Default for StatusRowProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            busy: false,
            activity_label: String::new(),
            accent: rgb(0xfa, 0xb2, 0x83),
        }
    }
}

fn initial_tip_index() -> usize {
    random_tip_index(0, TIPS.len())
}

fn random_tip_index(current: usize, tip_count: usize) -> usize {
    if tip_count <= 1 {
        return 0;
    }
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut next = (seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(current as u64) as usize) % tip_count;
    if next == current {
        next = (current + 1) % tip_count;
    }
    next
}

fn format_elapsed_secs(started: Instant) -> f64 {
    let tenths = started.elapsed().as_millis() / 100;
    tenths as f64 / 10.0
}

fn format_activity_line(label: &str, elapsed_secs: f64) -> String {
    format!("{label} · {elapsed_secs:.1}s")
}

#[component]
fn StatusRow(props: &StatusRowProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut tip_index = hooks.use_ref(initial_tip_index);
    let mut busy_started_at = hooks.use_ref(|| None::<Instant>);
    let mut is_busy = hooks.use_ref(|| false);
    let mut elapsed_secs = hooks.use_state(|| 0.0f64);

    let was_busy = is_busy.get();
    is_busy.set(props.busy);

    if props.busy && !was_busy {
        busy_started_at.set(Some(Instant::now()));
        elapsed_secs.set(0.0);
    } else if !props.busy && was_busy {
        busy_started_at.set(None);
        tip_index.set(random_tip_index(tip_index.get(), TIPS.len()));
        elapsed_secs.set(0.0);
    }

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(ELAPSED_TICK_MS)).await;
            if !is_busy.get() {
                continue;
            }
            if let Some(started) = busy_started_at.read().as_ref() {
                let next = format_elapsed_secs(*started);
                if (elapsed_secs.get() - next).abs() > f64::EPSILON {
                    elapsed_secs.set(next);
                }
            }
        }
    });

    let right_half = props.screen_width / 2;
    let idle_tip = TIPS[tip_index.get() % TIPS.len()].to_string();
    let activity_line = format_activity_line(&props.activity_label, elapsed_secs.get());

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
                width: right_half,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Start,
                padding: 0,
            ) {
                #(if props.busy {
                    element! {
                        View(
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Start,
                            flex_shrink: 0f32,
                            gap: 1,
                            padding: 0,
                        ) {
                            Text(
                                color: Color::DarkGrey,
                                wrap: TextWrap::NoWrap,
                                content: activity_line,
                            )
                            View(padding: 0, margin: 0) {
                                KittScannerView(
                                    width: 8u16,
                                    accent: props.accent,
                                    active: true,
                                )
                            }
                        }
                    }
                } else {
                    element! {
                        View(align_items: AlignItems::Center, justify_content: JustifyContent::Start) {
                            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: idle_tip)
                        }
                    }
                })
            }
            View(
                width: right_half,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::End,
                padding: 0,
            ) {
                #(if props.busy {
                    element! {
                        View(align_items: AlignItems::Center, justify_content: JustifyContent::End) {
                            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: BUSY_CANCEL_HINT)
                        }
                    }
                } else {
                    element! {
                        View(align_items: AlignItems::Center, justify_content: JustifyContent::End) {
                            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: IDLE_ACTION_HINT)
                        }
                    }
                })
            }
        }
    }
}

fn editor_max_height(screen_height: u16) -> u16 {
    (screen_height / 4).clamp(4, 12)
}

#[derive(Default, Props)]
struct EditorProps {
    screen_width: u16,
    screen_height: u16,
    agent_mode: AgentMode,
    draft: Option<State<String>>,
    live_draft: Option<Ref<String>>,
    suppress_enter_newline: Option<Ref<bool>>,
    on_submit: HandlerMut<'static, String>,
}

#[component]
fn Editor(props: &mut EditorProps) -> impl Into<AnyElement<'static>> {
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
        ) {
            Textarea(
                width: props.screen_width.saturating_sub(2),
                min_height: 1u16,
                max_height: Some(editor_max_height(props.screen_height)),
                show_border: Some(false),
                has_focus: true,
                value: props.draft,
                live_draft: props.live_draft,
                suppress_enter_newline: props.suppress_enter_newline,
                submit_on_enter: true,
                on_submit: props.on_submit.take(),
                text_color: Some(Color::Grey),
                cursor_color: Some(Color::White),
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

#[derive(Default, Props)]
struct PromptChromeProps {
    screen_width: u16,
    screen_height: u16,
    agent_mode: AgentMode,
    thinking_level: ThinkingLevel,
    project_label: String,
    model_label: String,
    draft: Option<State<String>>,
    live_draft: Option<Ref<String>>,
    suppress_enter_newline: Option<Ref<bool>>,
    on_submit: HandlerMut<'static, String>,
}

#[component]
fn PromptChrome(props: &mut PromptChromeProps) -> impl Into<AnyElement<'static>> {
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
                live_draft: props.live_draft,
                suppress_enter_newline: props.suppress_enter_newline,
                on_submit: props.on_submit.take(),
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
    let mut should_exit = hooks.use_state(|| false);
    let mut agent_mode = hooks.use_state(AgentMode::default);
    let mut thinking_level = hooks.use_state(|| ThinkingLevel::Xhigh);
    let mut draft = hooks.use_state(String::new);
    let mut live_draft = hooks.use_ref(String::new);
    let mut messages = hooks.use_state(seed_transcript_messages);
    let mut messages_revision = hooks.use_state(|| 0u64);
    let mut suppress_enter_newline = hooks.use_ref(|| false);
    let mut busy = hooks.use_state(|| false);
    let mut busy_generation = hooks.use_state(|| 0u64);
    let mut activity_label = hooks.use_state(|| "Working".to_string());

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if !busy.get() {
                continue;
            }
            let generation = busy_generation.get();
            tokio::time::sleep(Duration::from_secs(3)).await;
            if busy.get() && busy_generation.get() == generation {
                busy.set(false);
            }
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
            _ => {}
        }
    });

    if should_exit.get() {
        system.exit();
    }

    let (accent_r, accent_g, accent_b) = agent_mode.get().label_rgb();
    let scanner_accent = rgb(accent_r, accent_g, accent_b);

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
                messages: Some(messages),
                messages_revision: messages_revision.get(),
                sticky_scroll: false, // Disable sticky scroll for the transcript panel
            )
            StatusRow(
                screen_width: screen_width,
                busy: busy.get(),
                activity_label: activity_label.read().clone(),
                accent: scanner_accent,
            )
            PromptChrome(
                screen_width: screen_width,
                screen_height: screen_height,
                agent_mode: agent_mode.get(),
                thinking_level: thinking_level.get(),
                project_label: "~ my-project [branch-name]".to_string(),
                model_label: "anthropic/opus-4.8".to_string(),
                draft: Some(draft),
                live_draft: Some(live_draft),
                suppress_enter_newline: Some(suppress_enter_newline),
                on_submit: move |text: String| {
                    if text.trim().is_empty() {
                        return;
                    }
                    if is_quit_command(&text) {
                        should_exit.set(true);
                        draft.set(String::new());
                        live_draft.set(String::new());
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
                    messages_revision.set(messages_revision.get().wrapping_add(1));
                    busy.set(true);
                    busy_generation.set(busy_generation.get().saturating_add(1));
                    activity_label.set("Working".to_string());
                    draft.set(String::new());
                    live_draft.set(String::new());
                    suppress_enter_newline.set(true);
                },
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
