use std::cell::RefCell;
use std::rc::Rc;

use elph_tui::{
    CURSOR_MARKER, DiffTui, Editor, EditorTheme, LineComponent, RecordingTerminal, Text, extract_and_strip_cursor,
};

#[test]
fn editor_collapses_and_expands_multiline_paste() {
    let mut editor = Editor::new(EditorTheme::dark());
    editor.set_focused(true);
    let pasted = "line one\nline two\nline three\n";
    editor.handle_input(pasted);
    assert!(editor.get_text().contains("Pasted"));
    let expanded = editor.get_expanded_text();
    assert!(expanded.contains("line one"));
    assert!(expanded.contains("line three"));
}

#[test]
fn editor_undo_restores_previous_text() {
    let mut editor = Editor::new(EditorTheme::dark());
    editor.set_focused(true);
    editor.handle_input("hello");
    editor.handle_input("\x1f");
    assert_eq!(editor.get_text(), "");
}

#[test]
fn extract_cursor_strips_marker_from_lines() {
    let mut lines = vec![format!("abc{CURSOR_MARKER}def")];
    let pos = extract_and_strip_cursor(&mut lines).unwrap();
    assert_eq!(pos.col, 3);
    assert!(!lines[0].contains(CURSOR_MARKER));
}

#[test]
fn diff_tui_renders_focused_editor_with_cursor() {
    let mut tui = DiffTui::new(Box::new(RecordingTerminal::new(60, 12)));
    tui.add_child(Box::new(Text::new("transcript")));
    let mut editor = Editor::new(EditorTheme::dark());
    editor.set_focused(true);
    editor.set_text("type here");
    let _handle = tui.show_overlay(Box::new(editor), Default::default());
    tui.request_render(true);
    tui.pump_render().unwrap();
    assert!(tui.has_overlay());
}
