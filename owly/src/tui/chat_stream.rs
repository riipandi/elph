//! Owly-specific chat stream with structured transcript layout.

use elph_tui::{AssistantMessage, DEFAULT_LINE_SCROLL_STEP, PAGE_SCROLL_VIEWPORT, Theme};
use iocraft::prelude::*;

use super::chrome::SECTION_PAD;
use super::entries::{OwlyEntry, OwlyEntryKind};

#[derive(Props)]
pub struct OwlyChatStreamProps {
    pub entries_state: Option<State<Vec<OwlyEntry>>>,
    pub scroll_enabled: bool,
    pub auto_scroll: bool,
    pub line_scroll_step: u16,
    pub page_scroll_step: u16,
    pub theme: Theme,
    pub show_thinking: bool,
}

impl Default for OwlyChatStreamProps {
    fn default() -> Self {
        Self {
            entries_state: None,
            scroll_enabled: true,
            auto_scroll: true,
            line_scroll_step: DEFAULT_LINE_SCROLL_STEP,
            page_scroll_step: PAGE_SCROLL_VIEWPORT,
            theme: Theme::default(),
            show_thinking: true,
        }
    }
}

/// Scrollable Owly transcript with keyboard navigation.
#[component]
pub fn OwlyChatStream(mut hooks: Hooks, props: &mut OwlyChatStreamProps) -> impl Into<AnyElement<'static>> {
    let handle = hooks.use_ref_default::<ScrollViewHandle>();
    let line_scroll_step = props.line_scroll_step.max(1) as i32;
    let page_scroll_step = props.page_scroll_step;
    let auto_scroll = props.auto_scroll;
    let scroll_enabled = props.scroll_enabled;
    let show_thinking = props.show_thinking;
    let theme = props.theme;
    let entries_state = props.entries_state;

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
                OwlyTranscriptView(
                    entries_state: entries_state,
                    theme: theme,
                    show_thinking: show_thinking,
                )
            }
        }
    }
}

#[derive(Props)]
pub struct OwlyTranscriptViewProps {
    pub entries_state: Option<State<Vec<OwlyEntry>>>,
    pub theme: Theme,
    pub show_thinking: bool,
}

impl Default for OwlyTranscriptViewProps {
    fn default() -> Self {
        Self {
            entries_state: None,
            theme: Theme::default(),
            show_thinking: true,
        }
    }
}

#[component]
pub fn OwlyTranscriptView(props: &OwlyTranscriptViewProps) -> impl Into<AnyElement<'static>> {
    let theme = props.theme;
    let show_thinking = props.show_thinking;
    let mut children = Vec::new();
    let mut prev_kind: Option<OwlyEntryKind> = None;

    if let Some(state) = &props.entries_state {
        let entries = state.read();
        for entry in entries.iter() {
            if let Some(node) = render_entry(entry, theme, show_thinking, prev_kind) {
                prev_kind = Some(entry.kind);
                children.push(node);
            }
        }
    }

    element! {
        View(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            gap: Gap::Length(0),
            padding_top: SECTION_PAD,
            padding_bottom: SECTION_PAD,
        ) {
            #(children)
        }
    }
}

fn render_entry(
    entry: &OwlyEntry,
    theme: Theme,
    show_thinking: bool,
    prev_kind: Option<OwlyEntryKind>,
) -> Option<AnyElement<'static>> {
    let gap_before = section_gap(prev_kind, entry.kind);

    match entry.kind {
        OwlyEntryKind::Hint => {
            let content = entry.inner.content.trim();
            if content.is_empty() {
                return None;
            }
            Some(wrap_block(
                gap_before,
                element! {
                    Text(color: Some(theme.muted), content: content.to_string())
                }
                .into_any(),
            ))
        }
        OwlyEntryKind::User => Some(wrap_block(
            gap_before,
            element! {
                Text(color: theme.text_color(), content: format_user(&entry.inner.content))
            }
            .into_any(),
        )),
        OwlyEntryKind::Assistant => Some(wrap_block(
            gap_before,
            element!(AssistantMessage(
                content: entry.inner.content.clone(),
                is_streaming: entry.inner.is_streaming,
                theme: theme,
            ))
            .into_any(),
        )),
        OwlyEntryKind::Thinking if show_thinking => {
            let label = if entry.inner.thinking_expanded {
                format!("Thinking:\n{}", entry.inner.content)
            } else {
                "Thinking…".to_string()
            };
            Some(wrap_block(
                gap_before,
                element! {
                    Text(color: Some(theme.muted), content: label)
                }
                .into_any(),
            ))
        }
        OwlyEntryKind::Thinking => None,
        OwlyEntryKind::Status => {
            let content = entry.inner.content.trim();
            if content.is_empty() {
                return None;
            }
            Some(wrap_block(
                gap_before,
                element! {
                    Text(color: Some(theme.muted), content: format!("· {content}"))
                }
                .into_any(),
            ))
        }
        OwlyEntryKind::CommandHeader => Some(wrap_block(
            gap_before,
            element! {
                Text(color: Color::Cyan, content: format!("▸ {}", entry.inner.content))
            }
            .into_any(),
        )),
        OwlyEntryKind::CommandResult => Some(wrap_block(
            gap_before,
            element! {
                Text(
                    color: command_result_color(&entry.inner.content),
                    content: entry.inner.content.clone(),
                )
            }
            .into_any(),
        )),
        OwlyEntryKind::ToolSummary => entry.inner.tool.as_ref().map(|tool| {
            wrap_block(
                gap_before,
                element! {
                    Text(
                        color: tool_summary_color(tool.status),
                        content: format_tool_summary(tool),
                    )
                }
                .into_any(),
            )
        }),
    }
}

