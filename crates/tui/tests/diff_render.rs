use elph_tui::{LineComponent, RecordingTerminal, Terminal, TextBlock};

struct TestHarness {
    terminal: RecordingTerminal,
    block: TextBlock,
    state: elph_tui::RenderState,
}

impl TestHarness {
    fn new(lines: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            terminal: RecordingTerminal::new(40, 10),
            block: TextBlock::new(lines),
            state: elph_tui::RenderState::default(),
        }
    }

    fn render(&mut self) {
        let width = self.terminal.columns();
        let lines = self.block.render(width);
        elph_tui::do_render(&mut self.terminal, &mut self.state, &lines);
    }

    fn set_lines(&mut self, lines: impl IntoIterator<Item = impl Into<String>>) {
        self.block.set_lines(lines);
    }

    fn viewport(&self) -> Vec<String> {
        self.terminal.viewport()
    }

    fn writes(&self) -> &str {
        self.terminal.get_writes()
    }

    fn clear_writes(&mut self) {
        self.terminal.clear_writes();
    }
}

#[test]
fn spinner_updates_middle_line_without_full_clear() {
    let mut h = TestHarness::new(vec!["Header", "Working...", "Footer"]);
    h.render();
    h.clear_writes();

    for frame in ["|", "/", "-", "\\"] {
        h.set_lines(vec!["Header", &format!("Working {frame}"), "Footer"]);
        h.render();
    }

    assert!(
        !h.writes().contains("\x1b[2J"),
        "spinner tick should not full-clear, got: {}",
        h.writes()
    );

    let vp = h.viewport();
    assert!(vp[0].contains("Header"), "header: {:?}", vp[0]);
    assert!(vp[1].contains("Working"), "spinner: {:?}", vp[1]);
    assert!(vp[2].contains("Footer"), "footer: {:?}", vp[2]);
}

#[test]
fn width_change_triggers_full_redraw() {
    let mut h = TestHarness::new(vec!["Line 0", "Line 1"]);
    h.render();
    let redraws_before = h.state.full_redraw_count;
    h.terminal.columns = 60;
    h.render();
    assert!(h.state.full_redraw_count > redraws_before);
    assert!(h.writes().contains("\x1b[2J"));
}

#[test]
fn shrink_with_clear_on_shrink_redraws() {
    let mut h = TestHarness::new(vec!["Line 0", "Line 1", "Line 2", "Line 3"]);
    h.state.set_clear_on_shrink(true);
    h.render();
    let before = h.state.full_redraw_count;
    h.set_lines(vec!["Line 0"]);
    h.render();
    assert!(h.state.full_redraw_count > before);
}

#[test]
fn append_after_shrink_stays_differential() {
    let mut h = TestHarness::new((0..8).map(|i| format!("Line {i}")).collect::<Vec<_>>());
    h.render();
    h.set_lines(vec!["Line 0", "Line 1"]);
    h.render();
    let after_shrink = h.state.full_redraw_count;
    h.clear_writes();
    h.set_lines(vec!["Line 0", "Line 1", "Line 2"]);
    h.render();
    assert_eq!(h.state.full_redraw_count, after_shrink);
    assert!(!h.writes().contains("\x1b[2J"));
}
