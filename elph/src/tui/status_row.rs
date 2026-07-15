//! Status row between transcript and editor.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use elph_tui::rgb;
use iocraft::prelude::*;

use super::activity::{braille_spinner_glyph, format_activity_line};

const IDLE_ACTION_HINT: &str = "Enter to send · Ctrl+Q exit";

const TIPS: &[&str] = &[
    "Shift+↑↓ scrolls the transcript",
    "Ctrl+A cycles agent mode",
    "Shift+Tab cycles thinking level",
    "Ctrl+L opens the model picker",
    "Shift+Enter inserts a newline",
    "Click footer labels to change mode",
];

const BUSY_CANCEL_HINT: &str = "Ctrl+C cancel";

/// Props for [`StatusRow`].
#[derive(Props)]
pub struct StatusRowProps {
    pub screen_width: u16,
    pub busy: bool,
    pub activity_label: String,
    pub accent: Color,
    /// Drives braille spinner animation from the shell tick (no local timer).
    pub spinner_tick: u32,
    /// Elapsed seconds for the active turn, updated by the shell tick.
    pub elapsed_secs: f64,
}

impl Default for StatusRowProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            busy: false,
            activity_label: String::new(),
            accent: default_spinner_accent(),
            spinner_tick: 0,
            elapsed_secs: 0.0,
        }
    }
}

fn initial_tip_index() -> usize {
    random_tip_index(0, TIPS.len())
}

/// Pick a pseudo-random tip index, avoiding `current` when possible.
fn random_tip_index(current: usize, tip_count: usize) -> usize {
    if tip_count <= 1 {
        return 0;
    }
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut next = (seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(current as u64) as usize) % tip_count;
    if next == current {
        next = (current + 1) % tip_count;
    }
    next
}

#[component]
pub fn StatusRow(props: &StatusRowProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut tip_index = hooks.use_ref(initial_tip_index);
    let mut was_busy = hooks.use_ref(|| false);

    if props.busy && !was_busy.get() {
        was_busy.set(true);
    } else if !props.busy && was_busy.get() {
        was_busy.set(false);
        tip_index.set(random_tip_index(tip_index.get(), TIPS.len()));
    }

    let right_half = props.screen_width / 2;
    let idle_tip = TIPS[tip_index.get() % TIPS.len()].to_string();
    let activity_line = format_activity_line(&props.activity_label, props.elapsed_secs);
    let _spinner_frame = props.spinner_tick;
    let spinner_glyph = if props.busy {
        braille_spinner_glyph(props.spinner_tick)
    } else {
        " "
    };

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1,
            padding_right: 1,
        ) {
            View(
                width: right_half,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Start,
                padding: 0,
            ) {
                #(if props.busy {
                    element! {
                        View(
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Start,
                            flex_shrink: 0f32,
                            gap: 1,
                            padding: 0,
                        ) {
                            Text(
                                color: props.accent,
                                wrap: TextWrap::NoWrap,
                                content: spinner_glyph.to_string(),
                            )
                            Text(
                                color: Color::DarkGrey,
                                wrap: TextWrap::NoWrap,
                                content: activity_line,
                            )
                        }
                    }
                } else {
                    element! {
                        View(align_items: AlignItems::Center, justify_content: JustifyContent::Start) {
                            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: idle_tip)
                        }
                    }
                })
            }
            View(
                width: right_half,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::End,
                padding: 0,
            ) {
                #(if props.busy {
                    element! {
                        View(align_items: AlignItems::Center, justify_content: JustifyContent::End) {
                            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: BUSY_CANCEL_HINT)
                        }
                    }
                } else {
                    element! {
                        View(align_items: AlignItems::Center, justify_content: JustifyContent::End) {
                            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: IDLE_ACTION_HINT)
                        }
                    }
                })
            }
        }
    }
}

/// Default accent for the braille spinner (opencode theme).
pub fn default_spinner_accent() -> Color {
    rgb(0xfa, 0xb2, 0x83)
}

/// Elapsed seconds rounded to one decimal (50ms tick granularity).
pub fn format_elapsed_secs(started: Instant) -> f64 {
    let tenths = started.elapsed().as_millis() / 100;
    tenths as f64 / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_tip_index_is_in_range() {
        let idx = initial_tip_index();
        assert!(idx < TIPS.len());
    }

    #[test]
    fn random_tip_index_avoids_current_when_possible() {
        let next = random_tip_index(2, TIPS.len());
        assert_ne!(next, 2);
        assert!(next < TIPS.len());
    }

    #[test]
    fn format_elapsed_rounds_to_tenths() {
        let started = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let elapsed = format_elapsed_secs(started);
        assert!((0.2..=0.4).contains(&elapsed));
    }
}
