//! tuie-based agent shell composition.

use std::cell::RefCell;
use std::rc::Rc;
#[cfg(not(test))]
use std::time::Duration;

use tuie::prelude::*;

use crate::keymap::{GlobalChordHandler, SIDEBAR_MIN_TOTAL_WIDTH, SIDEBAR_WIDTH, ShellAction, ShellActionSink};
use crate::shell::host::{ShellChromeData, ShellHost};
use crate::theme::apply_tuie_theme;
use crate::widgets::{
    ActivityHandles, ActivityPane, CommandPaletteState, FooterHandles, FooterPane, PromptHandles, PromptPane,
    SidebarPlaceholder, TranscriptHandles, TranscriptPane, build_palette_widget, close_palette_popup,
    filtered_command_count, open_palette_popup, palette_selection_text, palette_visible,
};

/// Whether host prompt text should overwrite the tuie input widget.
#[cfg(test)]
fn should_sync_prompt_from_host(widget_text: &str, host_text: &str) -> bool {
    widget_text.is_empty() || widget_text == host_text
}

fn sidebar_visible(chrome: &ShellChromeData) -> bool {
    chrome.sidebar_open && tuie::get_runtime_info().size.x >= SIDEBAR_MIN_TOTAL_WIDTH
}

pub(crate) fn focus_prompt_input(input_id: WidgetId<Input>) {
    let target = input_id.untyped();
    if tuie::get_focused_widget() == Some(target) {
        return;
    }
    tuie::focus_widget(input_id);
    if tuie::get_focused_widget() == Some(target) {
        return;
    }
    for _ in 0..32 {
        if tuie::get_focused_widget() == Some(target) {
            return;
        }
        tuie::focus_next_tab_order(Sign::Positive);
    }
}

/// Stable shell layout: transcript, chrome stack, and optional sidebar.
struct ShellLayout {
    root: Box<dyn Widget>,
    transcript: TranscriptHandles,
    activity: ActivityHandles,
    prompt: PromptHandles,
    footer: FooterHandles,
    sidebar_open: bool,
}

impl ShellLayout {
    fn new(theme: crate::theme::Theme, chrome: &ShellChromeData, lines: &[String], prompt: &str) -> Self {
        let mut transcript = TranscriptPane::new(theme);
        transcript.set_lines(lines.to_vec());
        let transcript_handles = transcript.handles();

        let mut activity = ActivityPane::new(theme);
        activity.sync(chrome, theme);
        let activity_handles = activity.handles();

        let mut prompt_widget = PromptPane::new(theme);
        prompt_widget.set_content(prompt);
        prompt_widget.set_running(chrome.running);
        let prompt_handles = prompt_widget.handles();

        let mut footer = FooterPane::new(theme);
        footer.sync(chrome, theme);
        let footer_handles = footer.handles();

        let bottom = Pane::new().vertical().gap(0).children([
            activity as Box<dyn Widget>,
            prompt_widget as Box<dyn Widget>,
            footer as Box<dyn Widget>,
        ]);

        let main = Pane::new()
            .vertical()
            .children([transcript.flex(1) as Box<dyn Widget>, bottom]);

        let show_sidebar = sidebar_visible(chrome);
        let root = Self::wrap_sidebar(main, theme, show_sidebar);

        Self {
            root,
            transcript: transcript_handles,
            activity: activity_handles,
            prompt: prompt_handles,
            footer: footer_handles,
            sidebar_open: show_sidebar,
        }
    }

    fn wrap_sidebar(main: Box<Pane>, theme: crate::theme::Theme, visible: bool) -> Box<dyn Widget> {
        if visible {
            let sidebar = SidebarPlaceholder::new(theme)
                .min_width(SIDEBAR_WIDTH)
                .max_width(SIDEBAR_WIDTH);
            Split::new(
                SplitPane::new()
                    .horizontal()
                    .children([SplitPaneChild::from(main.flex(1)), SplitPaneChild::from(sidebar)]),
            )
        } else {
            main as Box<dyn Widget>
        }
    }

