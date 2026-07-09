//! Animated braille loading indicator for the Owly TUI.

use std::time::Duration;

use elph_tui::Theme;
use iocraft::prelude::*;
use tokio::time::sleep;

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const DEFAULT_INTERVAL_MS: u64 = 80;

/// Returns the spinner glyph for a monotonically increasing tick counter.
pub fn spinner_frame(tick: usize) -> &'static str {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

#[derive(Default, Props)]
pub struct LoadingSpinnerProps {
    pub theme: Theme,
}

#[component]
pub fn LoadingSpinner(mut hooks: Hooks, props: &LoadingSpinnerProps) -> impl Into<AnyElement<'static>> {
    let palette = props.theme;
    let mut tick = hooks.use_state(|| 0usize);

    hooks.use_future(async move {
        loop {
            sleep(Duration::from_millis(DEFAULT_INTERVAL_MS)).await;
            tick.set(tick.get().wrapping_add(1));
        }
    });

    element! {
        Text(content: spinner_frame(tick.get()), color: Some(palette.muted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_frame_cycles() {
        assert_eq!(spinner_frame(0), "⠋");
        assert_eq!(spinner_frame(9), "⠏");
        assert_eq!(spinner_frame(10), "⠋");
    }
}
