use elph_tui::{CollapseState, Theme, composer_demo_entries, render_composer_transcript, render_user_card};
use slt::TestBackend;

#[test]
fn user_card_renders_prefix() {
    let mut backend = TestBackend::new(80, 12);
    let theme = Theme::dark();

    backend.render(|ui| {
        render_user_card(ui, "Hello agent", theme);
    });

    backend.assert_contains("›");
    backend.assert_contains("Hello agent");
}

#[test]
fn composer_transcript_renders_blocks() {
    let mut backend = TestBackend::new(90, 40);
    let theme = Theme::dark();
    let entries = composer_demo_entries();
    let collapse = CollapseState::default();

    backend.render(|ui| {
        render_composer_transcript(ui, &entries, true, theme, &collapse, false);
    });

    backend.assert_contains("›");
    backend.assert_contains("Thought");
    backend.assert_contains("Edit");
}
