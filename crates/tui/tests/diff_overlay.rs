use elph_tui::{
    DiffTui, OverlayAnchor, OverlayOptions, RecordingTerminal, SelectItem, SelectList, SelectListTheme, SizeValue,
    Text, composite_line_at, resolve_layout,
};

#[test]
fn resolve_layout_centers_narrow_overlay() {
    let options = OverlayOptions {
        width: Some(SizeValue::Absolute(30)),
        anchor: OverlayAnchor::Center,
        ..Default::default()
    };
    let layout = resolve_layout(&options, 4, 60, 12).unwrap();
    assert_eq!(layout.col, 15);
}

#[test]
fn composite_line_at_splices_overlay_text() {
    let out = composite_line_at("hello world!", "PICK", 6, 4, 12);
    assert!(out.contains("PICK"));
}

#[test]
fn diff_tui_shows_overlay_and_handles_navigation() {
    let mut tui = DiffTui::new(Box::new(RecordingTerminal::new(60, 12)));
    tui.add_child(Box::new(Text::new("Background")));
    let list = SelectList::new(
        vec![SelectItem::new("a", "Alpha"), SelectItem::new("b", "Beta")],
        4,
        SelectListTheme::dark(),
    );
    let handle = tui.show_overlay(
        Box::new(list),
        OverlayOptions {
            width: Some(SizeValue::Absolute(30)),
            anchor: OverlayAnchor::Center,
            ..Default::default()
        },
    );
    tui.request_render(true);
    tui.pump_render().unwrap();
    assert!(tui.has_overlay());

    assert!(tui.handle_input("\x1b[B"));
    tui.focus_overlay(handle);
    tui.request_render(true);
    tui.pump_render().unwrap();
}
