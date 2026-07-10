use slt::Color;

use super::{Theme, ThemeMode};

impl Theme {
    /// Background color for views; `None` leaves the terminal background untouched.
    pub fn view_background(self) -> Option<Color> {
        match self.background {
            Color::Reset => None,
            color => Some(color),
        }
    }

    /// Foreground color for text; `None` inherits the terminal foreground.
    pub fn text_color(self) -> Option<Color> {
        match self.foreground {
            Color::Reset => None,
            color => Some(color),
        }
    }

    pub fn input_cursor(self) -> Color {
        self.dim_text()
    }

    pub fn input_placeholder(self) -> Color {
        self.dim_text()
    }

    pub fn paste_label(self) -> Color {
        self.dim_text()
    }

    pub fn blue_col(self) -> Color {
        match self.mode {
            ThemeMode::Dark => Color::Cyan,
            ThemeMode::Light => Color::Blue,
        }
    }

    pub fn yellow_col(self) -> Color {
        Color::Yellow
    }

    pub fn highlight(self) -> Color {
        Color::Magenta
    }

    pub fn special(self) -> Color {
        Color::Green
    }

    pub fn dim_text(self) -> Color {
        self.muted
    }

    pub fn bright_text(self) -> Color {
        Color::Reset
    }

    pub fn user_pipe_col(self) -> Color {
        Color::Magenta
    }

    pub fn ai_pipe_col(self) -> Color {
        Color::DarkGray
    }

    /// Primary emphasis — inherits terminal foreground.
    pub fn white_col(self) -> Color {
        Color::Reset
    }

    pub fn thinking_color(self, level: &str) -> Color {
        match level.trim().to_ascii_lowercase().as_str() {
            "low" => Color::Green,
            "medium" => Color::Yellow,
            "high" => Color::Yellow,
            "xhigh" => Color::Red,
            _ => Color::DarkGray,
        }
    }

    pub fn context_usage_color(self, pct: f64) -> Color {
        if pct >= 90.0 {
            Color::Red
        } else if pct >= 80.0 {
            Color::LightRed
        } else if pct >= 50.0 {
            Color::Yellow
        } else {
            Color::Reset
        }
    }

    pub fn git_status_color(self, additions: u32, deletions: u32) -> Color {
        if additions == 0 && deletions == 0 {
            Color::DarkGray
        } else if additions > 0 && deletions == 0 {
            Color::Green
        } else if additions == 0 && deletions > 0 {
            Color::Red
        } else {
            Color::Yellow
        }
    }
}
