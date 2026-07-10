use crate::ui_events::AgentUiEvent;

#[derive(Debug)]
pub(in crate::tui) enum AppMessage {
    UiEvent(AgentUiEvent),
    DispatchDone { lines: Vec<String>, should_exit: bool },
    DispatchError(String),
}
