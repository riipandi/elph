use super::prompt_transcript::PromptTranscript;
use crate::agent::TranscriptView;
use crate::theme::Theme;
use crate::transcript::TranscriptEntry;
use iocraft::prelude::*;

/// Default lines scrolled per Up/Down key press.
pub const DEFAULT_LINE_SCROLL_STEP: u16 = 3;

/// Use viewport height for Page Up/Down when [`ChatStreamProps::page_scroll_step`] is zero.
pub const PAGE_SCROLL_VIEWPORT: u16 = 0;

#[derive(Props)]
pub struct ChatStreamProps {
    /// Submitted messages, oldest first.
    pub messages: Vec<String>,
    /// When false, keyboard scroll keys are ignored (e.g. while the prompt has focus).
    pub scroll_enabled: bool,
    /// Pin to bottom while the user has not scrolled up.
    pub auto_scroll: bool,
    /// Lines to scroll per Up/Down key press.
    pub line_scroll_step: u16,
    /// Lines to scroll per Page Up/Down. [`PAGE_SCROLL_VIEWPORT`] uses the visible height.
    pub page_scroll_step: u16,
    /// Active color palette.
    pub theme: Theme,
    /// Rich transcript entries. When set, renders [`TranscriptView`] instead of plain messages.
    pub entries: Option<Vec<TranscriptEntry>>,
    /// Whether thinking blocks are shown in rich transcript mode.
    pub show_thinking: bool,
}

impl Default for ChatStreamProps {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            scroll_enabled: true,
            auto_scroll: true,
            line_scroll_step: DEFAULT_LINE_SCROLL_STEP,
            page_scroll_step: PAGE_SCROLL_VIEWPORT,
            theme: Theme::default(),
            entries: None,
            show_thinking: true,
        }
    }
}

/// Scrollable chat transcript area with configurable keyboard scroll speed.
#[component]
pub fn ChatStream(mut hooks: Hooks, props: &mut ChatStreamProps) -> impl Into<AnyElement<'static>> {
    let handle = hooks.use_ref_default::<ScrollViewHandle>();
    let line_scroll_step = props.line_scroll_step.max(1) as i32;
    let page_scroll_step = props.page_scroll_step;
    let auto_scroll = props.auto_scroll;
    let scroll_enabled = props.scroll_enabled;
    let show_thinking = props.show_thinking;
    let theme = props.theme;

    hooks.use_terminal_events({
        let mut handle = handle;
        move |event| {
            if !scroll_enabled {
                return;
            }

            let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
                return;
            };

            if kind == KeyEventKind::Release {
                return;
            }

            match code {
                KeyCode::Up => handle.write().scroll_by(-line_scroll_step),
                KeyCode::Down => handle.write().scroll_by(line_scroll_step),
                KeyCode::PageUp => {
                    let step = page_scroll_amount(&handle, page_scroll_step);
                    handle.write().scroll_by(-step);
                }
                KeyCode::PageDown => {
                    let step = page_scroll_amount(&handle, page_scroll_step);
                    handle.write().scroll_by(step);
                }
                KeyCode::Home => handle.write().scroll_to_top(),
                KeyCode::End => {
                    if auto_scroll {
                        handle.write().scroll_to_bottom();
                    } else {
                        let max = handle
                            .read()
                            .content_height()
                            .saturating_sub(handle.read().viewport_height());
                        handle.write().scroll_to(max as i32);
                    }
                }
                _ => {}
            }
        }
    });

    element! {
        View(width: 100pct, height: 100pct) {
            ScrollView(
                auto_scroll: auto_scroll,
                keyboard_scroll: false,
                scroll_step: Some(props.line_scroll_step.max(1)),
                scrollbar_thumb_color: Some(theme.scrollbar_thumb),
                scrollbar_track_color: Some(theme.scrollbar_track),
                handle: Some(handle),
            ) {
                #(if let Some(entries) = props.entries.clone() {
                    element!(TranscriptView(entries: entries, theme: theme, show_thinking: show_thinking)).into_any()
                } else {
                    element!(PromptTranscript(messages: props.messages.clone(), theme: theme)).into_any()
                })
            }
        }
    }
}

fn page_scroll_amount(handle: &Ref<ScrollViewHandle>, page_scroll_step: u16) -> i32 {
    if page_scroll_step == PAGE_SCROLL_VIEWPORT {
        handle.read().viewport_height().max(1) as i32
    } else {
        page_scroll_step.max(1) as i32
    }
}
