//! Cell grid for custom drawing. The iocraft render loop already diffs frames.

use iocraft::prelude::*;

/// A simple character cell grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameBuffer {
    width: u16,
    height: u16,
    cells: Vec<char>,
}

impl Default for FrameBuffer {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl FrameBuffer {
    pub fn new(width: u16, height: u16) -> Self {
        let len = width as usize * height as usize;
        Self {
            width,
            height,
            cells: vec![' '; len],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn set_char(&mut self, x: u16, y: u16, ch: char) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = y as usize * self.width as usize + x as usize;
        self.cells[idx] = ch;
    }

    pub fn set_text(&mut self, x: u16, y: u16, text: &str) {
        for (i, ch) in text.chars().enumerate() {
            let col = x.saturating_add(i as u16);
            if col >= self.width {
                break;
            }
            self.set_char(col, y, ch);
        }
    }

    pub fn line(&self, y: u16) -> String {
        if y >= self.height {
            return String::new();
        }
        let start = y as usize * self.width as usize;
        let end = start + self.width as usize;
        self.cells[start..end].iter().collect()
    }

    pub fn lines(&self) -> Vec<String> {
        (0..self.height).map(|y| self.line(y)).collect()
    }
}

/// Props for [`FrameBufferView`].
#[derive(Clone, Default, Props)]
pub struct FrameBufferViewProps {
    pub buffer: FrameBuffer,
    pub color: Option<Color>,
}

/// Render a [`FrameBuffer`] as monospace text lines.
#[component]
pub fn FrameBufferView(props: &FrameBufferViewProps) -> impl Into<AnyElement<'static>> {
    let color = props.color.unwrap_or(Color::Grey);
    let rows: Vec<_> = props
        .buffer
        .lines()
        .into_iter()
        .map(|line| {
            element! {
                Text(content: line, color, wrap: TextWrap::NoWrap)
            }
        })
        .collect();

    element! {
        View(
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Single,
            border_color: Color::DarkGrey,
            padding: 1,
        ) {
            #(rows)
        }
    }
}
