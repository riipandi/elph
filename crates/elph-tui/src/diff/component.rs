/// A rendered terminal line (may include ANSI sequences).
pub type Line = String;

/// Whether a component consumed keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputResult {
    Ignored,
    Consumed,
}

/// Core component contract.
pub trait LineComponent {
    fn render(&mut self, width: u16) -> Vec<Line>;
    fn invalidate(&mut self);
    fn handle_input(&mut self, _data: &str) -> InputResult {
        InputResult::Ignored
    }
    fn set_focused(&mut self, _focused: bool) {}
    fn is_focused(&self) -> bool {
        false
    }
}

/// Components that emit [`super::cursor::CURSOR_MARKER`] when focused.
pub trait Focusable {
    fn set_focused(&mut self, focused: bool);
    fn is_focused(&self) -> bool;
}

impl<T: LineComponent> Focusable for T {
    fn set_focused(&mut self, focused: bool) {
        LineComponent::set_focused(self, focused);
    }

    fn is_focused(&self) -> bool {
        LineComponent::is_focused(self)
    }
}

/// Vertical stack of child components.
#[derive(Default)]
pub struct Container {
    children: Vec<Box<dyn LineComponent>>,
}

impl Container {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_child(&mut self, child: Box<dyn LineComponent>) {
        self.children.push(child);
    }

    pub fn clear(&mut self) {
        self.children.clear();
    }

    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    pub fn child_mut(&mut self, index: usize) -> Option<&mut Box<dyn LineComponent>> {
        self.children.get_mut(index)
    }

    pub fn render_children(&mut self, width: u16) -> Vec<Line> {
        let width = width.max(1);
        let mut lines = Vec::new();
        for child in &mut self.children {
            for line in child.render(width) {
                lines.push(line);
            }
        }
        lines
    }

    pub fn invalidate_children(&mut self) {
        for child in &mut self.children {
            child.invalidate();
        }
    }
}

impl LineComponent for Container {
    fn render(&mut self, width: u16) -> Vec<Line> {
        self.render_children(width)
    }

    fn invalidate(&mut self) {
        self.invalidate_children();
    }
}

/// Simple static text block for tests and placeholders.
pub struct TextBlock {
    lines: Vec<Line>,
}

impl TextBlock {
    pub fn new(lines: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            lines: lines.into_iter().map(Into::into).collect(),
        }
    }

    pub fn set_lines(&mut self, lines: impl IntoIterator<Item = impl Into<String>>) {
        self.lines = lines.into_iter().map(Into::into).collect();
    }
}

impl LineComponent for TextBlock {
    fn render(&mut self, _width: u16) -> Vec<Line> {
        self.lines.clone()
    }

    fn invalidate(&mut self) {}
}
