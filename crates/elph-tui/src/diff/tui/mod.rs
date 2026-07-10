mod focus;
mod overlays;

use std::time::{Duration, Instant};

use crate::diff::component::{Container, Line, LineComponent};
use crate::diff::overlay::{OverlayEntry, OverlayHandle, OverlayOptions, composite_overlays};
use crate::diff::render::{RenderState, do_render};
use crate::diff::stdin_buffer::{InputEvent, StdinBuffer};
use crate::diff::terminal::Terminal;

use focus::{FocusState, FocusTarget};

const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(16);

/// Main diff-TUI engine.
pub struct DiffTui {
    container: Container,
    overlays: Vec<OverlayEntry>,
    terminal: Box<dyn Terminal>,
    render_state: RenderState,
    stdin_buffer: StdinBuffer,
    render_requested: bool,
    last_render_at: Option<Instant>,
    stopped: bool,
    focus: FocusState,
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
            focus: FocusState::new(),
        }
    }

    pub fn add_child(&mut self, child: Box<dyn LineComponent>) {
        let idx = self.container.len();
        self.container.add_child(child);
        if !matches!(self.focus.focused, FocusTarget::Overlay(_)) {
            self.set_focus(FocusTarget::Container(idx));
        }
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

    pub fn start(&mut self) -> anyhow::Result<()> {
        self.stopped = false;
        self.terminal.start(Box::new(|_| {}), Box::new(|| {}))?;
        self.request_render(true);
        self.pump_render()?;
        Ok(())
    }

    pub fn stop(&mut self) -> anyhow::Result<()> {
        self.stopped = true;
        self.terminal.stop()
    }

    /// Show an overlay and optionally capture focus.
    pub fn show_overlay(&mut self, component: Box<dyn LineComponent>, options: OverlayOptions) -> OverlayHandle {
        let result = overlays::show_overlay(
            &mut self.focus,
            &mut self.overlays,
            self.terminal.columns(),
            self.terminal.rows(),
            component,
            options,
        );
        if result.capture_focus {
            self.set_focus(FocusTarget::Overlay(result.handle.slot));
        }
        self.request_render(false);
        result.handle
    }

    /// Permanently remove an overlay.
    pub fn hide_overlay(&mut self, handle: OverlayHandle) -> bool {
        let result = overlays::hide_overlay(
            &mut self.focus,
            &mut self.overlays,
            self.terminal.columns(),
            self.terminal.rows(),
            handle,
        );
        if result.removed {
            if let Some(target) = result.focus_change {
                self.set_focus(target);
            }
            self.request_render(false);
        }
        result.removed
    }

    /// Temporarily hide or show an overlay.
    pub fn set_overlay_hidden(&mut self, handle: OverlayHandle, hidden: bool) {
        if let Some(target) = overlays::set_overlay_hidden(
            &mut self.focus,
            &mut self.overlays,
            self.terminal.columns(),
            self.terminal.rows(),
            handle,
            hidden,
        ) {
            self.set_focus(target);
            self.request_render(false);
        }
    }

    /// Focus an overlay and bring it to the visual front.
    pub fn focus_overlay(&mut self, handle: OverlayHandle) {
        if let Some(target) = overlays::focus_overlay(
            &mut self.focus,
            &mut self.overlays,
            self.terminal.columns(),
            self.terminal.rows(),
            handle,
        ) {
            self.set_focus(target);
            self.request_render(false);
        }
    }

    /// Process pending render if the minimum interval has elapsed.
    pub fn pump_render(&mut self) -> anyhow::Result<()> {
        if self.stopped || !self.render_requested {
            return Ok(());
        }

        let now = Instant::now();
        if let Some(last) = self.last_render_at
            && now.duration_since(last) < MIN_RENDER_INTERVAL
        {
            return Ok(());
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
                    consumed |= self
                        .focus
                        .dispatch_input(&paste, &mut self.container, &mut self.overlays);
                }
                InputEvent::Key(key) => {
                    consumed |= self.focus.dispatch_input(&key, &mut self.container, &mut self.overlays);
                }
            }
        }
        if consumed {
            self.request_render(false);
            let _ = self.pump_render();
        }
        consumed
    }

    fn set_focus(&mut self, target: FocusTarget) {
        self.focus.set_focus(target, &mut self.container, &mut self.overlays);
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
