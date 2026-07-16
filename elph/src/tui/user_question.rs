//! Ask-user prompt (replaces the editor until the user answers).

use iocraft::prelude::*;

use crate::agent::{UserQuestionOption, UserQuestionRequest};

use super::theme::rgb_color;

/// Pending question retained in shell state until the user responds.
pub struct PendingUserQuestion {
    pub question: String,
    pub options: Option<Vec<UserQuestionOption>>,
    pub default: Option<String>,
    pub is_confirm: bool,
    response_tx: tokio::sync::oneshot::Sender<String>,
}

impl PendingUserQuestion {
    pub fn from_request(req: UserQuestionRequest) -> Self {
        let is_confirm = req.options.is_none()
            && req
                .default
                .as_ref()
                .is_some_and(|value| value == "true" || value == "false");
        Self {
            question: req.question,
            options: req.options,
            default: req.default,
            is_confirm,
            response_tx: req.response_tx,
        }
    }

    pub fn needs_text_input(&self) -> bool {
        !self.is_confirm && self.options.is_none()
    }

    pub fn respond(self, answer: String) {
        let _ = self.response_tx.send(answer);
    }

    pub fn respond_confirm(self, yes: bool) {
        let _ = self.response_tx.send(yes.to_string());
    }

    pub fn respond_option(self, value: String) {
        let _ = self.response_tx.send(value);
    }
}

/// Map a key press to a select option index (1-based).
pub fn option_index_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<usize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char(c @ '1'..='9') => Some((c as u8 - b'0') as usize),
        _ => None,
    }
}

/// Map y/n to confirm answers.
pub fn confirm_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<bool> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => Some(true),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Some(false),
        _ => None,
    }
}

#[derive(Clone, Default, Props)]
pub struct UserQuestionPromptProps {
    pub screen_width: u16,
    pub question: String,
    pub options: Option<Vec<UserQuestionOption>>,
    pub is_confirm: bool,
    pub needs_text_input: bool,
}

#[component]
pub fn UserQuestionPrompt(props: &UserQuestionPromptProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let accent = rgb_color((129, 161, 193));
    let inner_width = props.screen_width.saturating_sub(4) as usize;

    let mut hint = String::new();
    if props.is_confirm {
        hint.push_str("y/Enter yes · n/Esc no");
    } else if let Some(options) = &props.options {
        for (index, option) in options.iter().enumerate().take(9) {
            if index > 0 {
                hint.push_str(" · ");
            }
            hint.push_str(&format!("{} {}", index + 1, option.label));
        }
    } else if props.needs_text_input {
        hint.push_str("Type your answer below and press Enter");
    }

    let option_lines: Vec<AnyElement<'static>> = props
        .options
        .as_ref()
        .map(|options| {
            options
                .iter()
                .enumerate()
                .take(9)
                .map(|(index, option)| {
                    element! {
                        Text(
                            color: Color::Grey,
                            wrap: TextWrap::NoWrap,
                            content: format!("{}. {}", index + 1, option.label),
                        )
                    }
                    .into()
                })
                .collect()
        })
        .unwrap_or_default();

    let question_lines = wrap_question(&props.question, inner_width);
    let question_elements: Vec<AnyElement<'static>> = question_lines
        .into_iter()
        .map(|line| {
            element! {
                Text(color: Color::White, wrap: TextWrap::NoWrap, content: line)
            }
            .into()
        })
        .collect();

    let mut children: Vec<AnyElement<'static>> = vec![
        element! {
            Text(
                color: accent,
                weight: Weight::Bold,
                wrap: TextWrap::NoWrap,
                content: " Agent question ".to_string(),
            )
        }
        .into(),
        element! {
            View(flex_direction: FlexDirection::Column, gap: 0u16) {
                #(question_elements)
            }
        }
        .into(),
    ];
    if !option_lines.is_empty() {
        children.push(
            element! {
                View(flex_direction: FlexDirection::Column, gap: 0u16) {
                    #(option_lines)
                }
            }
            .into(),
        );
    }
    if !hint.is_empty() {
        children.push(
            element! {
                Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: hint)
            }
            .into(),
        );
    }

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            border_style: BorderStyle::Round,
            border_color: accent,
            position: Position::Relative,
            flex_direction: FlexDirection::Column,
            gap: 1u16,
            padding_top: 1,
            padding_bottom: 1,
            padding_left: 1,
            padding_right: 1,
        ) {
            #(children)
        }
    }
}

fn wrap_question(text: &str, width: usize) -> Vec<String> {
    let width = width.max(20);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        let mut start = 0;
        while start < paragraph.len() {
            let end = (start + width).min(paragraph.len());
            let mut slice_end = end;
            if end < paragraph.len()
                && let Some(rel) = paragraph[start..end].rfind(' ')
                && rel > width / 3
            {
                slice_end = start + rel;
            }
            lines.push(paragraph[start..slice_end].trim().to_string());
            start = slice_end;
            while start < paragraph.len() && paragraph.as_bytes()[start] == b' ' {
                start += 1;
            }
        }
    }
    if lines.is_empty() {
        lines.push(text.to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_keys_map_to_bool() {
        assert_eq!(confirm_from_key(KeyModifiers::NONE, KeyCode::Char('y')), Some(true));
        assert_eq!(confirm_from_key(KeyModifiers::NONE, KeyCode::Char('n')), Some(false));
        assert_eq!(confirm_from_key(KeyModifiers::NONE, KeyCode::Esc), Some(false));
    }

    #[test]
    fn option_keys_are_one_based() {
        assert_eq!(option_index_from_key(KeyModifiers::NONE, KeyCode::Char('1')), Some(1));
        assert_eq!(option_index_from_key(KeyModifiers::NONE, KeyCode::Char('3')), Some(3));
    }
}
