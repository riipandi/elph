use crate::diff::{MarkdownTheme, render_markdown_lines};
use crate::theme::Theme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct AssistantMessageProps {
    pub content: String,
    pub is_streaming: bool,
    pub theme: Theme,
}

#[derive(Clone)]
struct RenderCache {
    content: String,
    is_streaming: bool,
    rendered: String,
}

#[component]
pub fn AssistantMessage(mut hooks: Hooks, props: &AssistantMessageProps) -> impl Into<AnyElement<'static>> {
    let mut cache = hooks.use_ref(|| None::<RenderCache>);
    let palette = markdown_theme_from(props.theme);
    let suffix = if props.is_streaming { " ▌" } else { "" };

    let body = {
        let mut guard = cache.write();
        let needs_rebuild = guard
            .as_ref()
            .is_none_or(|c| c.content != props.content || c.is_streaming != props.is_streaming);
        if needs_rebuild {
            let rendered = render_markdown_lines(&props.content, 120, palette).join("\n");
            *guard = Some(RenderCache {
                content: props.content.clone(),
                is_streaming: props.is_streaming,
                rendered,
            });
        }
        format!("{}{}", guard.as_ref().expect("cache populated").rendered, suffix)
    };

    element! {
        View(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            padding_left: 1,
        ) {
            Text(content: body)
        }
    }
}

fn markdown_theme_from(theme: Theme) -> MarkdownTheme {
    let _ = theme;
    MarkdownTheme::dark()
}
