use elph_tui::{AgentMode, PromptInput, sigint_channel};
use futures::{Stream, StreamExt, stream};
use iocraft::prelude::*;
use std::time::Duration;

fn shift_enter(kind: KeyEventKind) -> KeyEvent {
    let mut event = KeyEvent::new(kind, KeyCode::Enter);
    event.modifiers = KeyModifiers::SHIFT;
    event
}

fn ctrl_j(kind: KeyEventKind) -> KeyEvent {
    let mut event = KeyEvent::new(kind, KeyCode::Char('j'));
    event.modifiers = KeyModifiers::CONTROL;
    event
}

#[component]
fn Harness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state(|| false);

    if prompt.read().ends_with('!') {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: |_| {},
                on_mode_change: |_| {},
            )
        }
    }
}

#[component]
fn ValueHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state(|| false);

    if prompt.read().ends_with('!') {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: |_| {},
                on_mode_change: |_| {},
            )
        }
    }
}

#[component]
fn EnterHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(String::new);
    let mut submit_count = hooks.use_state(|| 0u32);
    let mut should_exit = hooks.use_state(|| false);

    if submit_count.get() > 0 || prompt.read().ends_with('!') {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: move |_| submit_count.set(submit_count.get() + 1),
                on_mode_change: |_| {},
            )
        }
    }
}

#[tokio::test]
async fn typing_updates_prompt() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('h'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('h'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('i'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('i'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(Harness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.iter().any(|frame| frame.contains("hi")),
        "expected typed text in a frame, got: {output:?}"
    );
}

#[component]
fn PrefilledEnterHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(|| "hi".to_string());
    let mut submit_count = hooks.use_state(|| 0u32);
    let mut should_exit = hooks.use_state(|| false);

    if submit_count.get() > 0 {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: move |_| submit_count.set(submit_count.get() + 1),
                on_mode_change: |_| {},
            )
        }
    }
}

fn paste_key_events(ch: char) -> [TerminalEvent; 2] {
    let press = match ch {
        '\n' | '\r' => TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('\n'))),
        '\t' => TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Tab)),
        ch => TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char(ch))),
    };
    let release = match ch {
        '\n' | '\r' => TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('\n'))),
        '\t' => TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Tab)),
        ch => TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char(ch))),
    };
    [press, release]
}

fn paste_chars(text: &str) -> Vec<TerminalEvent> {
    text.chars().flat_map(paste_key_events).collect()
}

#[component]
fn PasteHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state(|| false);

    if prompt.read().ends_with('!') {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: |_| panic!("paste should not submit the prompt"),
                on_mode_change: |_| {},
            )
        }
    }
}

#[component]
fn CollapseHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state(|| false);

    if prompt.read().ends_with('!') {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: |_| {},
                on_mode_change: |_| {},
            )
        }
    }
}

#[component]
fn PasteSubmitHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(String::new);
    let mut submitted = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state(|| false);

    if !submitted.read().is_empty() {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    let submitted_preview = submitted.read().replace('\n', "\\n");

    element! {
        View(width: 40, height: 8, padding: 1, flex_direction: FlexDirection::Column) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: move |text| submitted.set(text),
                on_mode_change: |_| {},
            )
            #(if submitted_preview.is_empty() {
                None
            } else {
                Some(element! {
                    Text(content: submitted_preview)
                })
            })
        }
    }
}

fn finalize_paste_event() -> TerminalEvent {
    TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::F(1)))
}

fn collapse_then_exit_events(pasted: &str) -> impl Stream<Item = TerminalEvent> + Send + 'static {
    let mut events = paste_chars(pasted);
    events.extend([
        finalize_paste_event(),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::F(1))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);
    stream::iter(events)
}

fn submit_after_collapsed_paste_events(pasted: &str) -> impl Stream<Item = TerminalEvent> + Send + 'static {
    let mut events = paste_chars(pasted);
    events.extend([
        finalize_paste_event(),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::F(1))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Enter)),
    ]);
    stream::iter(events)
}

#[tokio::test]
async fn multiline_paste_collapses_to_summary_with_preview() {
    let pasted = "fn main() {\n    println!(\"hi\");\n}";
    let events = collapse_then_exit_events(pasted);

    let output = element!(CollapseHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.iter().any(|frame| frame.contains("[Pasted: 03 lines]")),
        "multiline paste should collapse to a summary chip, got: {output:?}"
    );
    assert!(
        !output.iter().any(|frame| frame.contains("println")),
        "collapsed paste should not render the full pasted body, got: {output:?}"
    );
}

#[tokio::test]
async fn collapsed_paste_submits_full_text() {
    let pasted = "line one\nline two";
    let events = submit_after_collapsed_paste_events(pasted);

    let output = element!(PasteSubmitHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.iter().any(|frame| frame.contains("line one\\nline two")),
        "submit should expand collapsed paste to full text, got: {output:?}"
    );
}

