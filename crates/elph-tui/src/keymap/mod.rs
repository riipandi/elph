//! Global chord handling and shell layout constants.

mod actions;

pub use actions::{PromptSubmitMode, ShellAction};

use std::cell::RefCell;
use std::rc::Rc;

use chord_macro::chord;
use tuie::prelude::*;

/// Minimum terminal width before the sidebar is shown.
pub const SIDEBAR_MIN_TOTAL_WIDTH: u16 = 100;
/// Fixed sidebar width in cells when visible.
pub const SIDEBAR_WIDTH: u16 = 28;

/// Shared queue populated by [`GlobalChordHandler`].
#[derive(Clone, Default)]
pub struct ShellActionSink(Rc<RefCell<Vec<ShellAction>>>);

impl ShellActionSink {
    /// Drains any pending shell actions.
    pub fn take(&self) -> Vec<ShellAction> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

/// Intercepts global chords before dispatching to the wrapped widget tree.
pub struct GlobalChordHandler {
    inner: Box<dyn Widget>,
    sink: ShellActionSink,
}

impl GlobalChordHandler {
    /// Wraps `inner` and records matched chords in `sink`.
    pub fn new(inner: Box<dyn Widget>, sink: ShellActionSink) -> Box<Self> {
        Box::new(Self { inner, sink })
    }

    /// Returns the shared action sink for draining after input dispatch.
    pub fn sink(&self) -> ShellActionSink {
        self.sink.clone()
    }

    #[cfg(test)]
    pub(crate) fn prime_focus(
        &mut self,
        term: &mut tuie::emulator::Emulator,
        input_id: WidgetId<tuie::prelude::Input>,
    ) {
        crate::shell::tuie_shell::focus_prompt_input(input_id);
        term.update(
            self.inner.as_mut(),
            &[tuie::prelude::RuntimeEvent::Resize(tuie::get_runtime_info().size)],
        );
    }

    #[cfg(test)]
    pub(crate) fn flush_agent_effects(&mut self) {
        if let Some(agent) =
            (self.inner.as_mut() as &mut dyn std::any::Any).downcast_mut::<crate::shell::tuie_shell::AgentShell>()
        {
            agent.flush_input_side_effects();
        }
    }
}

fn chord_action(chord: &Chord) -> Option<ShellAction> {
    let actions = [
        (chord!(Ctrl + s), ShellAction::ToggleSidebar),
        (chord!(Ctrl + k), ShellAction::OpenPalette),
        (chord!(Ctrl + t), ShellAction::ToggleTheme),
        (chord!(Ctrl + q), ShellAction::Quit),
        (chord!(Shift + Up), ShellAction::TranscriptScrollUp),
        (chord!(Shift + Down), ShellAction::TranscriptScrollDown),
        (chord!(Shift + End), ShellAction::TranscriptJumpTail),
    ];
    actions
        .into_iter()
        .find_map(|(pattern, action)| (*chord == pattern).then_some(action))
}

impl DelegateWidget for GlobalChordHandler {
    tuie::delegate_widget!(inner);

    fn override_on_input(&mut self, queue: &mut InputQueue) -> InputResult {
        let Some(event) = queue.peek() else {
            return InputResult::Rejected;
        };

        if let Some(action) = chord_action(&event.chord) {
            self.sink.0.borrow_mut().push(action);
            queue.next();
            if action == ShellAction::Quit {
                tuie::quit(0);
            }
            return InputResult::Handled;
        }

        if matches!(&event.chord, chord!(Ctrl + c)) {
            let result = self.inner.on_input(queue);
            if result == InputResult::Handled {
                return result;
            }
            self.sink.0.borrow_mut().push(ShellAction::Cancel);
            queue.next();
            return InputResult::Handled;
        }

        let result = self.inner.on_input(queue);
        if let Some(agent) =
            (self.inner.as_mut() as &mut dyn std::any::Any).downcast_mut::<crate::shell::tuie_shell::AgentShell>()
        {
            agent.flush_input_side_effects();
        }

        if let Some(event) = queue.peek() {
            match &event.chord {
                chord!(Ctrl + z) => {
                    queue.next();
                    tuie::suspend();
                    return InputResult::Handled;
                }
                chord!(Tab) if queue.is_unhandled() => {
                    queue.next();
                    tuie::focus_next_tab_order(Sign::Positive);
                    return InputResult::Handled;
                }
                chord!(Shift + Tab) if queue.is_unhandled() => {
                    queue.next();
                    tuie::focus_next_tab_order(Sign::Negative);
                    return InputResult::Handled;
                }
                _ => {}
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuie::emulator::Emulator;

    #[test]
    fn layout_constants_match_design() {
        assert_eq!(SIDEBAR_MIN_TOTAL_WIDTH, 100);
        assert_eq!(SIDEBAR_WIDTH, 28);
    }

    #[test]
    fn global_chords_enqueue_actions() {
        let sink = ShellActionSink::default();
        let inner = Pane::new().child(Text::new().content("inner"));
        let mut handler = GlobalChordHandler::new(inner, sink.clone());
        let mut term = Emulator::new(&mut *handler, Vec2::new(80, 10));

        term.update(&mut *handler, &[RuntimeEvent::from(chord!(Ctrl + s))]);
        assert_eq!(sink.take(), vec![ShellAction::ToggleSidebar]);

        term.update(&mut *handler, &[RuntimeEvent::from(chord!(Ctrl + k))]);
        assert_eq!(sink.take(), vec![ShellAction::OpenPalette]);

        term.update(&mut *handler, &[RuntimeEvent::from(chord!(Shift + End))]);
        assert_eq!(sink.take(), vec![ShellAction::TranscriptJumpTail]);
    }

    #[test]
    fn sink_take_drains_pending_actions() {
        let sink = ShellActionSink::default();
        sink.0.borrow_mut().push(ShellAction::Cancel);
        assert_eq!(sink.take(), vec![ShellAction::Cancel]);
        assert!(sink.take().is_empty());
    }
}