    fn sync_sidebar_visibility(&mut self, theme: crate::theme::Theme, chrome: &ShellChromeData, lines: &[String]) {
        let want = sidebar_visible(chrome);
        if want == self.sidebar_open {
            return;
        }

        let prompt = PromptPane::content_in(&*self.root, &self.prompt);

        let mut transcript = TranscriptPane::new(theme);
        transcript.set_lines(lines.to_vec());
        let transcript_handles = transcript.handles();

        let mut activity = ActivityPane::new(theme);
        activity.sync(chrome, theme);
        let activity_handles = activity.handles();

        let mut prompt_widget = PromptPane::new(theme);
        prompt_widget.set_content(&prompt);
        prompt_widget.set_running(chrome.running);
        let prompt_handles = prompt_widget.handles();

        let mut footer = FooterPane::new(theme);
        footer.sync(chrome, theme);
        let footer_handles = footer.handles();

        let bottom = Pane::new().vertical().gap(0).children([
            activity as Box<dyn Widget>,
            prompt_widget as Box<dyn Widget>,
            footer as Box<dyn Widget>,
        ]);
        let main = Pane::new()
            .vertical()
            .children([transcript.flex(1) as Box<dyn Widget>, bottom]);

        self.root = Self::wrap_sidebar(main, theme, want);
        self.transcript = transcript_handles;
        self.activity = activity_handles;
        self.prompt = prompt_handles;
        self.footer = footer_handles;
        self.sidebar_open = want;
        self.root.dirty_layout();
    }

    fn prompt_text(&self) -> String {
        PromptPane::content_in(&*self.root, &self.prompt)
    }
}

impl DelegateWidget for ShellLayout {
    tuie::delegate_widget!(root);
}

/// Main agent shell widget: transcript, chrome stack, and optional sidebar.
pub struct AgentShell {
    layout: Box<ShellLayout>,
    host: Rc<RefCell<dyn ShellHost>>,
    action_sink: ShellActionSink,
    poll_task: TaskHandle,
    palette_state: CommandPaletteState,
    palette_popup_id: Option<WidgetId<List>>,
    palette_visible: bool,
    palette_revision: (String, usize, bool),
}

impl AgentShell {
    /// Builds the shell wrapped in [`GlobalChordHandler`].
    #[allow(clippy::new_ret_no_self)]
    pub fn new(host: Rc<RefCell<dyn ShellHost>>) -> Box<dyn Widget> {
        let action_sink = ShellActionSink::default();
        let shell = Self::build(host, action_sink.clone());
        GlobalChordHandler::new(shell, action_sink)
    }

    #[cfg(test)]
    pub(crate) fn prompt_handles(&self) -> &PromptHandles {
        &self.layout.prompt
    }

    fn build(host: Rc<RefCell<dyn ShellHost>>, action_sink: ShellActionSink) -> Box<Self> {
        let (theme, chrome, lines, prompt, commands) = {
            let host_ref = host.borrow();
            (
                host_ref.theme(),
                host_ref.chrome(),
                host_ref.transcript_lines(),
                host_ref.prompt_text(),
                host_ref.commands(),
            )
        };
        let _ = apply_tuie_theme(theme);

        let layout = Box::new(ShellLayout::new(theme, &chrome, &lines, &prompt));

        let mut shell = Box::new(Self {
            layout,
            host,
            action_sink,
            poll_task: TaskHandle::EMPTY,
            palette_state: CommandPaletteState::default(),
            palette_popup_id: None,
            palette_visible: false,
            palette_revision: (String::new(), 0, false),
        });

        let shell_id = shell.get_id();
        #[cfg(not(test))]
        {
            shell.poll_task = tuie::schedule(shell_id, Duration::from_millis(33), |shell| {
                shell.poll_host();
            });
        }
        #[cfg(test)]
        {
            let _ = shell_id;
            shell.poll_task = TaskHandle::EMPTY;
        }

        if chrome.palette_open && (shell.palette_state.forced || palette_visible(&prompt)) {
            shell.open_palette(&commands, theme);
        }

        shell
    }

