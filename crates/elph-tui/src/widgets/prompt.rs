//! Multiline prompt input for the tuie shell.

use std::any::Any;
use std::sync::{Arc, Mutex};

use crate::keymap::PromptSubmitMode;
use crate::prompt::{PromptAction, is_quit_command, strip_submit_trigger};
use crate::theme::Theme;
use crate::widgets::focus_pane::FocusPane;
use tuie::prelude::*;

const PROMPT_MIN_ROWS: u16 = 1;
const PROMPT_MAX_ROWS: u16 = 6;

fn visible_rows(text: &str) -> u16 {
    let mut lines = text.lines().count();
    if lines == 0 {
        lines = 1;
    }
    if lines == 1 && text.is_empty() {
        return PROMPT_MIN_ROWS;
    }
    (lines as u16).clamp(PROMPT_MIN_ROWS, PROMPT_MAX_ROWS)
}

fn enter_mode(chord: &Chord) -> Option<PromptSubmitMode> {
    if matches!(chord.trigger, Trigger::Key(Key::Enter)) && chord.modifiers.is_empty() {
        return Some(PromptSubmitMode::Submit);
    }
    if matches!(chord.trigger, Trigger::Key(Key::Enter)) && chord.modifiers == Modifiers::new().with(Modifier::Ctrl) {
        return Some(PromptSubmitMode::Steer);
    }
    None
}

fn should_cycle_mode(text: &str, chord: &Chord) -> bool {
    if matches!(chord.trigger, Trigger::Key(Key::Tab)) && chord.modifiers == Modifiers::new().with(Modifier::Ctrl) {
        return true;
    }
    matches!(chord.trigger, Trigger::Key(Key::Tab)) && chord.modifiers.is_empty() && text.is_empty()
}

#[derive(Default)]
pub(crate) struct PromptChordState {
    pending: Option<PromptAction>,
    running: bool,
}

/// Input bindings that map Enter/Esc/Tab to agent prompt actions.
struct AgentPromptBindings {
    inner: Box<dyn InputBindings<Text>>,
    state: Arc<Mutex<PromptChordState>>,
}

fn agent_prompt_bindings_factory() -> Box<dyn InputBindings<Text>> {
    Box::new(AgentPromptBindings {
        inner: DefaultBindings::new(),
        state: Arc::new(Mutex::new(PromptChordState::default())),
    })
}

impl AgentPromptBindings {
    fn attach_state(bindings: &mut dyn InputBindings<Text>, state: Arc<Mutex<PromptChordState>>) {
        let bindings = bindings
            .as_any_mut()
            .downcast_mut::<AgentPromptBindings>()
            .expect("prompt input must use AgentPromptBindings");
        bindings.state = state;
    }
}

impl InputBindings<Text> for AgentPromptBindings {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn configure_state(&self, state: &mut EditorState<Text>) {
        self.inner.configure_state(state);
    }

    fn on_input(&mut self, state: &mut EditorState<Text>, text: &mut Text, queue: &mut InputQueue) -> InputResult {
        let Some(event) = queue.peek() else {
            return InputResult::Rejected;
        };

        let content = text.get_str().to_string();
        let running = self.state.lock().expect("prompt chord lock").running;

        if matches!(event.chord.trigger, Trigger::Key(Key::Esc)) && !content.is_empty() && !running {
            queue.next();
            state.select_all(text);
            state.delete_char(text, Sign::Positive);
            self.state.lock().expect("prompt chord lock").pending = Some(PromptAction::Clear);
            return InputResult::Handled;
        }

        if should_cycle_mode(&content, &event.chord) && !running {
            queue.next();
            self.state.lock().expect("prompt chord lock").pending = Some(PromptAction::CycleMode);
            return InputResult::Handled;
        }

        if let Some(mode) = enter_mode(&event.chord) {
            let trimmed = content.trim();
            queue.next();
            if trimmed.is_empty() {
                return InputResult::Handled;
            }
            let submitted = strip_submit_trigger(trimmed);
            if is_quit_command(trimmed) {
                tuie::quit(0);
                return InputResult::Handled;
            }
            state.select_all(text);
            state.delete_char(text, Sign::Positive);
            let action = match mode {
                PromptSubmitMode::Submit if running => PromptAction::Queue(submitted),
                PromptSubmitMode::Submit => PromptAction::Submit(submitted),
                PromptSubmitMode::Steer if running => PromptAction::Steer(submitted),
                PromptSubmitMode::Steer => PromptAction::Submit(submitted),
            };
            self.state.lock().expect("prompt chord lock").pending = Some(action);
            return InputResult::Handled;
        }

        self.inner.on_input(state, text, queue)
    }
}

