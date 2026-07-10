use elph_tui::{PromptState, Theme, consume_ctrl_char, handle_prompt_input};
use slt::{Event, KeyCode, KeyModifiers, TestBackend};

#[test]
fn consume_legacy_ctrl_q() {
    let mut backend = TestBackend::new(80, 10);
    let mut state = PromptState::new("model");
    let theme = Theme::dark();

    backend.run_with_events(vec![Event::key(KeyCode::Char('\x11'))], |ui| {
        assert!(consume_ctrl_char(ui, 'q'));
        let action = handle_prompt_input(ui, &mut state, false);
        assert_eq!(action, elph_tui::PromptAction::None);
        render_prompt_smoke(ui, &mut state, theme);
    });
}

#[test]
fn consume_modifier_ctrl_t() {
    let mut backend = TestBackend::new(80, 10);
    backend.run_with_events(vec![Event::key_mod(KeyCode::Char('t'), KeyModifiers::CONTROL)], |ui| {
        assert!(consume_ctrl_char(ui, 't'));
    });
}

fn render_prompt_smoke(ui: &mut slt::Context, state: &mut PromptState, theme: Theme) {
    elph_tui::render_prompt(ui, state, theme, elph_tui::PromptOpts::default());
}
