use super::assistant_message::AssistantMessage;
use super::tool_execution::ToolExecutionCard;
use crate::theme::Theme;
use crate::transcript::{TranscriptEntry, TranscriptRole};
use iocraft::prelude::{HandlerMut, *};

#[derive(Props)]
pub struct TranscriptViewProps {
    /// Live transcript entries (preferred — avoids cloning the full vector each render).
    pub entries_state: Option<State<Vec<TranscriptEntry>>>,
    /// Static entries for tests and one-shot renders.
    pub entries: Vec<TranscriptEntry>,
    pub theme: Theme,
    pub show_thinking: bool,
}

impl Default for TranscriptViewProps {
    fn default() -> Self {
        Self {
            entries_state: None,
            entries: Vec::new(),
            theme: Theme::default(),
            show_thinking: true,
        }
    }
}

#[component]
pub fn TranscriptView(props: &TranscriptViewProps) -> impl Into<AnyElement<'static>> {
    let theme = props.theme;
    let show_thinking = props.show_thinking;
    let mut children = Vec::new();
    if let Some(state) = &props.entries_state {
        let entries = state.read();
        children.reserve(entries.len());
        children.extend(
            entries
                .iter()
                .filter_map(|entry| render_entry(entry, theme, show_thinking)),
        );
    } else {
        children.reserve(props.entries.len());
        children.extend(
            props
                .entries
                .iter()
                .filter_map(|entry| render_entry(entry, theme, show_thinking)),
        );
    }

    element! {
        View(flex_direction: FlexDirection::Column, width: 100pct) {
            #(children)
        }
    }
}

fn render_entry(entry: &TranscriptEntry, theme: Theme, show_thinking: bool) -> Option<AnyElement<'static>> {
    match entry.role {
        TranscriptRole::User => Some(
            element! {
                Text(color: theme.text_color(), content: format_user(&entry.content))
            }
            .into_any(),
        ),
        TranscriptRole::Assistant => Some(
            element!(AssistantMessage(
                content: entry.content.clone(),
                is_streaming: entry.is_streaming,
                theme: theme,
            ))
            .into_any(),
        ),
        TranscriptRole::Thinking if show_thinking => {
            let label = if entry.thinking_expanded {
                format!("Thinking:\n{}", entry.content)
            } else {
                "Thinking... (collapsed)".to_string()
            };
            Some(
                element! {
                    Text(color: Some(theme.muted), content: label)
                }
                .into_any(),
            )
        }
        TranscriptRole::Thinking => None,
        TranscriptRole::Tool => entry.tool.as_ref().map(|tool| {
            element!(ToolExecutionCard(
                tool: tool.clone(),
                theme: theme,
                compact: true,
                on_approve: HandlerMut::default(),
                on_deny: HandlerMut::default(),
            ))
            .into_any()
        }),
    }
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