/// Stable handles for shell code to reach prompt internals (delegate ids are not typed).
#[derive(Clone)]
pub(crate) struct PromptHandles {
    pub input_id: WidgetId<Input>,
    pub scroll_id: WidgetId<Pane>,
    pub chord: Arc<Mutex<PromptChordState>>,
}

/// Bordered multiline prompt backed by tuie [`Input`] inside [`FocusPane`].
pub struct PromptPane {
    root: Box<FocusPane>,
    scroll_id: WidgetId<Pane>,
    input_id: WidgetId<Input>,
    chord_state: Arc<Mutex<PromptChordState>>,
    pending_action: Option<PromptAction>,
    focused_once: bool,
}

impl PromptPane {
    pub fn new(theme: Theme) -> Box<Self> {
        let chord_state = Arc::new(Mutex::new(PromptChordState::default()));

        let mut input_id = WidgetId::EMPTY;
        let mut scroll_id = WidgetId::EMPTY;
        let mut input = Input::new()
            .multiline()
            .word_wrap()
            .flex(1)
            .bindings(agent_prompt_bindings_factory)
            .placeholder(
                Text::new()
                    .content("Message the agent…")
                    .style(Style::new().fg(theme.input_placeholder())),
            )
            .id(&mut input_id);

        let (bindings, _) = input.get_bindings_mut();
        AgentPromptBindings::attach_state(bindings, Arc::clone(&chord_state));

        let scroll = Pane::new()
            .min_height(visible_rows(""))
            .max_height(PROMPT_MAX_ROWS)
            .children([input as Box<dyn Widget>])
            .y_scroll(Scrollbar::AutoHide)
            .x_scroll(Scrollbar::AutoHide)
            .id(&mut scroll_id);

        let root = FocusPane::new(theme)
            .padding(Spacing::balanced(1))
            .child(scroll as Box<dyn Widget>);

        Box::new(Self {
            root,
            scroll_id,
            input_id,
            chord_state,
            pending_action: None,
            focused_once: false,
        })
    }

    pub fn set_running(&mut self, running: bool) {
        self.chord_state.lock().expect("prompt chord lock").running = running;
    }

    pub fn set_content(&mut self, text: &str) {
        let handles = self.handles();
        if let Some(input) = self.root.get_widget_mut(self.input_id) {
            input.set_content(text.to_string());
        }
        Self::sync_height(&mut *self.root, &handles, text);
    }

    pub fn content(&self) -> String {
        self.root
            .get_widget(self.input_id)
            .map(Input::get_string)
            .unwrap_or_default()
    }

    pub fn take_action(&mut self) -> Option<PromptAction> {
        self.drain_chord_actions();
        self.pending_action.take()
    }

    pub(crate) fn handles(&self) -> PromptHandles {
        PromptHandles {
            input_id: self.input_id,
            scroll_id: self.scroll_id,
            chord: Arc::clone(&self.chord_state),
        }
    }

    pub(crate) fn take_pending(handles: &PromptHandles) -> Option<PromptAction> {
        handles.chord.lock().expect("prompt chord lock").pending.take()
    }

    #[cfg(not(test))]
    pub(crate) fn sync_from_host(root: &mut dyn Widget, handles: &PromptHandles, host_text: &str, running: bool) {
        handles.chord.lock().expect("prompt chord lock").running = running;
        let widget_text = root
            .get_widget(handles.input_id)
            .map(Input::get_string)
            .unwrap_or_default();
        if Self::should_sync_prompt_from_host(&widget_text, host_text) && widget_text != host_text {
            if let Some(input) = root.get_widget_mut(handles.input_id) {
                input.set_content(host_text.to_string());
            }
            Self::sync_height(root, handles, host_text);
        }
    }

    pub(crate) fn content_in(root: &dyn Widget, handles: &PromptHandles) -> String {
        root.get_widget(handles.input_id)
            .map(Input::get_string)
            .unwrap_or_default()
    }

    pub(crate) fn set_content_in(root: &mut dyn Widget, handles: &PromptHandles, text: &str) {
        if let Some(input) = root.get_widget_mut(handles.input_id) {
            input.set_content(text.to_string());
        }
        Self::sync_height(root, handles, text);
    }

    #[cfg(not(test))]
    fn should_sync_prompt_from_host(widget_text: &str, host_text: &str) -> bool {
        widget_text.is_empty() || widget_text == host_text
    }

    fn sync_height(root: &mut dyn Widget, handles: &PromptHandles, text: &str) {
        let rows = visible_rows(text);
        if let Some(scroll) = root.get_widget_mut(handles.scroll_id) {
            scroll.set_min_height(Some(rows));
            scroll.set_max_height(Some(PROMPT_MAX_ROWS));
        }
        if let Some(input) = root.get_widget_mut(handles.input_id) {
            input.set_min_height(Some(rows));
        }
    }

