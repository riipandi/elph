use std::time::{Duration, Instant};

use super::ansi::{self, styled};
use super::component::{Line, LineComponent};
use super::text::Text;

const DEFAULT_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const DEFAULT_INTERVAL: Duration = Duration::from_millis(80);

/// Animated spinner with message (pi-tui `Loader`).
pub struct Loader {
    message: String,
    inner: Text,
    frames: Vec<String>,
    interval: Duration,
    running: bool,
    frame_index: usize,
    last_tick: Option<Instant>,
    spinner_color: u8,
    message_color: u8,
}

impl Loader {
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        let inner = Text::with_padding("", 0, 1);
        let frames: Vec<String> = DEFAULT_FRAMES.iter().map(|s| (*s).to_string()).collect();
        let mut loader = Self {
            message,
            inner,
            frames,
            interval: DEFAULT_INTERVAL,
            running: false,
            frame_index: 0,
            last_tick: None,
            spinner_color: 14,
            message_color: 252,
        };
        let line = loader.compose_line();
        loader.inner.set_text(line);
        loader
    }

    pub fn with_colors(mut self, spinner: u8, message: u8) -> Self {
        self.spinner_color = spinner;
        self.message_color = message;
        self
    }

    pub fn start(&mut self) {
        self.running = true;
        self.last_tick = None;
        self.inner.set_text(self.compose_line());
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.inner.invalidate();
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.inner.set_text(self.compose_line());
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn compose_line(&self) -> String {
        let frame = &self.frames[self.frame_index % self.frames.len()];
        let spinner = styled(&ansi::fg(self.spinner_color), frame);
        if self.message.is_empty() {
            spinner
        } else {
            let body = styled(&ansi::fg(self.message_color), &self.message);
            format!("{spinner} {body}")
        }
    }

    fn maybe_tick(&mut self) {
        if !self.running {
            return;
        }
        let now = Instant::now();
        if self.last_tick.is_none_or(|t| now.duration_since(t) >= self.interval) {
            self.frame_index = (self.frame_index + 1) % self.frames.len();
            self.last_tick = Some(now);
            self.inner.set_text(self.compose_line());
        }
    }
}

impl LineComponent for Loader {
    fn render(&mut self, width: u16) -> Vec<Line> {
        self.maybe_tick();
        self.inner.render(width)
    }

    fn invalidate(&mut self) {
        self.inner.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_renders_spinner_and_message() {
        let mut loader = Loader::new("Working");
        loader.start();
        let lines = loader.render(40);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("Working"));
    }
}
