use elph_tui::diff::LineComponent;
use elph_tui::{
    CancellableLoader, MarkdownTheme, SelectItem, SettingItem, SettingsList, SettingsListTheme, StreamingBuffer,
    ToolExecutionState, ToolExecutionStatus, TranscriptEntry, render_markdown_lines,
};

#[test]
fn streaming_buffer_throttles_and_accumulates() {
    let mut buf = StreamingBuffer::new();
    assert!(buf.push("Hello"));
    buf.push(" world");
    assert_eq!(buf.content(), "Hello world");
}

#[test]
fn transcript_entries_cover_elph_roles() {
    let entries = [
        TranscriptEntry::user("fix the bug"),
        TranscriptEntry::assistant_streaming("Looking at **main.rs**..."),
        TranscriptEntry::tool(
            ToolExecutionState::new("bash", "bash")
                .with_args("npm test")
                .with_status(ToolExecutionStatus::Running),
        ),
        TranscriptEntry::thinking("planning", false),
    ];
    assert_eq!(entries.len(), 4);
}

#[test]
fn settings_list_and_cancellable_loader_work_together() {
    let mut settings = SettingsList::new(
        vec![SettingItem::new("model", "Model", "gpt-4")],
        3,
        SettingsListTheme::dark(),
    );
    settings.set_focused(true);
    let lines = settings.render(40);
    assert!(!lines.is_empty());

    let mut loader = CancellableLoader::new("Authenticating");
    loader.start();
    loader.handle_input("\x1b");
    assert!(loader.aborted());
}

#[test]
fn markdown_renders_assistant_style_output() {
    let lines = render_markdown_lines("# Title\n\n`code`", 60, MarkdownTheme::dark());
    assert!(!lines.is_empty());
}

#[test]
fn markdown_renders_gfm_table_in_assistant_output() {
    let md = "| Tool | Status |\n|------|--------|\n| cargo test | ok |";
    let lines = render_markdown_lines(md, 60, MarkdownTheme::dark());
    let joined = lines.join("\n");
    assert!(joined.contains("Tool"));
    assert!(joined.contains("cargo test"));
    assert!(joined.contains('┌'));
}

#[test]
fn mock_selectors_use_select_items() {
    let sessions = [SelectItem::new("s1", "Session 1"), SelectItem::new("s2", "Session 2")];
    assert_eq!(sessions.len(), 2);
}