    fn drain_chord_actions(&mut self) {
        if let Some(action) = self.chord_state.lock().expect("prompt chord lock").pending.take() {
            self.pending_action = Some(action);
        }
    }
}

impl DelegateWidget for PromptPane {
    tuie::delegate_widget!(root);

    fn after_before_layout(&mut self) {
        if !self.focused_once {
            tuie::focus_widget(self.input_id);
            self.focused_once = true;
        }
    }

    fn after_on_input(&mut self, _result: InputResult) {
        self.drain_chord_actions();
        let handles = self.handles();
        let text = self.content();
        Self::sync_height(&mut *self.root, &handles, &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuie::emulator::Emulator;

    fn init_pane() -> (Box<PromptPane>, Emulator) {
        let _ = crate::theme::apply_tuie_theme(Theme::dark());
        let mut pane = PromptPane::new(Theme::dark());
        let mut term = Emulator::new(pane.as_mut(), Vec2::new(40, 8));
        tuie::ensure_focused();
        term.update(pane.as_mut(), &[]);
        (pane, term)
    }

    fn send_enter(pane: &mut Box<PromptPane>, term: &mut Emulator, steer: bool) {
        let chord = if steer { chord!(Ctrl + Enter) } else { chord!(Enter) };
        term.update(pane.as_mut(), &[RuntimeEvent::from(chord)]);
        pane.drain_chord_actions();
    }

    fn type_chars(pane: &mut Box<PromptPane>, term: &mut Emulator, text: &str) {
        for ch in text.chars() {
            term.update(pane.as_mut(), &[RuntimeEvent::from(chord!(Char(ch)))]);
        }
        pane.drain_chord_actions();
    }

    #[test]
    fn submit_when_idle() {
        let (mut pane, mut term) = init_pane();
        type_chars(&mut pane, &mut term, "hello");
        send_enter(&mut pane, &mut term, false);
        assert!(matches!(
            pane.take_action(),
            Some(PromptAction::Submit(s)) if s == "hello"
        ));
        assert!(pane.content().is_empty());
    }

    #[test]
    fn queue_when_running() {
        let (mut pane, mut term) = init_pane();
        pane.set_running(true);
        type_chars(&mut pane, &mut term, "follow up");
        send_enter(&mut pane, &mut term, false);
        assert!(matches!(
            pane.take_action(),
            Some(PromptAction::Queue(s)) if s == "follow up"
        ));
    }

    #[test]
    fn steer_when_running() {
        let (mut pane, mut term) = init_pane();
        pane.set_running(true);
        type_chars(&mut pane, &mut term, "interrupt");
        send_enter(&mut pane, &mut term, true);
        assert!(matches!(
            pane.take_action(),
            Some(PromptAction::Steer(s)) if s == "interrupt"
        ));
    }

    #[test]
    fn esc_clears_only_when_idle() {
        let (mut pane, mut term) = init_pane();
        type_chars(&mut pane, &mut term, "draft");
        term.update(pane.as_mut(), &[RuntimeEvent::from(chord!(Esc))]);
        pane.drain_chord_actions();
        assert!(matches!(pane.take_action(), Some(PromptAction::Clear)));
        assert!(pane.content().is_empty());

        pane.set_running(true);
        type_chars(&mut pane, &mut term, "busy");
        term.update(pane.as_mut(), &[RuntimeEvent::from(chord!(Esc))]);
        pane.drain_chord_actions();
        assert!(pane.take_action().is_none());
        assert_eq!(pane.content(), "busy");
    }

    #[test]
    fn uses_rounded_border_corners() {
        let (mut pane, _term) = init_pane();
        let term = Emulator::new(pane.as_mut(), Vec2::new(30, 8));
        let snap = term.get_snapshot_text();
        assert!(
            snap.contains('╭') || snap.contains('╮') || snap.contains('╰') || snap.contains('╯'),
            "expected rounded border glyphs in:\n{snap}"
        );
    }

    #[test]
    fn strips_slash_trigger_on_submit() {
        let (mut pane, mut term) = init_pane();
        type_chars(&mut pane, &mut term, "/help");
        send_enter(&mut pane, &mut term, false);
        assert!(matches!(
            pane.take_action(),
            Some(PromptAction::Submit(s)) if s == "help"
        ));
    }

    #[test]
    fn tab_cycles_mode_on_empty_prompt() {
        let (mut pane, mut term) = init_pane();
        term.update(pane.as_mut(), &[RuntimeEvent::from(chord!(Tab))]);
        pane.drain_chord_actions();
        assert!(matches!(pane.take_action(), Some(PromptAction::CycleMode)));
    }
}
