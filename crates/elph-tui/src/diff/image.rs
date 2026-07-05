use crate::utils::truncate_to_width_no_ellipsis;

use super::ansi::{self, styled};
use super::component::{Line, LineComponent};
use super::terminal_image::{detect_image_protocol, encode_inline_image, png_dimensions};

/// Styling for unsupported image fallback text.
#[derive(Debug, Clone, Copy)]
pub struct ImageTheme {
    pub fallback_color: u8,
}

impl ImageTheme {
    pub fn dark() -> Self {
        Self { fallback_color: 245 }
    }
}

/// Inline image options.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageOptions {
    pub max_width_cells: Option<u16>,
    pub max_height_cells: Option<u16>,
    pub filename: Option<&'static str>,
}

/// Renders inline terminal images with Kitty/iTerm2 fallback (pi-tui `Image`).
pub struct Image {
    data: Vec<u8>,
    mime: String,
    theme: ImageTheme,
    options: ImageOptions,
    cache_width: Option<u16>,
    cache_lines: Vec<Line>,
}

impl Image {
    pub fn new(data: Vec<u8>, mime: impl Into<String>, theme: ImageTheme) -> Self {
        Self {
            data,
            mime: mime.into(),
            theme,
            options: ImageOptions::default(),
            cache_width: None,
            cache_lines: Vec::new(),
        }
    }

    pub fn with_options(mut self, options: ImageOptions) -> Self {
        self.options = options;
        self
    }

    fn cell_dimensions(&self) -> (u16, u16) {
        let (w, h) = png_dimensions(&self.data).unwrap_or((80, 40));
        let mut width_cells = w.div_ceil(8).min(120) as u16;
        let mut height_cells = h.div_ceil(16).min(40) as u16;
        if let Some(max_w) = self.options.max_width_cells {
            width_cells = width_cells.min(max_w);
        }
        if let Some(max_h) = self.options.max_height_cells {
            height_cells = height_cells.min(max_h);
        }
        (width_cells.max(1), height_cells.max(1))
    }

    fn fallback_label(&self) -> String {
        let name = self.options.filename.unwrap_or("image");
        let protocol = match detect_image_protocol() {
            super::terminal_image::ImageProtocol::Kitty => "kitty",
            super::terminal_image::ImageProtocol::ITerm2 => "iterm2",
            super::terminal_image::ImageProtocol::Unsupported => "unsupported",
        };
        format!("[Image: {name} — {protocol}]")
    }

    fn build_lines(&self, width: u16) -> Vec<Line> {
        let (w, h) = self.cell_dimensions();
        if let Some(seq) = encode_inline_image(&self.data, &self.mime, w, h) {
            return vec![seq];
        }
        let label = truncate_to_width_no_ellipsis(&self.fallback_label(), width.max(1) as usize);
        vec![styled(&ansi::fg(self.theme.fallback_color), &label)]
    }
}

impl LineComponent for Image {
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