#[component]
fn ModeCycleHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(String::new);
    let mut mode = hooks.use_state(|| AgentMode::Build);
    let mut should_exit = hooks.use_state(|| false);

    if prompt.read().ends_with('!') {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: mode.get(),
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: |_| {},
                on_mode_change: move |next| mode.set(next),
            )
        }
    }
}

#[tokio::test]
async fn tab_as_second_pasted_char_does_not_cycle_agent_mode() {
    let pasted = "{\t";
    let mut events = paste_chars(pasted);
    events.extend([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(ModeCycleHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(events)))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.iter().any(|frame| frame.contains("Build")),
        "tab after one pasted char should not cycle mode, got: {output:?}"
    );
    assert!(
        !output.iter().any(|frame| frame.contains("Plan")),
        "early tab in paste burst must not switch to Plan, got: {output:?}"
    );
}

#[tokio::test]
async fn json_paste_with_tabs_does_not_cycle_agent_mode() {
    let pasted = "{\n\t\"name\": \"elph\"\n}";
    let mut events = paste_chars(pasted);
    events.extend([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(ModeCycleHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(events)))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.iter().any(|frame| frame.contains("Build")),
        "json paste should not cycle agent mode away from Build, got: {output:?}"
    );
    assert!(
        !output.iter().any(|frame| frame.contains("Plan")),
        "tab characters in pasted json must not switch to Plan, got: {output:?}"
    );
}

#[tokio::test]
async fn long_single_line_paste_is_retained() {
    let pasted = "x".repeat(120);
    let mut events = paste_chars(&pasted);
    events.extend([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(Harness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(events)))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.iter().any(|frame| frame.contains(&"x".repeat(20))),
        "long pasted text should remain in the prompt, got: {output:?}"
    );
}

#[tokio::test]
async fn paste_with_trailing_enter_does_not_submit() {
    let mut events = paste_chars("pasted text");
    events.extend([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Enter)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(PasteHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(events)))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.iter().any(|frame| frame.contains("pasted text")),
        "pasted text should remain in the prompt, got: {output:?}"
    );
}

#[component]
fn SubmitThenTypeHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(|| "hi".to_string());
    let mut should_exit = hooks.use_state(|| false);

    if prompt.read().ends_with('!') {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: |_| {},
                on_mode_change: |_| {},
            )
        }
    }
}

#[tokio::test]
async fn first_keystroke_after_submit_registers_immediately() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Enter)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('n'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('n'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(SubmitThenTypeHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let shows_post_submit_text = output.iter().any(|frame| {
        let prompt_line = frame.lines().find(|line| line.contains('>')).unwrap_or("");
        prompt_line.contains("> n") && !prompt_line.contains('h')
    });
    assert!(
        shows_post_submit_text,
        "first character after submit should appear after one press, got: {output:?}"
    );
}

#[tokio::test]
async fn plain_enter_submits_without_newline() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Enter)),
    ]);

    let output = element!(PrefilledEnterHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.iter().any(|frame| frame.contains("hi")),
        "prompt text should remain on one line before submit, got: {output:?}"
    );
    assert!(output.len() > 1, "plain Enter should submit and exit, got: {output:?}");
}

