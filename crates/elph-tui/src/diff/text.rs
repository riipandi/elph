use crate::utils::{pad_lines, wrap_ansi_text};

use super::component::{Line, LineComponent};

/// Word-wrapped static text with optional padding (pi-tui `Text`).
pub struct Text {
    text: String,
    padding_x: u16,
    padding_y: u16,
    cache_width: Option<u16>,
    cache_lines: Vec<Line>,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_padding(text, 0, 0)
    }

    pub fn with_padding(text: impl Into<String>, padding_x: u16, padding_y: u16) -> Self {
        Self {
            text: text.into(),
            padding_x,
            padding_y,
            cache_width: None,
            cache_lines: Vec::new(),
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.invalidate();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn build_lines(&self, width: u16) -> Vec<Line> {
        let width = width.max(1) as usize;
        if self.text.trim().is_empty() {
            return Vec::new();
        }

        let normalized = self.text.replace('\t', "   ");
        let content_width = width.saturating_sub(self.padding_x as usize).max(1);
        let wrapped = wrap_ansi_text(&normalized, content_width);
        pad_lines(&wrapped, self.padding_x as usize, self.padding_y as usize)
    }
}

impl LineComponent for Text {
    fn render(&mut self, width: u16) -> Vec<Line> {
        if self.cache_width == Some(width) && !self.cache_lines.is_empty() {
            return self.cache_lines.clone();
        }
        let lines = self.build_lines(width);
        self.cache_width = Some(width);
        self.cache_lines = lines.clone();
        lines
    }

    fn invalidate(&mut self) {
        self.cache_width = None;
        self.cache_lines.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::str_display_width;

    #[test]
    fn empty_text_renders_nothing() {
        let mut text = Text::new("   ");
        assert!(text.render(40).is_empty());
    }

    #[test]
    fn wraps_to_width() {
        let mut text = Text::new("hello world foo bar");
        let lines = text.render(10);
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|l| str_display_width(l) <= 10));
    }
}
