use crate::diff::{LineComponent, SelectItem, SelectList, SelectListTheme};
use iocraft::prelude::*;

#[derive(Props)]
pub struct OAuthSelectorProps {
    pub providers: Vec<SelectItem>,
    pub visible: bool,
    pub on_select: HandlerMut<'static, SelectItem>,
    pub on_cancel: HandlerMut<'static, ()>,
}

/// Mock OAuth provider list for TUI-only integration.
pub fn mock_oauth_providers() -> Vec<SelectItem> {
    vec![
        SelectItem::new("anthropic", "Anthropic").with_description("Claude models"),
        SelectItem::new("openai", "OpenAI").with_description("GPT models"),
        SelectItem::new("google", "Google").with_description("Gemini models"),
    ]
}

#[component]
pub fn OAuthSelector(mut hooks: Hooks, props: &mut OAuthSelectorProps) -> impl Into<AnyElement<'static>> {
    let mut selected = hooks.use_state(|| 0usize);
    let mut on_select = props.on_select.take();
    let mut on_cancel = props.on_cancel.take();
    let providers = props.providers.clone();
    let providers_for_input = providers.clone();
    let visible = props.visible;
    let (term_width, _) = hooks.use_terminal_size();

    hooks.use_terminal_events(move |event| {
        if !visible || providers_for_input.is_empty() {
            return;
        }
        let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
            return;
        };
        if kind == KeyEventKind::Release {
            return;
        }
        match code {
            KeyCode::Up => {
                let next = if selected.get() == 0 {
                    providers_for_input.len() - 1
                } else {
                    selected.get() - 1
                };
                selected.set(next);
            }
            KeyCode::Down => {
                let next = if selected.get() + 1 >= providers_for_input.len() {
                    0
                } else {
                    selected.get() + 1
                };
                selected.set(next);
            }
            KeyCode::Enter => {
                if let Some(item) = providers_for_input.get(selected.get()).cloned() {
                    on_select(item);
                }
            }
            KeyCode::Esc => on_cancel(()),
            _ => {}
        }
    });

    let lines = if visible && !providers.is_empty() {
        let mut list = SelectList::new(providers, 6, SelectListTheme::dark());
        list.set_selected_index(selected.get());
        let mut rendered = list.render(term_width.max(40));
        rendered.insert(0, "Select provider:".to_string());
        rendered
    } else {
        Vec::new()
    };

    element! {
        View(
            width: 100pct,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        ) {
            #(if visible && !lines.is_empty() {
                Some(element! {
                    View(
                        border_style: BorderStyle::Round,
                        border_color: Color::Yellow,
                        padding: 1,
                        width: 60pct,
                    ) {
                        Text(content: lines.join("\n"))
                    }
                })
            } else {
                None
            })
        }
    }
}