#[tokio::test]
async fn first_keystroke_registers_immediately() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('x'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('x'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(Harness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let first_char_frame = output.iter().find(|frame| frame.contains('x'));
    assert!(
        first_char_frame.is_some(),
        "first keystroke should appear after a single press, got: {output:?}"
    );
}

#[tokio::test]
async fn ctrl_j_inserts_single_newline_without_blank_line() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('a'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('a'))),
        TerminalEvent::Key(ctrl_j(KeyEventKind::Press)),
        TerminalEvent::Key(ctrl_j(KeyEventKind::Release)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(EnterHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let has_double_newline = output.iter().any(|frame| frame.contains("a\n\nb"));
    assert!(
        !has_double_newline,
        "ctrl+j should insert one newline, not a blank line, got: {output:?}"
    );
}

#[tokio::test]
async fn consecutive_newlines_do_not_stack_blank_lines() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('a'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('a'))),
        TerminalEvent::Key(shift_enter(KeyEventKind::Press)),
        TerminalEvent::Key(shift_enter(KeyEventKind::Release)),
        TerminalEvent::Key(shift_enter(KeyEventKind::Press)),
        TerminalEvent::Key(shift_enter(KeyEventKind::Release)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(ValueHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let has_double_newline = output.iter().any(|frame| frame.contains("a\n\n"));
    assert!(
        !has_double_newline,
        "repeated newline shortcuts should not stack blank lines, got: {output:?}"
    );
}

#[tokio::test]
async fn shift_enter_inserts_single_newline_without_blank_line() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('a'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('a'))),
        TerminalEvent::Key(shift_enter(KeyEventKind::Press)),
        TerminalEvent::Key(shift_enter(KeyEventKind::Release)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(EnterHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let has_double_newline = output.iter().any(|frame| frame.contains("a\n\nb"));
    assert!(
        !has_double_newline,
        "shift+enter should insert one newline, not a blank line, got: {output:?}"
    );
}

#[tokio::test]
async fn shift_enter_produces_expected_multiline_value() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('a'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('a'))),
        TerminalEvent::Key(shift_enter(KeyEventKind::Press)),
        TerminalEvent::Key(shift_enter(KeyEventKind::Release)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(ValueHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let multiline = output.iter().any(|frame| {
        let a_line = frame.lines().any(|line| line.contains('a') && !line.contains('b'));
        let b_line = frame.lines().any(|line| line.contains('b') && !line.contains('a'));
        a_line && b_line
    });
    assert!(
        multiline,
        "expected 'a' on the first line and 'b' on the next, got: {output:?}"
    );
    let reversed = output.iter().any(|frame| {
        frame.lines().any(|line| line.contains('b') && !line.contains('a'))
            && !frame.lines().any(|line| line.contains('a') && !line.contains('b'))
    });
    assert!(
        !reversed,
        "newline should not place the next character before typed text, got: {output:?}"
    );
}

#[tokio::test]
async fn shift_enter_places_cursor_on_new_line_for_next_character() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('a'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('a'))),
        TerminalEvent::Key(shift_enter(KeyEventKind::Press)),
        TerminalEvent::Key(shift_enter(KeyEventKind::Release)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(EnterHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let preserves_first_line = output
        .iter()
        .any(|frame| frame.lines().any(|line| line.contains('a') && !line.contains('b')));
    assert!(
        preserves_first_line,
        "first line should keep 'a' after newline, got: {output:?}"
    );
}

#[tokio::test]
async fn shift_enter_inserts_newline_without_submit() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('a'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('a'))),
        TerminalEvent::Key(shift_enter(KeyEventKind::Press)),
        TerminalEvent::Key(shift_enter(KeyEventKind::Release)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('b'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('!'))),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Char('!'))),
    ]);

    let output = element!(EnterHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    let multiline = output.iter().any(|frame| {
        let a_line = frame.lines().any(|line| line.contains("a") && !line.contains('b'));
        let b_line = frame.lines().any(|line| line.contains('b') && !line.contains('a'));
        a_line && b_line
    });
    assert!(multiline, "expected multiline prompt text, got: {output:?}");
}

#[cfg(unix)]
#[component]
fn SigintPromptHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(String::new);
    let mut interrupted = hooks.use_state(|| false);
    let mut should_exit = hooks.use_state(|| false);

    hooks.use_future(async move {
        let mut sigint = sigint_channel();
        if sigint.recv().await {
            interrupted.set(true);
        }
    });

    if interrupted.get() || prompt.read().ends_with('!') {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: |_| {},
                on_mode_change: |_| {},
            )
        }
    }
}

#[component]
fn EscapeClearHarness(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let prompt = hooks.use_state(|| "draft".to_string());
    let mut should_exit = hooks.use_state(|| false);

    if prompt.read().is_empty() {
        should_exit.set(true);
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        View(width: 40, height: 8, padding: 1) {
            PromptInput(
                value: Some(prompt),
                model_name: "test-model".to_string(),
                mode: AgentMode::Build,
                theme: elph_tui::Theme::dark(),
                has_focus: true,
                on_submit: |_| panic!("escape should clear, not submit"),
                on_mode_change: |_| {},
            )
        }
    }
}

#[tokio::test]
async fn escape_clears_non_empty_prompt() {
    let events = stream::iter([
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Esc)),
        TerminalEvent::Key(KeyEvent::new(KeyEventKind::Release, KeyCode::Esc)),
    ]);

    let output = element!(EscapeClearHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        output.len() > 1,
        "escape should clear the prompt and exit harness, got: {output:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn sigint_channel_interrupts_prompt_input_loop() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(100));
        unsafe {
            libc::raise(libc::SIGINT);
        }
    });

    let events = stream::iter([] as [TerminalEvent; 0]);

    let output = element!(SigintPromptHarness)
        .mock_terminal_render_loop(MockTerminalConfig::with_events(events))
        .map(|frame| frame.to_string())
        .collect::<Vec<_>>()
        .await;

    assert!(
        !output.is_empty(),
        "sigint_channel should drive PromptInput harness to render and exit, got: {output:?}"
    );
}
