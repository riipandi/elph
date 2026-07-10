use elph_tui::{Theme, render_label};
use slt::TestBackend;

#[test]
fn frame_and_label_render() {
    let mut backend = TestBackend::new(40, 10);
    let theme = Theme::dark();

    backend.render(|ui| {
        elph_tui::frame(ui, theme, |ui| {
            render_label(ui, "hello", None);
        });
    });

    backend.assert_contains("hello");
}
