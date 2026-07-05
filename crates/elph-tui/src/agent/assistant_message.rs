use crate::diff::{MarkdownTheme, render_markdown_lines};
use crate::theme::Theme;
use iocraft::prelude::*;

#[derive(Props)]
pub struct AssistantMessageProps {
    pub content: String,
    pub is_streaming: bool,
    pub theme: Theme,
}

impl Default for AssistantMessageProps {
    fn default() -> Self {
        Self {
            content: String::new(),
            is_streaming: false,
            theme: Theme::default(),
        }
    }
}

#[component]
pub fn AssistantMessage(props: &AssistantMessageProps) -> impl Into<AnyElement<'static>> {
    let palette = markdown_theme_from(props.theme);
    let rendered = render_markdown_lines(&props.content, 120, palette).join("\n");
    let suffix = if props.is_streaming { " ▌" } else { "" };

    element! {
        View(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            padding_left: 1,
        ) {
            Text(content: format!("{rendered}{suffix}"))
        }
    }
}

fn markdown_theme_from(theme: Theme) -> MarkdownTheme {
    let _ = theme;
    MarkdownTheme::dark()
}