    fn open_palette(&mut self, commands: &[crate::diff::SlashCommand], theme: crate::theme::Theme) {
        let input = self.layout.prompt_text();
        if !self.palette_state.forced && !palette_visible(&input) {
            return;
        }
        let revision = (
            self.palette_state.filter_key.clone(),
            self.palette_state.selected,
            self.palette_state.forced,
        );
        if self.palette_popup_id.is_some() && self.palette_revision == revision {
            return;
        }
        if let Some(id) = self.palette_popup_id.take() {
            close_palette_popup(id);
        }
        let widget = build_palette_widget(commands, &input, &self.palette_state, theme);
        self.palette_popup_id = Some(open_palette_popup(widget));
        self.palette_revision = revision;
        self.palette_visible = true;
    }

    fn refresh_palette(&mut self, commands: &[crate::diff::SlashCommand], theme: crate::theme::Theme) {
        self.palette_revision = (String::new(), usize::MAX, false);
        self.open_palette(commands, theme);
    }

    fn close_palette(&mut self) {
        self.palette_visible = false;
        self.palette_state.forced = false;
        if let Some(id) = self.palette_popup_id.take() {
            close_palette_popup(id);
        }
        self.host.borrow_mut().set_palette_open(false);
    }

    fn apply_palette_selection(&mut self) {
        let input = self.layout.prompt_text();
        let commands = self.host.borrow().commands();
        let Some(cmd) = self.palette_state.selected_command(&commands, &input) else {
            return;
        };
        let text = palette_selection_text(&cmd);
        self.host.borrow_mut().set_prompt_text(text.clone());
        PromptPane::set_content_in(&mut *self.layout.root, &self.layout.prompt, &text);
        self.close_palette();
    }

    fn handle_palette_input(&mut self, queue: &mut InputQueue) -> bool {
        if !self.palette_visible {
            return false;
        }
        let Some(event) = queue.peek() else {
            return false;
        };

        let commands = self.host.borrow().commands();
        let theme = self.host.borrow().theme();
        let len = filtered_command_count(&commands, &self.layout.prompt_text(), self.palette_state.forced);

        match &event.chord {
            chord!(Up) | chord!(Ctrl + p) => {
                queue.next();
                self.palette_state.move_up(len);
                self.refresh_palette(&commands, theme);
                true
            }
            chord!(Down) | chord!(Ctrl + n) => {
                queue.next();
                self.palette_state.move_down(len);
                self.refresh_palette(&commands, theme);
                true
            }
            chord!(Tab) | chord!(Enter) => {
                queue.next();
                self.apply_palette_selection();
                true
            }
            chord!(Esc) => {
                queue.next();
                self.close_palette();
                true
            }
            _ => false,
        }
    }

    #[cfg(not(test))]
    fn sync_from_host(&mut self) {
        let (theme, chrome, lines, prompt, commands, palette_open) = {
            let host = self.host.borrow();
            (
                host.theme(),
                host.chrome(),
                host.transcript_lines(),
                host.prompt_text(),
                host.commands(),
                host.palette_open(),
            )
        };
        let _ = apply_tuie_theme(theme);

        self.layout.sync_sidebar_visibility(theme, &chrome, &lines);

        self.layout
            .transcript
            .sync_lines(&mut *self.layout.root, lines, chrome.running);
        ActivityPane::sync_in(&mut *self.layout.root, &self.layout.activity, &chrome, theme);
        FooterPane::sync_in(&mut *self.layout.root, &self.layout.footer, &chrome, theme);
        PromptPane::sync_from_host(&mut *self.layout.root, &self.layout.prompt, &prompt, chrome.running);

        let prompt = self.layout.prompt_text();
        let show_palette = palette_open && (self.palette_state.forced || palette_visible(&prompt));
        if show_palette {
            self.palette_state.sync_filter(&prompt);
            self.open_palette(&commands, theme);
        } else if self.palette_visible {
            self.close_palette();
        }
    }

