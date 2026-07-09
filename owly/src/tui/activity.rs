//! Compact live activity bar shown while the agent is running.

use elph_tui::{Theme, ToolExecutionState, ToolExecutionStatus};
use iocraft::prelude::*;

use super::chrome::{H_INSET, SECTION_PAD};
use super::spinner::LoadingSpinner;

#[derive(Default, Props)]
pub struct ActivityBarProps {
    pub command: Option<String>,
    pub live_tools: Option<State<Vec<ToolExecutionState>>>,
    pub theme: Theme,
}

#[component]
pub fn ActivityBar(props: &ActivityBarProps) -> impl Into<AnyElement<'static>> {
    let palette = props.theme;
    let command = props
        .command
        .as_deref()
        .map(|name| format!("owly {name}"))
        .unwrap_or_else(|| "owly".to_string());

    let running_count = props
        .live_tools
        .as_ref()
        .map(|state| {
            state
                .read()
                .iter()
                .filter(|tool| matches!(tool.status, ToolExecutionStatus::Running | ToolExecutionStatus::Pending))
                .count()
        })
        .unwrap_or(0);

    let status_label = if running_count > 0 {
        format!("{command} · {running_count} tool(s)")
    } else {
        format!("{command} · working")
    };

    let chips: Vec<AnyElement<'static>> = props
        .live_tools
        .as_ref()
        .map(|state| {
            state
                .read()
                .iter()
                .take(6)
                .map(|tool| {
                    let label = tool_chip_label(tool);
                    element! {
                        Text(color: Some(tool_chip_color(tool.status, palette)), content: label)
                    }
                    .into_any()
                })
                .collect()
        })
        .unwrap_or_default();

    element! {
        View(
            flex_shrink: 0.0,
            width: 100pct,
            padding_left: H_INSET,
            padding_right: H_INSET,
            padding_top: SECTION_PAD,
            padding_bottom: SECTION_PAD,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            gap: Gap::Length(2),
        ) {
            LoadingSpinner(theme: palette)
            Text(content: status_label, color: Some(palette.muted))
            #(if chips.is_empty() {
                None
            } else {
                Some(element! {
                    View(
                        flex_direction: FlexDirection::Row,
                        flex_grow: 1.0,
                        gap: Gap::Length(3),
                        align_items: AlignItems::Center,
                        padding_left: 1,
                    ) {
                        Text(content: "·", color: Some(palette.prompt_prefix))
                        #(chips)
                    }
                }.into_any())
            })
        }
    }
}

fn tool_chip_label(tool: &ToolExecutionState) -> String {
    let args = tool.args_summary.trim();
    if args.is_empty() {
        tool.name.clone()
    } else {
        format!("{} {}", tool.name, truncate_args(args))
    }
}

fn truncate_args(args: &str) -> String {
    const MAX: usize = 24;
    if args.len() <= MAX {
        args.to_string()
    } else {
        format!("{}…", &args[..MAX.saturating_sub(1)])
    }
}

fn tool_chip_color(status: ToolExecutionStatus, theme: Theme) -> Color {
    match status {
        ToolExecutionStatus::Running | ToolExecutionStatus::Pending => theme.muted,
        ToolExecutionStatus::Success => theme.prompt_prefix,
        ToolExecutionStatus::Error => Color::Red,
        ToolExecutionStatus::Cancelled => theme.muted,
    }
}
