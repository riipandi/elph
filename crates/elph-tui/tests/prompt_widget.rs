use elph_tui::{AgentMode, PromptState, Theme, handle_prompt_input, render_prompt};
use slt::{Event, KeyCode, KeyModifiers, TestBackend};

#[test]
fn prompt_renders_minimal_prefix() {
    let mut backend = TestBackend::new(80, 10);
    let mut state = PromptState::new("test-model");
    state.mode = AgentMode::Ask;
    let theme = Theme::dark();

    backend.render(|ui| {
        render_prompt(ui, &mut state, theme, elph_tui::PromptOpts::default());
    });

    backend.assert_contains(">");
}

#[test]
fn submit_clears_prompt_value() {
    let mut state = PromptState::new("model");
    state.textarea.set_value("hello");

    let mut backend = TestBackend::new(80, 10);
    backend.run_with_events(vec![Event::key(KeyCode::Enter)], |ui| {
        let action = handle_prompt_input(ui, &mut state, false);
        assert!(matches!(action, elph_tui::PromptAction::Submit(ref s) if s == "hello"));
    });

    assert!(state.value().is_empty());
}

#[test]
fn running_enter_queues_message() {
    let mut state = PromptState::new("model");
    state.textarea.set_value("follow up");

    let mut backend = TestBackend::new(80, 10);
    backend.run_with_events(vec![Event::key(KeyCode::Enter)], |ui| {
        let action = handle_prompt_input(ui, &mut state, true);
        assert!(matches!(action, elph_tui::PromptAction::Queue(ref s) if s == "follow up"));
    });

    assert!(state.value().is_empty());
}

#[test]
fn running_ctrl_enter_steers() {
    let mut state = PromptState::new("model");
    state.textarea.set_value("stop and do this");

    let mut backend = TestBackend::new(80, 10);
    backend.run_with_events(vec![Event::key_mod(KeyCode::Enter, KeyModifiers::CONTROL)], |ui| {
        let action = handle_prompt_input(ui, &mut state, true);
        assert!(matches!(action, elph_tui::PromptAction::Steer(ref s) if s == "stop and do this"));
    });

    assert!(state.value().is_empty());
}