    fn dispatch_shell_actions(&mut self, actions: Vec<ShellAction>) {
        for action in actions {
            if self
                .layout
                .transcript
                .handle_shell_action(&mut *self.layout.root, action)
            {
                continue;
            }

            match action {
                ShellAction::ToggleSidebar => {
                    let next = {
                        let host = self.host.borrow();
                        !host.sidebar_open()
                    };
                    self.host.borrow_mut().set_sidebar_open(next);
                    let theme = self.host.borrow().theme();
                    let chrome = self.host.borrow().chrome();
                    let lines = self.host.borrow().transcript_lines();
                    self.layout.sync_sidebar_visibility(theme, &chrome, &lines);
                }
                ShellAction::OpenPalette => {
                    self.palette_state.forced = true;
                    self.host.borrow_mut().set_palette_open(true);
                    let commands = self.host.borrow().commands();
                    let theme = self.host.borrow().theme();
                    self.open_palette(&commands, theme);
                }
                ShellAction::ToggleTheme => {
                    let mut host = self.host.borrow_mut();
                    let next = host.theme().toggle();
                    host.set_theme(next);
                    let _ = apply_tuie_theme(next);
                }
                ShellAction::Quit => tuie::quit(0),
                other => self.host.borrow_mut().on_shell_action(other),
            }
        }
    }

    fn dispatch_prompt_actions(&mut self) {
        if let Some(action) = PromptPane::take_pending(&self.layout.prompt) {
            self.host.borrow_mut().on_prompt_action(action);
        }
    }

    pub(crate) fn flush_input_side_effects(&mut self) {
        let actions = self.action_sink.take();
        if !actions.is_empty() {
            self.dispatch_shell_actions(actions);
        }
        self.dispatch_prompt_actions();
    }

    #[cfg(not(test))]
    fn poll_host(&mut self) {
        {
            let mut host = self.host.borrow_mut();
            host.poll();
            if host.should_exit() {
                tuie::quit(0);
                return;
            }
        }
        self.sync_from_host();
        self.flush_input_side_effects();
    }
}

impl DelegateWidget for AgentShell {
    tuie::delegate_widget!(layout);

    fn after_before_layout(&mut self) {
        if !self.palette_visible {
            focus_prompt_input(self.layout.prompt.input_id);
        }
    }

