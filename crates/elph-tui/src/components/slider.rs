//! Numeric range slider.

use iocraft::prelude::*;

use super::theme::{UiTheme, resolve_ui_theme};

/// Props for [`Slider`].
#[derive(Default, Props)]
pub struct SliderProps {
    pub width: u16,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub value: Option<State<f32>>,
    pub has_focus: bool,
    pub label: String,
    pub fill_color: Option<Color>,
    pub track_color: Option<Color>,
    pub theme: Option<UiTheme>,
    pub on_change: HandlerMut<'static, f32>,
}

/// Next slider value after a key press.
pub fn slider_key_to_value(current: f32, min: f32, max: f32, code: KeyCode, step: f32) -> f32 {
    slider_key_delta(code, step)
        .map(|delta| (current + delta).clamp(min, max))
        .unwrap_or(current)
}

/// Step delta from a focused [`Slider`] key press.
pub fn slider_key_delta(code: KeyCode, step: f32) -> Option<f32> {
    match code {
        KeyCode::Left => Some(-step),
        KeyCode::Right => Some(step),
        _ => None,
    }
}

/// Label above a [`Slider`] track.
pub fn slider_label(label: &str, current: f32) -> String {
    if label.is_empty() {
        String::new()
    } else {
        format!("{label}: {:.0}", current)
    }
}

/// Fill percentage for [`Slider`] track (0–100).
pub fn slider_fill_percent(current: f32, min: f32, max: f32) -> f32 {
    if max <= min {
        0.0
    } else {
        ((current.clamp(min, max) - min) / (max - min)) * 100.0
    }
}

/// Horizontal slider adjusted with arrow keys when focused.
#[component]
pub fn Slider(props: &mut SliderProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let min = props.min;
    let max = props.max.max(min + props.step);
    let step = props.step.max(0.01);
    let internal = hooks.use_state(|| min);
    let mut value = props.value.unwrap_or(internal);
    let has_focus = props.has_focus;
    let theme = resolve_ui_theme(&hooks, props.theme);

    hooks.use_terminal_events({
        let mut on_change = props.on_change.take();
        move |event| {
            if !has_focus {
                return;
            }
            let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }
            let prev = value.get();
            let next = slider_key_to_value(prev, min, max, code, step);
            if next != prev {
                value.set(next);
                if !on_change.is_default() {
                    on_change(next);
                }
            }
        }
    });

    let current = value.get().clamp(min, max);
    let pct = slider_fill_percent(current, min, max);
    let fill = props.fill_color.unwrap_or(theme.success);
    let track = props.track_color.unwrap_or(theme.border_subtle);

    element! {
        View(width: props.width, flex_direction: FlexDirection::Column, gap: theme.gap_md) {
            Text(
                content: slider_label(&props.label, current),
                color: theme.text_secondary,
                wrap: TextWrap::NoWrap,
            )
            View(
                width: props.width,
                border_style: theme.container_border(has_focus),
                border_color: theme.container_border_color(has_focus),
                background_color: track,
                padding: theme.container_inset(),
            ) {
                View(width: Percent(pct), height: 1, background_color: fill)
            }
            #(if has_focus {
                Some(element! {
                    Text(
                        content: "←/→ adjust value".to_string(),
                        color: theme.text_hint,
                        wrap: TextWrap::NoWrap,
                    )
                })
            } else {
                None
            })
        }
    }
}
