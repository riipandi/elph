use crate::bridge::OverlaySlot;
use crate::diff::{LineComponent, OverlayAnchor, OverlayOptions, SelectItem, SelectList, SelectListTheme, SizeValue};
use iocraft::prelude::*;

#[derive(Props)]
pub struct ModelSelectorProps {
    pub models: Vec<SelectItem>,
    pub current_model: String,
    pub visible: bool,
    pub on_select: HandlerMut<'static, SelectItem>,
    pub on_cancel: HandlerMut<'static, ()>,
}

#[component]
pub fn ModelSelector(mut hooks: Hooks, props: &mut ModelSelectorProps) -> impl Into<AnyElement<'static>> {
    let mut selected = hooks.use_state(|| 0usize);
    let mut on_select = props.on_select.take();
    let mut on_cancel = props.on_cancel.take();
    let models = props.models.clone();
    let models_for_input = models.clone();
    let current_model = props.current_model.clone();
    let visible = props.visible;
    let (term_width, _) = hooks.use_terminal_size();

    hooks.use_terminal_events(move |event| {
        if !visible || models_for_input.is_empty() {
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
                    models_for_input.len() - 1
                } else {
                    selected.get() - 1
                };
                selected.set(next);
            }
            KeyCode::Down => {
                let next = if selected.get() + 1 >= models_for_input.len() {
                    0
                } else {
                    selected.get() + 1
                };
                selected.set(next);
            }
            KeyCode::Enter => {
                if let Some(item) = models_for_input.get(selected.get()).cloned() {
                    on_select(item);
                }
            }
            KeyCode::Esc => on_cancel(()),
            _ => {}
        }
    });

    let lines = if visible && !models.is_empty() {
        let mut list = SelectList::new(models, 10, SelectListTheme::dark());
        list.set_selected_index(selected.get());
        let mut rendered = list.render(term_width.max(40));
        rendered.insert(0, format!("Model: {current_model}"));
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
                        border_color: Color::Cyan,
                        padding: 1,
                        width: 70pct,
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

/// Builds an overlay slot for model selection.
pub fn model_overlay_slot(models: Vec<SelectItem>) -> OverlaySlot {
    OverlaySlot::new(
        Box::new(SelectList::new(models, 10, SelectListTheme::dark())),
        OverlayOptions {
            width: Some(SizeValue::Percent(70.0)),
            max_height: Some(SizeValue::Percent(50.0)),
            anchor: OverlayAnchor::Center,
            ..Default::default()
        },
    )
}
