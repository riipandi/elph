//! Streaming log text with selection and follow-scroll (tuie-demo pattern).

use std::sync::{Arc, Mutex};

use chord_macro::chord;
use tuie::prelude::*;

/// Shared transcript body state addressable by stable [`WidgetId`] handles.
#[derive(Clone)]
pub(crate) struct StreamState {
    pub text_id: WidgetId<Text>,
    pub content: Arc<Mutex<StyledString>>,
    pub auto_scroll: Arc<Mutex<bool>>,
}

impl StreamState {
    pub(crate) fn paint(&self, root: &mut dyn Widget) {
        let content = self.content.lock().expect("stream content lock");
        let auto_scroll = *self.auto_scroll.lock().expect("stream auto_scroll lock");
        if let Some(text) = root.get_widget_mut(self.text_id) {
            text.set_content(content.clone());
        }
        if auto_scroll {
            tuie::reveal(self.text_id, Vec2::new(None, Some(Align::End)));
        }
    }

    pub(crate) fn plain_text(&self) -> String {
        self.content.lock().expect("stream content lock").to_string()
    }

    pub(crate) fn set_content(&self, root: &mut dyn Widget, content: StyledString) {
        *self.content.lock().expect("stream content lock") = content;
        self.paint(root);
    }

    pub(crate) fn append<'a>(&self, root: &mut dyn Widget, text: impl Into<StyledStr<'a>>) {
        self.content.lock().expect("stream content lock").push_span(text.into());
        self.paint(root);
    }

    pub(crate) fn set_auto_scroll(&self, root: &mut dyn Widget, enabled: bool) {
        *self.auto_scroll.lock().expect("stream auto_scroll lock") = enabled;
        if enabled {
            self.paint(root);
        }
    }

    pub(crate) fn is_auto_scrolling(&self) -> bool {
        *self.auto_scroll.lock().expect("stream auto_scroll lock")
    }
}

/// Append-only text surface backed by a single tuie [`Text`] widget.
pub struct StreamingText {
    pane: Box<Pane>,
    state: StreamState,
    selecting: bool,
    anchor: usize,
    active: usize,
    copy_on_select: bool,
}

impl StreamingText {
    pub fn new(initial: impl Into<StyledString>) -> Box<Self> {
        let content: StyledString = initial.into();
        let mut text_id = WidgetId::EMPTY;

        let pane = Pane::new().flex(1).children([Text::new()
            .id(&mut text_id)
            .content(content.clone())
            .overflow(TextOverflow::VISIBLE) as Box<dyn Widget>]);

        let state = StreamState {
            text_id,
            content: Arc::new(Mutex::new(content)),
            auto_scroll: Arc::new(Mutex::new(true)),
        };

        Box::new(Self {
            pane,
            state,
            selecting: false,
            anchor: 0,
            active: 0,
            copy_on_select: false,
        })
    }

    pub(crate) fn stream_state(&self) -> StreamState {
        self.state.clone()
    }

    pub fn set_copy_on_select(&mut self, enabled: bool) {
        self.copy_on_select = enabled;
    }

    pub fn is_copy_on_select(&self) -> bool {
        self.copy_on_select
    }

    /// Replaces the full transcript body.
    pub fn set_content(&mut self, content: StyledString) {
        self.anchor = 0;
        self.active = 0;
        self.selecting = false;
        self.state.set_content(&mut *self.pane, content);
    }

