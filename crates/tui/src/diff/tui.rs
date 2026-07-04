use std::time::{Duration, Instant};

use super::component::{Container, InputResult, Line, LineComponent};
use super::overlay::{OverlayEntry, OverlayHandle, OverlayOptions, composite_overlays};
use super::render::{RenderState, do_render};
use super::stdin_buffer::{InputEvent, StdinBuffer};
use super::terminal::Terminal;

const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusTarget {
    None,
    Overlay(usize),
}

/// Main diff-TUI engine (pi-tui `TUI`).
pub struct DiffTui {
    container: Container,
    overlays: Vec<OverlayEntry>,
    terminal: Box<dyn Terminal>,
    render_state: RenderState,
    stdin_buffer: StdinBuffer,
    render_requested: bool,
    last_render_at: Option<Instant>,
    stopped: bool,
    focused: FocusTarget,
    focus_order_counter: u64,
}

impl DiffTui {
    pub fn new(terminal: Box<dyn Terminal>) -> Self {
        Self {
            container: Container::new(),
            overlays: Vec::new(),
            terminal,
            render_state: RenderState::default(),
            stdin_buffer: StdinBuffer::default(),
            render_requested: false,
            last_render_at: None,
            stopped: false,
            focused: FocusTarget::None,
            focus_order_counter: 0,
        }
    }

    pub fn add_child(&mut self, child: Box<dyn LineComponent>) {
        self.container.add_child(child);
    }

    pub fn clear_children(&mut self) {
        self.container.clear();
    }

    pub fn full_redraws(&self) -> u32 {
        self.render_state.full_redraw_count
    }

    pub fn has_overlay(&self) -> bool {
        let w = self.terminal.columns();
        let h = self.terminal.rows();
        self.overlays.iter().any(|entry| entry.is_visible(w, h))
    }

    pub fn set_clear_on_shrink(&mut self, enabled: bool) {
        self.render_state.set_clear_on_shrink(enabled);
    }

    pub fn request_render(&mut self, force: bool) {
        if force {
            self.render_state.reset();
        }
        self.render_requested = true;
    }

    pub fn start(&mut self) -> std::io::Result<()> {
        self.stopped = false;
        self.terminal.start(Box::new(|_| {}), Box::new(|| {}))?;
        self.request_render(true);
        self.pump_render()?;
        Ok(())
    }

    pub fn stop(&mut self) -> std::io::Result<()> {
        self.stopped = true;
        self.terminal.stop()
    }

    /// Show an overlay and optionally capture focus.
    pub fn show_overlay(&mut self, component: Box<dyn LineComponent>, options: OverlayOptions) -> OverlayHandle {
        self.focus_order_counter += 1;
        let pre_focus = match self.focused {
            FocusTarget::Overlay(idx) => Some(idx),
            FocusTarget::None => None,
        };
        let slot = self.overlays.len();
        let entry = OverlayEntry {
            component,
            options: options.clone(),
            pre_focus,
            hidden: false,
            alive: true,
            focus_order: self.focus_order_counter,
        };
        let visible = entry.is_visible(self.terminal.columns(), self.terminal.rows());
        self.overlays.push(entry);

        if visible && !options.non_capturing {
            self.set_focus(FocusTarget::Overlay(slot));
        }
        self.request_render(false);
        OverlayHandle { slot }
    }

    /// Permanently remove an overlay.
    pub fn hide_overlay(&mut self, handle: OverlayHandle) -> bool {
        let Some(entry) = self.overlays.get_mut(handle.slot) else {
            return false;
        };
        if !entry.alive {
            return false;
        }

        entry.alive = false;
        let pre_focus = entry.pre_focus;
        if self.focused == FocusTarget::Overlay(handle.slot) {
            let fallback = self
                .topmost_visible_overlay()
                .or(pre_focus)
                .map(FocusTarget::Overlay)
                .unwrap_or(FocusTarget::None);
            self.set_focus(fallback);
        }
        self.request_render(false);
        true
    }