fn section_gap(prev: Option<OwlyEntryKind>, current: OwlyEntryKind) -> u16 {
    let Some(prev) = prev else {
        return 0;
    };
    if prev == current {
        return match current {
            OwlyEntryKind::Status | OwlyEntryKind::ToolSummary => 0,
            _ => 1,
        };
    }
    match (prev, current) {
        (OwlyEntryKind::Hint, OwlyEntryKind::Hint) => 0,
        (OwlyEntryKind::Status, OwlyEntryKind::Status) => 0,
        (OwlyEntryKind::ToolSummary, OwlyEntryKind::ToolSummary) => 0,
        (OwlyEntryKind::CommandHeader, OwlyEntryKind::Status) => 0,
        (OwlyEntryKind::Status, OwlyEntryKind::Assistant) => 1,
        (OwlyEntryKind::User, _) => 2,
        (OwlyEntryKind::CommandHeader, _) => 2,
        (OwlyEntryKind::CommandResult, _) => 2,
        (OwlyEntryKind::Assistant, OwlyEntryKind::User) => 2,
        (OwlyEntryKind::Hint, OwlyEntryKind::User) => 2,
        _ => 1,
    }
}

fn wrap_block(gap_before: u16, child: AnyElement<'static>) -> AnyElement<'static> {
    if gap_before == 0 {
        return child;
    }
    element! {
        View(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            padding_top: gap_before,
        ) {
            #(child)
        }
    }
    .into_any()
}

fn format_user(message: &str) -> String {
    let trimmed = message.trim_end();
    let mut lines = trimmed.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let mut out = format!("> {first}");
    for line in lines {
        out.push('\n');
        out.push_str("  ");
        out.push_str(line);
    }
    out
}

fn command_result_color(content: &str) -> Option<Color> {
    if content.starts_with('✓') {
        Some(Color::Green)
    } else if content.starts_with('✗') {
        Some(Color::Red)
    } else {
        None
    }
}

fn tool_summary_color(status: elph_tui::ToolExecutionStatus) -> Option<Color> {
    use elph_tui::ToolExecutionStatus;
    match status {
        ToolExecutionStatus::Success => Some(Color::Green),
        ToolExecutionStatus::Error => Some(Color::Red),
        ToolExecutionStatus::Running | ToolExecutionStatus::Pending => Some(Color::Cyan),
        ToolExecutionStatus::Cancelled => Some(Color::Yellow),
    }
}

fn format_tool_summary(tool: &elph_tui::ToolExecutionState) -> String {
    use elph_tui::ToolExecutionStatus;
    let icon = match tool.status {
        ToolExecutionStatus::Success => "✓",
        ToolExecutionStatus::Error => "✗",
        ToolExecutionStatus::Cancelled => "⊘",
        ToolExecutionStatus::Running => "⠋",
        ToolExecutionStatus::Pending => "○",
    };
    let args = tool.args_summary.trim();
    if args.is_empty() {
        format!("  {icon} {}", tool.name)
    } else {
        format!("  {icon} {}  {args}", tool.name)
    }
}

fn page_scroll_amount(handle: &Ref<ScrollViewHandle>, page_scroll_step: u16) -> i32 {
    if page_scroll_step == PAGE_SCROLL_VIEWPORT {
        handle.read().viewport_height().max(1) as i32
    } else {
        page_scroll_step.max(1) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_tui::{ToolExecutionState, ToolExecutionStatus};

    #[test]
    fn format_tool_summary_includes_name_and_args() {
        let tool = ToolExecutionState::new("1", "bash")
            .with_args("ls -la")
            .with_status(ToolExecutionStatus::Success);
        assert_eq!(format_tool_summary(&tool), "  ✓ bash  ls -la");
    }

    #[test]
    fn section_gap_adds_space_before_user_turn() {
        assert_eq!(section_gap(Some(OwlyEntryKind::Assistant), OwlyEntryKind::User), 2);
        assert_eq!(section_gap(Some(OwlyEntryKind::Status), OwlyEntryKind::Status), 0);
    }
}