    /// Appends styled text and auto-scrolls when follow mode is on.
    pub fn append<'a>(&mut self, text: impl Into<StyledStr<'a>>) {
        self.state.append(&mut *self.pane, text);
        self.apply_highlight();
    }

    pub fn set_auto_scroll(&mut self, enabled: bool) {
        self.state.set_auto_scroll(&mut *self.pane, enabled);
    }

    pub fn is_auto_scrolling(&self) -> bool {
        self.state.is_auto_scrolling()
    }

    pub fn plain_text(&self) -> String {
        self.state.plain_text()
    }

    fn text(&self) -> String {
        self.plain_text()
    }

    fn mouse_to_index(&self, mouse: Vec2<i32>) -> usize {
        let origin = self.pane.get_pos();
        let text = self.pane.get_widget::<Text>(self.state.text_id).unwrap();
        let text_pos = text.get_pos() - origin;
        let local = Vec2::new((mouse.x - text_pos.x).max(0) as usize, (mouse.y - text_pos.y).max(0) as usize);
        text.pos_to_index(local)
    }

    fn word_at(&self, index: usize) -> (usize, usize) {
        let text = self.text();
        if text.is_empty() || index >= text.len() {
            return (index, index);
        }
        let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
        let bytes = text.as_bytes();
        let at_word = is_word(bytes[index]);
        let start = if at_word {
            let mut i = index;
            while i > 0 && is_word(bytes[i - 1]) {
                i -= 1;
            }
            i
        } else {
            let mut i = index;
            while i > 0 && !is_word(bytes[i - 1]) && bytes[i - 1] != b'\n' {
                i -= 1;
            }
            i
        };
        let end = if at_word {
            let mut i = index + 1;
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            i
        } else {
            let mut i = index + 1;
            while i < bytes.len() && !is_word(bytes[i]) && bytes[i] != b'\n' {
                i += 1;
            }
            i
        };
        (start, end.min(text.len()))
    }

    fn line_at(&self, index: usize) -> (usize, usize) {
        let text = self.text();
        if text.is_empty() {
            return (0, 0);
        }
        let index = index.min(text.len());
        let start = text[..index].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = text[index..].find('\n').map(|i| index + i + 1).unwrap_or(text.len());
        (start, end)
    }

    fn apply_highlight(&mut self) {
        let start = self.anchor.min(self.active);
        let end = self.anchor.max(self.active);

        let text = self.pane.get_widget_mut(self.state.text_id).unwrap();

        if start == end {
            text.clear_highlight();
            return;
        }

        let base = self.state.content.lock().expect("stream content lock").clone();
        let mut styled = base;
        styled.style_range(start..end, |s| *s = Style::new().reverse());
        text.set_content(styled);
    }

    fn copy_selection(&self) {
        let start = self.anchor.min(self.active);
        let end = self.anchor.max(self.active);
        if start != end {
            let text = self.pane.get_widget(self.state.text_id).unwrap();
            let selected = text.get_str()[start..end].to_string();
            tuie::clipboard::write_string(selected);
        }
    }
}

impl DelegateWidget for StreamingText {
    tuie::delegate_widget!(pane);

    fn override_is_focusable(&self) -> bool {
        true
    }

    fn override_on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        let Some(event) = queue.peek() else {
            return InputResult::Rejected;
        };

        let click = event.click_cycle(3);

        match &event.chord {
            chord!(LeftClick) if click == 1 => {
                queue.next();
                tuie::focus_widget(self.get_id());
                self.state.set_auto_scroll(&mut *self.pane, false);
                let index = self.mouse_to_index(event.cell());
                self.selecting = true;
                self.anchor = index;
                self.active = index;
                self.apply_highlight();
                InputResult::Handled
            }
            chord!(LeftClick) if click == 2 => {
                queue.next();
                tuie::focus_widget(self.get_id());
                self.state.set_auto_scroll(&mut *self.pane, false);
                let index = self.mouse_to_index(event.cell());
                let (start, end) = self.word_at(index);
                self.selecting = true;
                self.anchor = start;
                self.active = end;
                self.apply_highlight();
                InputResult::Handled
            }
            chord!(LeftClick) if click == 3 => {
                queue.next();
                tuie::focus_widget(self.get_id());
                self.state.set_auto_scroll(&mut *self.pane, false);
                let index = self.mouse_to_index(event.cell());
                let (start, end) = self.line_at(index);
                self.selecting = true;
                self.anchor = start;
                self.active = end;
                self.apply_highlight();
                InputResult::Handled
            }
            chord!(LeftDrag) => {
                queue.next();
                if self.selecting {
                    let index = self.mouse_to_index(event.cell());
                    self.active = index;
                    self.apply_highlight();
                }
                InputResult::Handled
            }
            chord!(LeftRelease) => {
                queue.next();
                if self.selecting {
                    self.selecting = false;
                    self.apply_highlight();
                    if self.copy_on_select {
                        self.copy_selection();
                    }
                }
                InputResult::Handled
            }
            chord!(Ctrl + c) | chord!(Super + c) | chord!('y') => {
                queue.next();
                if self.anchor != self.active {
                    self.copy_selection();
                    return InputResult::Handled;
                }
                InputResult::Rejected
            }
            _ => InputResult::Rejected,
        }
    }
}
