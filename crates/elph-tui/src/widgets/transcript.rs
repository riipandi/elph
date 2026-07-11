//! Scrollable transcript backed by tuie [`StreamingText`].

use std::sync::{Arc, Mutex};

use crate::keymap::ShellAction;
use crate::theme::Theme;
use crate::widgets::streaming_text::{StreamState, StreamingText};
use tuie::prelude::*;

const LINE_SCROLL_STEP: i32 = 3;

fn lines_to_styled(lines: &[String]) -> StyledString {
    let mut out = StyledString::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push_str("\n");
        }
        out.push_str(line.as_str());
    }
    out
}

/// Stable handles for shell-side transcript control.
#[derive(Clone)]
pub(crate) struct TranscriptHandles {
    pub stream: StreamState,
    pub viewport_id: WidgetId<Pane>,
    pub cache: Arc<Mutex<Vec<String>>>,
}

/// Transcript viewport using incremental [`StreamingText`] updates.
pub struct TranscriptPane {
    root: Box<Pane>,
    stream: StreamState,
    cached_lines: Arc<Mutex<Vec<String>>>,
}

impl TranscriptPane {
    pub fn new(_theme: Theme) -> Box<Self> {
        let stream_widget = StreamingText::new(StyledString::new());
        let stream = stream_widget.stream_state();

        let root = Pane::new()
            .flex(1)
            .y_scroll(Scrollbar::AutoHide)
            .child(stream_widget as Box<dyn Widget>);

        Box::new(Self {
            root,
            stream,
            cached_lines: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub(crate) fn handles(&self) -> TranscriptHandles {
        TranscriptHandles {
            stream: self.stream.clone(),
            viewport_id: self.root.get_id(),
            cache: Arc::clone(&self.cached_lines),
        }
    }

    fn cached(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        self.cached_lines.lock().expect("transcript cache lock")
    }

    /// Replaces the transcript and follows the tail when `follow` is true.
    pub fn set_lines(&mut self, lines: Vec<String>) {
        {
            let mut cache = self.cached();
            if lines == *cache {
                return;
            }
            *cache = lines.clone();
        }
        let styled = lines_to_styled(&lines);
        self.stream.set_content(&mut *self.root, styled);
    }

    /// Replaces transcript content and forces follow-scroll.
    pub fn set_lines_follow(&mut self, lines: Vec<String>) {
        self.stream.set_auto_scroll(&mut *self.root, true);
        self.set_lines(lines);
        self.jump_tail();
    }

    /// Appends only the tail lines that are not yet cached.
    pub fn sync_lines(&mut self, lines: Vec<String>, running: bool) {
        let handles = self.handles();
        handles.sync_lines(&mut *self.root, lines, running);
    }

    pub fn set_auto_scroll(&mut self, auto_scroll: bool) {
        self.stream.set_auto_scroll(&mut *self.root, auto_scroll);
    }

    pub fn auto_scroll(&self) -> bool {
        self.stream.is_auto_scrolling()
    }

    pub fn scroll_up(&mut self, step: usize) {
        self.set_auto_scroll(false);
        self.root.scroll_by(-(step as i32).max(LINE_SCROLL_STEP));
    }

    pub fn scroll_down(&mut self, step: usize) {
        self.root.scroll_by(step as i32);
        if self.root.get_scroll_progress(Axis2D::Y) >= 0.98 {
            self.set_auto_scroll(true);
        }
    }

    pub fn jump_tail(&mut self) {
        self.set_auto_scroll(true);
        tuie::reveal(self.stream.text_id, Vec2::new(None, Some(Align::End)));
        self.root.set_scroll_progress(Axis2D::Y, 1.0);
    }

    pub fn handle_shell_action(&mut self, action: ShellAction) -> bool {
        self.handles().handle_shell_action(&mut *self.root, action)
    }
}

impl TranscriptHandles {
    pub(crate) fn set_lines(&self, root: &mut dyn Widget, lines: Vec<String>) {
        let mut cache = self.cache.lock().expect("transcript cache lock");
        if lines == *cache {
            return;
        }
        *cache = lines;
        let styled = lines_to_styled(&cache);
        self.stream.set_content(root, styled);
    }

    pub(crate) fn sync_lines(&self, root: &mut dyn Widget, lines: Vec<String>, running: bool) {
        let cache_len = self.cache.lock().expect("transcript cache lock").len();
        if lines.len() < cache_len {
            self.set_lines(root, lines);
            if running {
                self.stream.set_auto_scroll(root, true);
            }
            return;
        }

        if lines.len() > cache_len {
            let new_tail = &lines[cache_len..];
            for line in new_tail {
                let mut chunk = line.clone();
                chunk.push('\n');
                self.stream.append(root, chunk.as_str());
            }
            if running {
                self.stream.set_auto_scroll(root, true);
            }
            *self.cache.lock().expect("transcript cache lock") = lines;
            return;
        }

        let cached = self.cache.lock().expect("transcript cache lock").clone();
        if lines != cached {
            if running
                && lines.len() == cached.len()
                && !lines.is_empty()
                && lines[..lines.len() - 1] == cached[..cached.len() - 1]
            {
                self.replace_last_line(root, lines);
            } else if running {
                self.set_lines_follow(root, lines);
            } else {
                self.set_lines(root, lines);
            }
            return;
        }

        if running {
            self.jump_tail(root);
        }
    }

    fn set_lines_follow(&self, root: &mut dyn Widget, lines: Vec<String>) {
        self.stream.set_auto_scroll(root, true);
        self.set_lines(root, lines);
        self.jump_tail(root);
    }

    fn replace_last_line(&self, root: &mut dyn Widget, lines: Vec<String>) {
        let last = lines.last().cloned().unwrap_or_default();
        let prev = self
            .cache
            .lock()
            .expect("transcript cache lock")
            .last()
            .cloned()
            .unwrap_or_default();
        if last == prev {
            return;
        }
        let plain = self.stream.plain_text();
        if let Some(pos) = plain.rfind('\n') {
            let keep = &plain[..=pos];
            self.stream.set_content(root, StyledString::from(keep));
            self.stream.append(root, format!("{last}\n").as_str());
        } else {
            self.stream.set_content(root, StyledString::new());
            self.stream.append(root, format!("{last}\n").as_str());
        }
        self.stream.set_auto_scroll(root, true);
        tuie::reveal(self.stream.text_id, Vec2::new(None, Some(Align::End)));
        *self.cache.lock().expect("transcript cache lock") = lines;
    }

    fn jump_tail(&self, root: &mut dyn Widget) {
        self.stream.set_auto_scroll(root, true);
        tuie::reveal(self.stream.text_id, Vec2::new(None, Some(Align::End)));
        if let Some(viewport) = root.get_widget_mut(self.viewport_id) {
            viewport.set_scroll_progress(Axis2D::Y, 1.0);
        }
    }

    pub(crate) fn handle_shell_action(&self, root: &mut dyn Widget, action: ShellAction) -> bool {
        match action {
            ShellAction::TranscriptScrollUp => {
                self.stream.set_auto_scroll(root, false);
                if let Some(viewport) = root.get_widget_mut(self.viewport_id) {
                    viewport.scroll_by(-LINE_SCROLL_STEP);
                }
                true
            }
            ShellAction::TranscriptScrollDown => {
                if let Some(viewport) = root.get_widget_mut(self.viewport_id) {
                    viewport.scroll_by(LINE_SCROLL_STEP);
                    if viewport.get_scroll_progress(Axis2D::Y) >= 0.98 {
                        self.stream.set_auto_scroll(root, true);
                    }
                }
                true
            }
            ShellAction::TranscriptJumpTail => {
                self.jump_tail(root);
                true
            }
            _ => false,
        }
    }
}

impl DelegateWidget for TranscriptPane {
    tuie::delegate_widget!(root);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuie::emulator::Emulator;

    #[test]
    fn set_lines_updates_cached_payload_and_renders() {
        let mut pane = TranscriptPane::new(Theme::dark());
        pane.set_lines(vec!["one".into(), "two".into()]);
        assert_eq!(pane.cached().len(), 2);
        assert!(pane.auto_scroll());

        let term = Emulator::new(pane.as_mut(), Vec2::new(40, 8));
        let snap = term.get_snapshot_text();
        assert!(snap.contains("one"), "expected rendered text in:\n{snap}");
        assert!(snap.contains("two"), "expected rendered text in:\n{snap}");
    }

    #[test]
    fn sync_lines_appends_only_new_tail() {
        let mut pane = TranscriptPane::new(Theme::dark());
        pane.set_lines(vec!["one".into()]);
        pane.sync_lines(vec!["one".into(), "two".into()], true);
        assert_eq!(&*pane.cached(), &vec!["one".to_string(), "two".to_string()]);

        let term = Emulator::new(pane.as_mut(), Vec2::new(40, 8));
        let snap = term.get_snapshot_text();
        assert!(snap.contains("two"), "expected appended line in:\n{snap}");
    }

    #[test]
    fn scroll_up_disables_auto_scroll() {
        let mut pane = TranscriptPane::new(Theme::dark());
        pane.set_lines((0..20).map(|i| format!("line {i}")).collect());
        pane.scroll_up(1);
        assert!(!pane.auto_scroll());
    }

    #[test]
    fn jump_tail_re_enables_auto_scroll() {
        let mut pane = TranscriptPane::new(Theme::dark());
        pane.set_lines(vec!["a".into()]);
        pane.scroll_up(1);
        assert!(!pane.auto_scroll());
        pane.jump_tail();
        assert!(pane.auto_scroll());
    }

    #[test]
    fn shell_actions_are_consumed() {
        let mut pane = TranscriptPane::new(Theme::dark());
        pane.set_lines(vec!["a".into(), "b".into()]);
        assert!(pane.handle_shell_action(ShellAction::TranscriptScrollUp));
        assert!(!pane.handle_shell_action(ShellAction::ToggleSidebar));
    }

    #[test]
    fn transcript_has_no_border() {
        let mut pane = TranscriptPane::new(Theme::dark());
        let term = Emulator::new(pane.as_mut(), Vec2::new(40, 8));
        let snap = term.get_snapshot_text();
        assert!(
            !snap.contains('┌') && !snap.contains('│') && !snap.contains('─'),
            "transcript should not draw a border:\n{snap}"
        );
    }
}
