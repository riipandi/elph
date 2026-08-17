//! Dialog shell gallery — cycles presets and header variants.
//!
//! ```sh
//! cargo run -p elph-tui --example demo_dialog_shell
//! ```
//!
//! Keys: `1`–`5` presets · `s` cycle header · `Esc` hub · `q` quit

use anyhow::Result;
use elph_tui::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preset {
    Hub,
    Confirm,
    Question,
    ModeSelect,
    TodoList,
    TodoProgress,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HeaderKind {
    Title,
    Search,
    Tabs,
}

fn sample_todos() -> Vec<DialogTodoItem> {
    vec![
        DialogTodoItem::new("Audit dialog_shell module", DialogTodoStatus::Done)
            .with_detail("Frame, header, overlay, presets"),
        DialogTodoItem::new("Add coding-agent example", DialogTodoStatus::InProgress),
        DialogTodoItem::new("Wire into elph shell", DialogTodoStatus::Pending),
        DialogTodoItem::new("Add interactive demo", DialogTodoStatus::Skipped).with_detail("Deferred to follow-up PR"),
    ]
}

fn sample_progress() -> Vec<DialogTodoProgressItem> {
    vec![
        DialogTodoProgressItem::new("Index codebase", DialogTodoProgress::Done),
        DialogTodoProgressItem::new("Run quality gates", DialogTodoProgress::Running),
        DialogTodoProgressItem::new("Publish docs", DialogTodoProgress::Queued),
    ]
}

fn question_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("Composer", "Fast balanced model"),
        SelectOption::new("Opus", "Deep reasoning"),
        SelectOption::new("Local", "Offline / privacy"),
    ]
}

fn header_for(kind: HeaderKind, preset: Preset, search: State<String>, tabs: State<usize>) -> DialogHeader {
    match kind {
        HeaderKind::Title => DialogHeader::title(match preset {
            Preset::Confirm => "Confirm exit",
            Preset::Question => "Choose model",
            Preset::ModeSelect => "Agent mode",
            Preset::TodoList => "Session goals",
            Preset::TodoProgress => "Goal progress",
            Preset::Hub => "Dialog gallery",
        }),
        HeaderKind::Search => DialogHeader::search("filter…", Some(search), true),
        HeaderKind::Tabs => {
            DialogHeader::tabs(vec!["All".into(), "Commands".into(), "Skills".into()], Some(tabs), true)
        }
    }
}

fn dialog_layout(width: u16, height: u16, preset: Preset) -> (DialogChrome, u16) {
    let theme = UiTheme::default();
    let outer_width = width.clamp(44, 72);
    let chrome = DialogChrome {
        width: outer_width,
        ..DialogChrome::default()
    };
    let body_width = chrome.inner_body_width();
    let max_body = dialog_max_content_height(height, &chrome, 4);
    let todos = sample_todos();
    let (min_h, list_h) = match preset {
        Preset::Confirm => {
            let message = "Quit now? The agent turn will be canceled.";
            (
                dialog_body_min_height(dialog_text_rows(message, body_width).saturating_add(2)),
                DIALOG_SELECT_AUTO_HEIGHT,
            )
        }
        Preset::Question => dialog_select_body_plan(
            &question_options(),
            true,
            body_width,
            theme,
            "Which model should handle the next turn?",
            0,
            Some(max_body),
            false,
        ),
        Preset::ModeSelect => dialog_select_body_plan(
            &dialog_mode_select_options(),
            true,
            body_width,
            theme,
            "Choose how much autonomy the agent has for this session.",
            0,
            Some(max_body),
            false,
        ),
        Preset::TodoList => {
            let natural = dialog_todo_list_content_rows(&todos, body_width, theme, theme.dialog_row_gap());
            (dialog_body_min_height(natural.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT)
        }
        Preset::TodoProgress => (dialog_body_min_height(5.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT),
        Preset::Hub => (dialog_body_min_height(6.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT),
    };
    (
        DialogChrome {
            min_content_height: min_h,
            ..chrome
        },
        list_h,
    )
}

#[component]
fn Gallery(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (screen_width, screen_height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut preset = hooks.use_state(|| Preset::Hub);
    let mut header_kind = hooks.use_state(|| HeaderKind::Title);
    let mut exit = hooks.use_state(|| false);
    let selected = hooks.use_state(|| 0usize);
    let search = hooks.use_state(String::new);
    let tabs = hooks.use_state(|| 0usize);

    hooks.use_terminal_events(move |event| {
        let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
            return;
        };
        if kind == KeyEventKind::Release {
            return;
        }
        match code {
            KeyCode::Char('q') => exit.set(true),
            KeyCode::Esc => preset.set(Preset::Hub),
            KeyCode::Char('1') => preset.set(Preset::Confirm),
            KeyCode::Char('2') => preset.set(Preset::Question),
            KeyCode::Char('3') => preset.set(Preset::ModeSelect),
            KeyCode::Char('4') => preset.set(Preset::TodoList),
            KeyCode::Char('5') => preset.set(Preset::TodoProgress),
            KeyCode::Char('s') => {
                header_kind.set(match header_kind.get() {
                    HeaderKind::Title => HeaderKind::Search,
                    HeaderKind::Search => HeaderKind::Tabs,
                    HeaderKind::Tabs => HeaderKind::Title,
                });
            }
            _ => {}
        }
    });

    if exit.get() {
        system.exit();
    }

    let active = preset.get();
    let (chrome, list_height) = dialog_layout(screen_width, screen_height, active);
    let body_width = chrome.inner_body_width();
    let header = if active == Preset::Hub {
        DialogHeader::title("Dialog gallery")
    } else {
        header_for(header_kind.get(), active, search, tabs)
    };

    let hub = element! {
        View(flex_direction: FlexDirection::Column, gap: 1) {
            Text(content: "Press 1–5 to open a preset dialog body.".to_string(), color: Color::Grey, wrap: TextWrap::Wrap)
            Text(content: "Press s to cycle Title / Search / Tabs headers.".to_string(), color: Color::DarkGrey, wrap: TextWrap::Wrap)
            Text(content: "Esc returns here · q quits.".to_string(), color: Color::DarkGrey, wrap: TextWrap::Wrap)
        }
    };

    let body: AnyElement<'static> = match active {
        Preset::Hub => hub.into(),
        Preset::Confirm => element! {
            DialogConfirmContent(
                width: body_width,
                message: "Quit now? The agent turn will be canceled.".to_string(),
                action_hint: String::new(),
                message_color: Color::Grey,
                hint_color: Color::DarkGrey,
            )
        }
        .into(),
        Preset::Question => element! {
            DialogQuestionContent(
                width: body_width,
                height: list_height,
                question: "Which model should handle the next turn?".to_string(),
                options: question_options(),
                selected_index: selected,
                has_focus: true,
                show_description: true,
                question_color: Color::Grey,
            )
        }
        .into(),
        Preset::ModeSelect => element! {
            DialogModeSelectContent(
                width: body_width,
                height: list_height,
                selected_index: selected,
                has_focus: true,
                intro: String::new(),
            )
        }
        .into(),
        Preset::TodoList => element! {
            DialogTodoListContent(width: body_width, items: sample_todos())
        }
        .into(),
        Preset::TodoProgress => element! {
            DialogTodoProgressContent(width: body_width, items: sample_progress())
        }
        .into(),
    };

    element! {
        View(
            width: screen_width,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: 2,
        ) {
            DialogShell(chrome: chrome, header: header, on_copy: HandlerMut::default()) {
                #(body)
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(Gallery).render_loop().fullscreen().await?;
    Ok(())
}
