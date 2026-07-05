#[test]
fn frame_renders() {
    let _ = elph_tui::frame(elph_tui::Theme::dark(), vec![]);
}

#[test]
fn label_props_constructible() {
    use elph_tui::LabelProps;
    let _props = LabelProps {
        content: "hello".into(),
        color: None,
    };
}