    fn override_on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        if self.palette_visible && self.handle_palette_input(queue) {
            return InputResult::Handled;
        }
        // Default delegate path — required for tuie focus routing to reach PromptPane.
        let result = self.get_delegate_mut().on_input(queue);
        self.dispatch_prompt_actions();
        result
    }

    fn after_on_input(&mut self, _result: InputResult) {
        self.flush_input_side_effects();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::SlashCommand;
    use crate::prompt::{PromptAction, owly_builtin_commands};
    use tuie::emulator::Emulator;

    struct MockHost {
        chrome: ShellChromeData,
        lines: Vec<String>,
        prompt: String,
        commands: Vec<SlashCommand>,
        palette_open: bool,
        submitted: Vec<PromptAction>,
        exit: bool,
        theme: crate::theme::Theme,
    }

    impl MockHost {
        fn new() -> Self {
            Self {
                chrome: ShellChromeData {
                    model_name: "test-model".into(),
                    session_id: "sess".into(),
                    ..ShellChromeData::default()
                },
                lines: vec!["› hello".into()],
                prompt: String::new(),
                commands: owly_builtin_commands(),
                palette_open: false,
                submitted: Vec::new(),
                exit: false,
                theme: crate::theme::Theme::dark(),
            }
        }
    }

    impl ShellHost for MockHost {
        fn poll(&mut self) {}

        fn should_exit(&self) -> bool {
            self.exit
        }

        fn chrome(&self) -> ShellChromeData {
            self.chrome.clone()
        }

        fn commands(&self) -> Vec<SlashCommand> {
            self.commands.clone()
        }

        fn transcript_lines(&self) -> Vec<String> {
            self.lines.clone()
        }

        fn on_shell_action(&mut self, _action: ShellAction) {}

        fn on_prompt_action(&mut self, action: PromptAction) {
            self.submitted.push(action);
        }

        fn running(&self) -> bool {
            self.chrome.running
        }

        fn sidebar_open(&self) -> bool {
            self.chrome.sidebar_open
        }

        fn set_sidebar_open(&mut self, open: bool) {
            self.chrome.sidebar_open = open;
        }

        fn palette_open(&self) -> bool {
            self.palette_open
        }

        fn set_palette_open(&mut self, open: bool) {
            self.palette_open = open;
        }

        fn theme(&self) -> crate::theme::Theme {
            self.theme
        }

        fn set_theme(&mut self, theme: crate::theme::Theme) {
            self.theme = theme;
        }

        fn prompt_text(&self) -> String {
            self.prompt.clone()
        }

        fn set_prompt_text(&mut self, text: String) {
            self.prompt = text;
        }

        fn clear_prompt(&mut self) {
            self.prompt.clear();
        }
    }

    fn build_shell(host: Rc<RefCell<MockHost>>) -> (Box<dyn Widget>, PromptHandles) {
        let host_ref: Rc<RefCell<dyn ShellHost>> = host;
        let sink = ShellActionSink::default();
        let agent = AgentShell::build(host_ref, sink.clone());
        let handles = agent.prompt_handles().clone();
        let root = GlobalChordHandler::new(agent, sink);
        (root, handles)
    }

    fn build_shell_direct(host: Rc<RefCell<MockHost>>) -> Box<AgentShell> {
        let host_ref: Rc<RefCell<dyn ShellHost>> = host;
        AgentShell::build(host_ref, ShellActionSink::default())
    }

    #[test]
    fn sync_prompt_skips_active_draft() {
        assert!(!should_sync_prompt_from_host("hello", ""));
        assert!(!should_sync_prompt_from_host("draft", "stale"));
    }

    #[test]
    fn sync_prompt_applies_when_empty_or_matching() {
        assert!(should_sync_prompt_from_host("", "restored"));
        assert!(should_sync_prompt_from_host("same", "same"));
    }

    #[test]
    fn shell_accepts_typed_characters() {
        let _ = apply_tuie_theme(crate::theme::Theme::dark());
        let host = Rc::new(RefCell::new(MockHost::new()));
        let mut root = build_shell_direct(Rc::clone(&host));
        let mut term = Emulator::new(root.as_mut(), Vec2::new(80, 20));
        focus_prompt_input(root.layout.prompt.input_id);
        term.update(root.as_mut(), &[RuntimeEvent::Resize(tuie::get_runtime_info().size)]);
        term.update(root.as_mut(), &[RuntimeEvent::from(chord!(Char('x')))]);
        term.update(root.as_mut(), &[RuntimeEvent::from(chord!(Char('y')))]);
        let prompt = PromptPane::content_in(&*root.layout.root, &root.layout.prompt);
        assert_eq!(prompt, "xy", "expected typed prompt text, got {prompt:?}");
    }

    #[test]
    fn enter_reaches_prompt_inside_vertical_layout() {
        let _ = apply_tuie_theme(crate::theme::Theme::dark());
        let mut prompt = PromptPane::new(crate::theme::Theme::dark());
        prompt.set_content("hi");
        let handles = prompt.handles();
        let mut root = Pane::new().vertical().children([prompt as Box<dyn Widget>]);
        let mut term = Emulator::new(&mut *root, Vec2::new(80, 20));
        tuie::ensure_focused();
        term.update(&mut *root, &[]);
        term.update(&mut *root, &[RuntimeEvent::from(chord!(Enter))]);
        let action = PromptPane::take_pending(&handles);
        assert!(
            matches!(&action, Some(PromptAction::Submit(s)) if s == "hi"),
            "expected submit from nested layout, got {action:?}"
        );
    }

    #[test]
    fn agent_shell_enter_without_global_handler() {
        let _ = apply_tuie_theme(crate::theme::Theme::dark());
        let host = Rc::new(RefCell::new(MockHost::new()));
        host.borrow_mut().prompt = "hi".into();
        let mut root = build_shell_direct(Rc::clone(&host));
        let mut term = Emulator::new(root.as_mut(), Vec2::new(80, 20));
        focus_prompt_input(root.layout.prompt.input_id);
        term.update(root.as_mut(), &[RuntimeEvent::Resize(tuie::get_runtime_info().size)]);
        term.update(root.as_mut(), &[RuntimeEvent::from(chord!(Enter))]);
        root.flush_input_side_effects();
        let submitted = host.borrow().submitted.clone();
        assert!(
            submitted
                .iter()
                .any(|a| matches!(a, PromptAction::Submit(s) if s == "hi")),
            "expected direct shell submit, got {submitted:?}"
        );
    }

    #[test]
    fn shell_submit_dispatches_to_host() {
        let _ = apply_tuie_theme(crate::theme::Theme::dark());
        let host = Rc::new(RefCell::new(MockHost::new()));
        host.borrow_mut().prompt = "hi".into();
        let (mut root, handles) = build_shell(Rc::clone(&host));
        let mut term = Emulator::new(&mut *root, Vec2::new(80, 20));
        if let Some(handler) = (root.as_mut() as &mut dyn std::any::Any).downcast_mut::<GlobalChordHandler>() {
            handler.prime_focus(&mut term, handles.input_id);
        }
        term.update(&mut *root, &[RuntimeEvent::from(chord!(Enter))]);
        if let Some(handler) = (root.as_mut() as &mut dyn std::any::Any).downcast_mut::<GlobalChordHandler>() {
            handler.flush_agent_effects();
        }
        let submitted = host.borrow().submitted.clone();
        assert!(
            submitted
                .iter()
                .any(|a| matches!(a, PromptAction::Submit(s) if s == "hi")),
            "expected submit action, got {submitted:?}"
        );
    }

    #[test]
    fn shell_submit_after_live_typing() {
        let _ = apply_tuie_theme(crate::theme::Theme::dark());
        let host = Rc::new(RefCell::new(MockHost::new()));
        let (mut root, handles) = build_shell(Rc::clone(&host));
        let mut term = Emulator::new(&mut *root, Vec2::new(80, 20));
        if let Some(handler) = (root.as_mut() as &mut dyn std::any::Any).downcast_mut::<GlobalChordHandler>() {
            handler.prime_focus(&mut term, handles.input_id);
        }
        for ch in ['h', 'i'] {
            term.update(&mut *root, &[RuntimeEvent::from(chord!(Char(ch)))]);
        }
        term.update(&mut *root, &[RuntimeEvent::from(chord!(Enter))]);
        if let Some(handler) = (root.as_mut() as &mut dyn std::any::Any).downcast_mut::<GlobalChordHandler>() {
            handler.flush_agent_effects();
        }
        let submitted = host.borrow().submitted.clone();
        assert!(
            submitted
                .iter()
                .any(|a| matches!(a, PromptAction::Submit(s) if s == "hi")),
            "expected submit after typing, got {submitted:?}"
        );
    }

    #[test]
    fn shell_renders_transcript_lines() {
        let _ = apply_tuie_theme(crate::theme::Theme::dark());
        let host = Rc::new(RefCell::new(MockHost::new()));
        let (mut root, _handles) = build_shell(host);
        let term = Emulator::new(&mut *root, Vec2::new(80, 20));
        let snap = term.get_snapshot_text();
        assert!(snap.contains("hello"), "expected transcript line:\n{snap}");
    }
}
