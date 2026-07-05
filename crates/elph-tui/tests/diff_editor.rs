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
fn diff_tui_routes_input_to_container_child() {
    let captured = Rc::new(RefCell::new(String::new()));
    let captured_cb = captured.clone();
    let mut editor = Editor::new(EditorTheme::dark());
    editor.set_focused(true);
    editor.on_change = Some(Box::new(move |text| {
        *captured_cb.borrow_mut() = text.to_string();
    }));
    let mut tui = DiffTui::new(Box::new(RecordingTerminal::new(60, 12)));
    tui.add_child(Box::new(editor));
    tui.handle_input("typed");
    assert_eq!(captured.borrow().as_str(), "typed");
}

#[test]
fn editor_deletes_collapsed_paste_block() {
    let mut editor = Editor::new(EditorTheme::dark());
    editor.set_focused(true);
    editor.handle_input("line one\nline two\nline three\n");
    assert!(editor.get_text().contains("Pasted"));
    editor.handle_input("\x7f");
    assert!(!editor.get_text().contains("Pasted"));
}

#[test]
fn editor_deletes_second_duplicate_paste_block() {
    let mut editor = Editor::new(EditorTheme::dark());
    editor.set_focused(true);
    let pasted = "line one\nline two\n";
    editor.handle_input(pasted);
    editor.handle_input(" ");
    editor.handle_input(pasted);

    let text_before = editor.get_text();
    assert_eq!(text_before.matches("Pasted").count(), 2);

    editor.set_cursor(text_before.len().saturating_sub(1));
    editor.handle_input("\x7f");

    let text_after = editor.get_text();
    assert_eq!(text_after.matches("Pasted").count(), 1);
    assert!(editor.get_expanded_text().contains("line one"));
}

#[test]
fn editor_deletes_first_duplicate_paste_then_expands_remaining() {
    let mut editor = Editor::new(EditorTheme::dark());
    editor.set_focused(true);
    let pasted = "line one\nline two\n";
    editor.handle_input(pasted);
    editor.handle_input(" ");
    editor.handle_input(pasted);

    editor.set_cursor(1);
    editor.handle_input("\x7f");

    let text_after = editor.get_text();
    assert_eq!(text_after.matches("Pasted").count(), 1);
    let expanded = editor.get_expanded_text();
    assert!(expanded.contains("line one"));
    assert!(expanded.contains("line two"));
}

#[test]
fn editor_expands_after_pre_marker_edit() {
    let mut editor = Editor::new(EditorTheme::dark());
    editor.set_focused(true);
    editor.handle_input("line one\nline two\n");
    assert!(editor.get_text().contains("Pasted"));
    editor.set_cursor(0);
    editor.handle_input("pre ");
    let expanded = editor.get_expanded_text();
    assert!(expanded.starts_with("pre "));
    assert!(expanded.contains("line one"));
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
