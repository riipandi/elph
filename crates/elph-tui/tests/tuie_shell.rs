//! Integration tests for the tuie agent shell widgets.

use elph_tui::{
    AgentMode, CommandPaletteState, GlobalChordHandler, PromptPane, ShellAction, ShellActionSink, ShellChromeData,
    SidebarPlaceholder, Theme, TranscriptPane, apply_tuie_theme, build_activity_widget, build_footer_widget,
    owly_builtin_commands, palette_visible,
};
use tuie::emulator::Emulator;
use tuie::prelude::*;

fn sample_chrome() -> ShellChromeData {
    ShellChromeData {
        running: true,
        sidebar_open: false,
        palette_open: false,
        activity_visible: true,
        activity_label: "Working".into(),
        activity_cancel_requested: false,
        model_name: "claude-sonnet".into(),
        provider: "anthropic".into(),
        thinking_level: "high".into(),
        supports_images: false,
        cost_usd: 0.0,
        tokens_used: 12_000,
        context_pct: 4.5,
        context_limit: 200_000,
        project_dir: "~/elph".into(),
        session_id: "sess-1".into(),
        mode: AgentMode::Ask,
        turn: 3,
        branch: "main".into(),
        git_additions: 2,
        git_deletions: 1,
    }
}

#[test]
fn footer_renders_model_and_session() {
    let theme = Theme::dark();
    let chrome = sample_chrome();
    let mut footer = build_footer_widget(&chrome, theme);
    let term = Emulator::new(&mut *footer, Vec2::new(60, 4));
    let snap = term.get_snapshot_text();
    assert!(snap.contains("claude-sonnet"));
    assert!(snap.contains("sess-1"));
    assert!(snap.contains("turn: 3"));
}

#[test]
fn activity_hidden_when_idle() {
    let theme = Theme::dark();
    let mut chrome = sample_chrome();
    chrome.activity_visible = false;
    let mut activity = build_activity_widget(&chrome, theme);
    let term = Emulator::new(&mut *activity, Vec2::new(40, 2));
    assert!(term.get_snapshot_text().trim().is_empty());
}

#[test]
fn activity_shows_label_when_busy() {
    let theme = Theme::dark();
    let chrome = sample_chrome();
    let mut activity = build_activity_widget(&chrome, theme);
    let term = Emulator::new(&mut *activity, Vec2::new(40, 2));
    assert!(term.get_snapshot_text().contains("Working"));
}

#[test]
fn sidebar_uses_rounded_border() {
    let theme = Theme::dark();
    let _ = apply_tuie_theme(theme);
    let mut sidebar = SidebarPlaceholder::new(theme);
    let term = Emulator::new(&mut *sidebar, Vec2::new(28, 8));
    let snap = term.get_snapshot_text();
    assert!(
        snap.contains('╭') || snap.contains('╮') || snap.contains('╰') || snap.contains('╯'),
        "expected rounded border glyphs in:\n{snap}"
    );
}

#[test]
fn sidebar_placeholder_renders_title() {
    let theme = Theme::dark();
    let mut sidebar = SidebarPlaceholder::new(theme);
    let term = Emulator::new(&mut *sidebar, Vec2::new(28, 8));
    let snap = term.get_snapshot_text();
    assert!(snap.contains("Side panel"));
}

#[test]
fn transcript_and_prompt_compose_minimal_shell() {
    let theme = Theme::dark();
    let mut transcript = TranscriptPane::new(theme);
    transcript.set_lines(vec!["❯ hello".into(), "world".into()]);

    let mut prompt = PromptPane::new(theme);
    prompt.set_content("draft");

    let root = Pane::new()
        .vertical()
        .children([transcript.flex(1) as Box<dyn Widget>, prompt as Box<dyn Widget>]);

    let mut root = GlobalChordHandler::new(root, ShellActionSink::default());
    let term = Emulator::new(&mut *root, Vec2::new(60, 12));
    let snap = term.get_snapshot_text();
    assert!(snap.contains("hello"));
    assert!(snap.contains("draft"));
}

#[test]
fn palette_lists_all_commands_in_forced_mode() {
    let commands = owly_builtin_commands();
    let state = CommandPaletteState {
        forced: true,
        ..Default::default()
    };
    assert!(palette_visible("/help"));
    assert!(!palette_visible("help"));
    let first = state.selected_command(&commands, "").unwrap();
    assert_eq!(first.name, "help");
}

#[test]
fn global_chords_enqueue_shell_actions() {
    let sink = ShellActionSink::default();
    let inner = Pane::new().child(Text::new().content("body"));
    let mut handler = GlobalChordHandler::new(inner, sink.clone());
    let mut term = Emulator::new(&mut *handler, Vec2::new(80, 10));

    term.update(&mut *handler, &[RuntimeEvent::from(chord!(Ctrl + s))]);
    assert_eq!(sink.take(), vec![ShellAction::ToggleSidebar]);

    term.update(&mut *handler, &[RuntimeEvent::from(chord!(Ctrl + k))]);
    assert_eq!(sink.take(), vec![ShellAction::OpenPalette]);

    term.update(&mut *handler, &[RuntimeEvent::from(chord!(Shift + Down))]);
    assert_eq!(sink.take(), vec![ShellAction::TranscriptScrollDown]);
}
