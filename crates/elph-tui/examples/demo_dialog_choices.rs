//! User-question dialog gallery — single, multi, free-text, and confirm-with-buttons.
//!
//! All copy is generated with the [`lipsum`] crate (placeholder Latin).
//!
//! ```sh
//! cargo run -p elph-tui --example demo_dialog_choices
//! ```
//!
//! Keys: `1` single · `2` multiple · `3` user input · `4` confirm · `Enter` submit · `Esc` hub · `q` quit

#[path = "common/mod.rs"]
mod common;

use anyhow::Result;
use common::lipsum_mock::{mock_paragraph, mock_select_options, mock_sentence, mock_title};
use elph_tui::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preset {
    Hub,
    SingleChoice,
    MultipleChoice,
    UserInput,
    Confirm,
}

fn dialog_layout(
    width: u16,
    height: u16,
    preset: Preset,
    options: &[SelectOption],
    question: &str,
) -> (DialogChrome, u16) {
    let theme = UiTheme::default();
    let outer_width = width.clamp(48, 76);
    let chrome = DialogChrome {
        width: outer_width,
        ..DialogChrome::default()
    };
    let body_width = chrome.inner_body_width();
    let max_body = dialog_max_content_height(height.saturating_sub(8), &chrome, 4);
    let (min_h, list_h) = match preset {
        Preset::Confirm => {
            let message_rows = dialog_text_rows(&format!("{}\n\n{}", mock_title(), mock_paragraph()), body_width);
            (
                dialog_body_min_height(message_rows.saturating_add(2).min(max_body)),
                DIALOG_SELECT_AUTO_HEIGHT,
            )
        }
        Preset::UserInput => (dialog_body_min_height(6.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT),
        Preset::SingleChoice => {
            dialog_select_body_plan(options, true, body_width, theme, question, 0, Some(max_body), false)
        }
        Preset::MultipleChoice => {
            dialog_select_body_plan(options, true, body_width, theme, question, 1, Some(max_body), false)
        }
        Preset::Hub => (dialog_body_min_height(8.min(max_body)), DIALOG_SELECT_AUTO_HEIGHT),
    };
    (
        DialogChrome {
            min_content_height: min_h,
            ..chrome
        },
        list_h,
    )
}

fn header_for(preset: Preset) -> DialogHeader {
    DialogHeader::title(match preset {
        Preset::SingleChoice => "Single choice",
        Preset::MultipleChoice => "Multiple choice",
        Preset::UserInput => "Your answer",
        Preset::Confirm => "Confirm action",
        Preset::Hub => "Question dialogs",
    })
}

fn append_log(log: &mut State<String>, line: String) {
    let prev = log.read().clone();
    log.set(if prev.is_empty() {
        line
    } else {
        format!("{prev}\n{line}")
    });
}

#[component]
fn Gallery(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (screen_width, screen_height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let theme = UiTheme::default();

    let mut preset = hooks.use_state(|| Preset::Hub);
    let exit = hooks.use_state(|| false);
    let mut result_log = hooks.use_state(|| "Pick a dialog preset (1–4).".to_string());

    let single_selected = hooks.use_state(|| 0usize);
    let multi_cursor = hooks.use_state(|| 0usize);
    let multi_checked = hooks.use_state(|| vec![false; 5]);
    let user_answer = hooks.use_state(String::new);
    let confirm_focus = hooks.use_state(ConfirmButtonFocus::default);

    let single_options = hooks.use_state(|| mock_select_options(4));
    let multi_options = hooks.use_state(|| mock_select_options(5));
    let question_line = hooks.use_state(mock_sentence);
    let confirm_message = hooks.use_state(|| format!("{}\n\n{}", mock_title(), mock_paragraph()));

    hooks.use_terminal_events({
        let mut preset = preset;
        let mut exit = exit;
        let mut result_log = result_log;
        let mut user_answer = user_answer;
        move |event| {
            let TerminalEvent::Key(KeyEvent {
                code, kind, modifiers, ..
            }) = event
            else {
                return;
            };
            if kind == KeyEventKind::Release || !modifiers.is_empty() {
                return;
            }

            match code {
                KeyCode::Char('q') => exit.set(true),
                KeyCode::Esc => preset.set(Preset::Hub),
                KeyCode::Char('1') => preset.set(Preset::SingleChoice),
                KeyCode::Char('2') => preset.set(Preset::MultipleChoice),
                KeyCode::Char('3') => preset.set(Preset::UserInput),
                KeyCode::Char('4') => preset.set(Preset::Confirm),
                KeyCode::Enter if preset.get() == Preset::SingleChoice => {
                    if let Some(opt) = single_options.read().get(single_selected.get()) {
                        append_log(&mut result_log, format!("Single → {}", opt.name));
                        preset.set(Preset::Hub);
                    }
                }
                KeyCode::Enter if preset.get() == Preset::UserInput => {
                    let answer = user_answer.read().clone();
                    if !answer.trim().is_empty() {
                        append_log(&mut result_log, format!("User → {answer}"));
                        user_answer.set(String::new());
                        preset.set(Preset::Hub);
                    }
                }
                _ => {}
            }
        }
    });

    if exit.get() {
        system.exit();
    }

    let active = preset.get();
    let options_single = single_options.read().clone();
    let options_multi = multi_options.read().clone();
    let question = question_line.read().clone();
    let options_for_layout = match active {
        Preset::SingleChoice => options_single.as_slice(),
        Preset::MultipleChoice => options_multi.as_slice(),
        _ => &[],
    };
    let (chrome, list_height) = dialog_layout(screen_width, screen_height, active, options_for_layout, &question);
    let body_width = chrome.inner_body_width();

    let hub = element! {
        View(flex_direction: FlexDirection::Column, gap: 1) {
            Text(content: "1 — single choice (pick one option)".to_string(), color: theme.text_secondary, wrap: TextWrap::Wrap)
            Text(content: "2 — multiple choice (Space toggle, Enter confirm)".to_string(), color: theme.text_secondary, wrap: TextWrap::Wrap)
            Text(content: "3 — user input (type a free-text answer)".to_string(), color: theme.text_secondary, wrap: TextWrap::Wrap)
            Text(content: "4 — confirmation (Yes / No buttons)".to_string(), color: theme.text_secondary, wrap: TextWrap::Wrap)
            Text(content: "Esc returns here · q quits.".to_string(), color: theme.text_muted, wrap: TextWrap::Wrap)
        }
    };

    let body: AnyElement<'static> = match active {
        Preset::Hub => hub.into(),
        Preset::SingleChoice => element! {
            DialogQuestionContent(
                width: body_width,
                height: list_height,
                question: question.clone(),
                options: options_single,
                selected_index: single_selected,
                has_focus: true,
                show_description: true,
                question_color: theme.text_secondary,
            )
        }
        .into(),
        Preset::MultipleChoice => element! {
            DialogMultiChoiceContent(
                width: body_width,
                height: list_height,
                question: question.clone(),
                options: options_multi.clone(),
                cursor_index: multi_cursor,
                checked: multi_checked,
                has_focus: true,
                show_description: true,
                theme: Some(theme),
                on_submit: move |picked: Vec<usize>| {
                    let labels: Vec<_> = picked
                        .iter()
                        .filter_map(|i| options_multi.get(*i).map(|o| o.name.clone()))
                        .collect();
                    let line = if labels.is_empty() {
                        "Multiple → (none selected)".to_string()
                    } else {
                        format!("Multiple → {}", labels.join(", "))
                    };
                    append_log(&mut result_log, line);
                    preset.set(Preset::Hub);
                },
            )
        }
        .into(),
        Preset::UserInput => element! {
            DialogUserInputContent(
                width: body_width,
                question: question.clone(),
                placeholder: mock_title(),
                value: user_answer,
                has_focus: true,
                theme: Some(theme),
            )
        }
        .into(),
        Preset::Confirm => element! {
            DialogConfirmButtonsContent(
                width: body_width,
                message: confirm_message.read().clone(),
                yes_label: "Yes".to_string(),
                no_label: "No".to_string(),
                focused_button: confirm_focus,
                has_focus: true,
                theme: Some(theme),
                on_yes: move |_| {
                    append_log(&mut result_log, "Confirm → Yes".to_string());
                    preset.set(Preset::Hub);
                },
                on_no: move |_| {
                    append_log(&mut result_log, "Confirm → No".to_string());
                    preset.set(Preset::Hub);
                },
            )
        }
        .into(),
    };

    element! {
        View(
            width: screen_width,
            height: screen_height,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: 2,
            gap: 1,
        ) {
            DialogShell(chrome: chrome, header: header_for(active), on_copy: HandlerMut::default()) {
                #(body)
            }
            View(
                width: screen_width.saturating_sub(4),
                max_height: 6u16,
                border_style: BorderStyle::Single,
                border_color: theme.border_subtle,
                padding: 1,
            ) {
                Text(
                    content: result_log.read().clone(),
                    color: theme.text_muted,
                    wrap: TextWrap::Wrap,
                )
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    element!(Gallery).render_loop().fullscreen().await?;
    Ok(())
}