    /// Temporarily hide or show an overlay.
    pub fn set_overlay_hidden(&mut self, handle: OverlayHandle, hidden: bool) {
        let Some(entry) = self.overlays.get_mut(handle.slot) else {
            return;
        };
        if !entry.alive || entry.hidden == hidden {
            return;
        }
        entry.hidden = hidden;
        let pre_focus = entry.pre_focus;
        if hidden && self.focused == FocusTarget::Overlay(handle.slot) {
            let fallback = self
                .topmost_visible_overlay()
                .or(pre_focus)
                .map(FocusTarget::Overlay)
                .unwrap_or(FocusTarget::None);
            self.set_focus(fallback);
        } else if !hidden && !entry.options.non_capturing {
            self.focus_order_counter += 1;
            entry.focus_order = self.focus_order_counter;
            self.set_focus(FocusTarget::Overlay(handle.slot));
        }
        self.request_render(false);
    }

    /// Focus an overlay and bring it to the visual front.
    pub fn focus_overlay(&mut self, handle: OverlayHandle) {
        let visible = self
            .overlays
            .get(handle.slot)
            .is_some_and(|e| e.alive && e.is_visible(self.terminal.columns(), self.terminal.rows()));
        if !visible {
            return;
        }
        self.focus_order_counter += 1;
        if let Some(entry) = self.overlays.get_mut(handle.slot) {
            entry.focus_order = self.focus_order_counter;
        }
        self.set_focus(FocusTarget::Overlay(handle.slot));
        self.request_render(false);
    }

    /// Process pending render if the minimum interval has elapsed.
    pub fn pump_render(&mut self) -> std::io::Result<()> {
        if self.stopped || !self.render_requested {
            return Ok(());
        }

        let now = Instant::now();
        if let Some(last) = self.last_render_at {
            if now.duration_since(last) < MIN_RENDER_INTERVAL {
                return Ok(());
            }
        }

        self.render_requested = false;
        self.last_render_at = Some(now);
        self.do_render_internal();
        Ok(())
    }

    /// Dispatch raw terminal input to the focused component.
    pub fn handle_input(&mut self, data: &str) -> bool {
        let mut consumed = false;
        for event in self.stdin_buffer.push(data) {
            match event {
                InputEvent::Paste(paste) => {
                    consumed |= self.dispatch_input(&paste);
                }
                InputEvent::Key(key) => {
                    consumed |= self.dispatch_input(&key);
                }
            }
        }
        if consumed {
            self.request_render(false);
            let _ = self.pump_render();
        }
        consumed
    }

    fn dispatch_input(&mut self, data: &str) -> bool {
        if let FocusTarget::Overlay(idx) = self.focused {
            if let Some(entry) = self.overlays.get_mut(idx) {
                if entry.alive && !entry.hidden {
                    return entry.component.handle_input(data) == InputResult::Consumed;
                }
            }
        }
        false
    }

    fn set_focus(&mut self, target: FocusTarget) {
        if let FocusTarget::Overlay(idx) = self.focused {
            if let Some(entry) = self.overlays.get_mut(idx) {
                entry.component.set_focused(false);
            }
        }

        self.focused = target;

        if let FocusTarget::Overlay(idx) = self.focused {
            if let Some(entry) = self.overlays.get_mut(idx) {
                entry.component.set_focused(true);
            }
        }
    }

    fn topmost_visible_overlay(&self) -> Option<usize> {
        let w = self.terminal.columns();
        let h = self.terminal.rows();
        self.overlays
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.alive && !entry.hidden && !entry.options.non_capturing && entry.is_visible(w, h))
            .max_by_key(|(_, entry)| entry.focus_order)
            .map(|(idx, _)| idx)
    }

    fn collect_lines(&mut self) -> Vec<Line> {
        let width = self.terminal.columns();
        let height = self.terminal.rows();
        let lines = self.container.render_children(width);
        composite_overlays(lines, &mut self.overlays[..], width, height)
    }

    fn do_render_internal(&mut self) {
        let lines = self.collect_lines();
        do_render(self.terminal.as_mut(), &mut self.render_state, &lines);
    }
}
