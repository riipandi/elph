use crate::diff::{MarkdownTheme, render_markdown_lines};
use crate::theme::Theme;
use crate::transcript::{ToolExecutionState, ToolExecutionStatus};
use iocraft::prelude::*;

#[derive(Props)]
pub struct ToolExecutionCardProps {
    pub tool: ToolExecutionState,
    pub theme: Theme,
    pub compact: bool,
    pub on_approve: HandlerMut<'static, String>,
    pub on_deny: HandlerMut<'static, String>,
}

impl Default for ToolExecutionCardProps {
    fn default() -> Self {
        Self {
            tool: ToolExecutionState::new("tool", "tool"),
            theme: Theme::default(),
            compact: false,
            on_approve: HandlerMut::default(),
            on_deny: HandlerMut::default(),
        }
    }
}

#[component]
pub fn ToolExecutionCard(props: &ToolExecutionCardProps) -> impl Into<AnyElement<'static>> {
    let tool = &props.tool;
    let status = status_label(tool.status);
    let border = props.theme.frame_border;
    let output = if tool.output.is_empty() || props.compact {
        String::new()
    } else {
        render_markdown_lines(&tool.output, 100, MarkdownTheme::dark()).join("\n")
    };

    element! {
        View(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            border_style: BorderStyle::Single,
            border_color: border,
            padding: 1,
            margin_bottom: 1,
        ) {
            Text(content: format!("⚙ {}  [{status}]", tool.name))
            #(if !tool.args_summary.is_empty() {
                Some(element! {
                    Text(color: Some(props.theme.muted), content: tool.args_summary.clone())
                })
            } else {
                None
            })
            #(if tool.status == ToolExecutionStatus::Running {
                Some(element! {
                    Text(content: "⠋ Running... (Esc to cancel)".to_string())
                })
            } else {
                None
            })
            #(if !output.is_empty() {
                Some(element! {
                    Text(content: output)
                })
            } else {
                None
            })
            #(if tool.requires_approval && tool.status == ToolExecutionStatus::Pending {
                Some(element! {
                    View(flex_direction: FlexDirection::Row, gap: Gap::Length(2)) {
                        Text(content: "[Approve]".to_string())
                        Text(content: "[Deny]".to_string())
                    }
                })
            } else {
                None
            })
        }
    }
}

#[derive(Props)]
pub struct ToolExecutionListProps {
    pub tools: Vec<ToolExecutionState>,
    pub theme: Theme,
}

#[component]
pub fn ToolExecutionList(props: &ToolExecutionListProps) -> impl Into<AnyElement<'static>> {
    let theme = props.theme;
    let cards: Vec<AnyElement<'static>> = props
        .tools
        .iter()
        .map(|tool| element!(ToolExecutionCard(tool: tool.clone(), theme: theme, compact: false)).into_any())
        .collect();

    element! {
        View(flex_direction: FlexDirection::Column, width: 100pct) {
            #(cards)
        }
    }
}

fn status_label(status: ToolExecutionStatus) -> &'static str {
    match status {
        ToolExecutionStatus::Pending => "pending",
        ToolExecutionStatus::Running => "running",
        ToolExecutionStatus::Success => "ok",
        ToolExecutionStatus::Error => "error",
        ToolExecutionStatus::Cancelled => "cancelled",
    }
}
