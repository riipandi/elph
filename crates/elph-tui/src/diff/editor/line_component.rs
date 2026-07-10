use crate::diff::component::{InputResult, Line, LineComponent};
use crate::diff::keybindings::{EditorAction, match_editor_action};

use super::Editor;

impl LineComponent for Editor {
    fn render(&mut self, width: u16) -> Vec<Line> {
        self.last_width = width;
        let key = (self.cursor, width, self.scroll_row);
        if self.cache_key == Some(key) && !self.cache_lines.is_empty() {
            return self.cache_lines.clone();
        }
        let lines = self.build_lines(width);
        self.cache_key = Some(key);
        self.cache_lines = lines.clone();
        lines
    }

    fn invalidate(&mut self) {
        self.cache_key = None;
        self.cache_lines.clear();
    }

    fn handle_input(&mut self, data: &str) -> InputResult {
        if !self.focused {
            return InputResult::Ignored;
        }
        if let Some(action) = match_editor_action(data) {
            return self.handle_action(action, data);
        }
        if data.len() > 1 && !data.starts_with('\x1b') {
            return self.handle_action(EditorAction::InsertText, data);
        }
        InputResult::Ignored
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.invalidate();
    }

    fn is_focused(&self) -> bool {
        self.focused
    }
}
