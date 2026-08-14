//! Dialog overlay kinds and chrome tokens.

use elph_tui::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    Mode,
    Question,
    Confirm,
    ConfirmButtons,
    MultiChoice,
    UserInput,
    TodoList,
    TodoProgress,
}

pub const DEMO_MULTI_OPTION_COUNT: usize = 4;

pub fn sample_todos() -> Vec<DialogTodoItem> {
    vec![
        DialogTodoItem::new("Audit dialog_shell spacing", DialogTodoStatus::Done),
        DialogTodoItem::new("Wire slash demo commands", DialogTodoStatus::InProgress)
            .with_detail("Palette entries for each TUI preset"),
        DialogTodoItem::new("Integrate with elph shell", DialogTodoStatus::Pending),
        DialogTodoItem::new("Add interactive demo", DialogTodoStatus::Skipped),
    ]
}

pub fn sample_progress() -> Vec<DialogTodoProgressItem> {
    vec![
        DialogTodoProgressItem::new("load_context", DialogTodoProgress::Done),
        DialogTodoProgressItem::new("run_tools", DialogTodoProgress::Running),
        DialogTodoProgressItem::new("summarize", DialogTodoProgress::Queued),
    ]
}

pub fn overlay_chrome(screen_width: u16, screen_height: u16, kind: OverlayKind) -> (DialogChrome, u16) {
    let theme = UiTheme::default();
    let outer = screen_width.clamp(48, 72);
    let chrome = DialogChrome {
        width: outer,
        ..DialogChrome::default()
    };
    let body_width = chrome.inner_body_width();
    let max_body = dialog_max_content_height(screen_height, &chrome, 4);
    let todos = sample_todos();
    let options = crate::common::lipsum_mock::mock_select_options(DEMO_MULTI_OPTION_COUNT);
    let question = crate::common::lipsum_mock::mock_sentence();
    let (min_h, list_h) = match kind {
        OverlayKind::Confirm => (dialog_body_min_height(5.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT),
        OverlayKind::ConfirmButtons => (dialog_body_min_height(10.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT),
        OverlayKind::UserInput => (dialog_body_min_height(7.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT),
        OverlayKind::Mode => dialog_select_body_plan(
            &dialog_mode_select_options(),
            true,
            body_width,
            theme,
            "Choose how much autonomy the agent has for this session.",
            0,
            Some(max_body),
            false,
        ),
        OverlayKind::Question => {
            dialog_select_body_plan(&options, true, body_width, theme, &question, 0, Some(max_body), false)
        }
        OverlayKind::MultiChoice => {
            dialog_select_body_plan(&options, true, body_width, theme, &question, 1, Some(max_body), false)
        }
        OverlayKind::TodoList => {
            let natural = dialog_todo_list_content_rows(&todos, body_width, theme, theme.dialog_row_gap());
            (dialog_body_min_height(natural.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT)
        }
        OverlayKind::TodoProgress => (dialog_body_min_height(5.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT),
    };
    (
        DialogChrome {
            min_content_height: min_h,
            ..chrome
        },
        list_h,
    )
}

pub fn overlay_header(kind: OverlayKind) -> DialogHeader {
    match kind {
        OverlayKind::Mode => DialogHeader::title("Agent mode"),
        OverlayKind::Question => DialogHeader::title("Single choice"),
        OverlayKind::MultiChoice => DialogHeader::title("Multiple choice"),
        OverlayKind::UserInput => DialogHeader::title("Your answer"),
        OverlayKind::Confirm => DialogHeader::title("Allow tool"),
        OverlayKind::ConfirmButtons => DialogHeader::title("Confirm action"),
        OverlayKind::TodoList => DialogHeader::title("Session goals"),
        OverlayKind::TodoProgress => DialogHeader::title("Goal progress"),
    }
}
