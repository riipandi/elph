use crate::bridge::OverlaySlot;
use crate::diff::{LineComponent, OverlayAnchor, OverlayOptions, SelectItem, SelectList, SelectListTheme, SizeValue};
use iocraft::prelude::*;

#[derive(Props)]
pub struct SessionSelectorProps {
    pub sessions: Vec<SelectItem>,
    pub visible: bool,
    pub on_select: HandlerMut<'static, SelectItem>,
    pub on_cancel: HandlerMut<'static, ()>,
}

#[component]
pub fn SessionSelector(mut hooks: Hooks, props: &mut SessionSelectorProps) -> impl Into<AnyElement<'static>> {
    let mut selected = hooks.use_state(|| 0usize);
    let mut on_select = props.on_select.take();
    let mut on_cancel = props.on_cancel.take();
    let sessions = props.sessions.clone();
    let sessions_for_input = sessions.clone();
    let visible = props.visible;
    let (term_width, _) = hooks.use_terminal_size();

    hooks.use_terminal_events(move |event| {
        if !visible || sessions_for_input.is_empty() {
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
                    sessions_for_input.len() - 1
                } else {
                    selected.get() - 1
                };
                selected.set(next);
            }
            KeyCode::Down => {
                let next = if selected.get() + 1 >= sessions_for_input.len() {
                    0
                } else {
                    selected.get() + 1
                };
                selected.set(next);
            }
            KeyCode::Enter => {
                if let Some(item) = sessions_for_input.get(selected.get()).cloned() {
                    on_select(item);
                }
            }
            KeyCode::Esc => on_cancel(()),
            _ => {}
        }
    });

    let lines = if visible && !sessions.is_empty() {
        let mut list = SelectList::new(sessions, 8, SelectListTheme::dark());
        list.set_selected_index(selected.get());
        list.render(term_width.max(40))
    } else {
        Vec::new()
    };

    element! {
        View(
            width: 100pct,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        ) {
            #(if visible && !lines.is_empty() {
                Some(element! {
                    View(
                        border_style: BorderStyle::Round,
                        border_color: Color::Blue,
                        padding: 1,
                        width: 80pct,
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

/// Builds an overlay slot for session selection (for use with [`OverlayStackHandle`]).
pub fn session_overlay_slot(sessions: Vec<SelectItem>) -> OverlaySlot {
    OverlaySlot::new(
        Box::new(SelectList::new(sessions, 8, SelectListTheme::dark())),
        OverlayOptions {
            width: Some(SizeValue::Percent(80.0)),
            max_height: Some(SizeValue::Percent(60.0)),
            anchor: OverlayAnchor::Center,
            ..Default::default()
        },
    )
}
