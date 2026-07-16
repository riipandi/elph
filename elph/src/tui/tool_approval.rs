//! Tool approval prompt (replaces the editor until the user chooses).

use iocraft::prelude::*;

use crate::agent::{ToolApprovalChoice, ToolApprovalRequest};

use super::theme::rgb_color;

/// Pending approval retained in shell state until the user responds.
pub struct PendingToolApproval {
    pub tool_name: String,
    pub args_summary: String,
    pub response_tx: tokio::sync::oneshot::Sender<ToolApprovalChoice>,
}

impl PendingToolApproval {
    pub fn from_request(req: ToolApprovalRequest) -> Self {
        Self {
            tool_name: req.tool_name,
            args_summary: req.args_summary,
            response_tx: req.response_tx,
        }
    }

    pub fn respond(self, choice: ToolApprovalChoice) {
        let _ = self.response_tx.send(choice);
    }
}

/// Map a key press to an approval choice while the prompt is active.
pub fn choice_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<ToolApprovalChoice> {
    if modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match code {
        KeyCode::Char('y') | KeyCode::Char('1') => Some(ToolApprovalChoice::Approve),
        KeyCode::Char('a') | KeyCode::Char('2') => Some(ToolApprovalChoice::AllowSession),
        KeyCode::Char('n') | KeyCode::Char('3') => Some(ToolApprovalChoice::Reject),
        KeyCode::Enter => Some(ToolApprovalChoice::Approve),
        KeyCode::Esc => Some(ToolApprovalChoice::Reject),
        _ => None,
    }
}

fn format_args_preview(args: &str, width: usize, max_lines: usize) -> Vec<String> {
    let width = width.max(20);
    let mut lines = Vec::new();
    for paragraph in args.split('\n') {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        let mut start = 0;
        while start < paragraph.len() && lines.len() < max_lines {
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
        if lines.len() >= max_lines {
            break;
        }
    }
    if lines.is_empty() {
        lines.push("(no arguments)".to_string());
    }
    if (args.lines().count() > max_lines || args.len() > width.saturating_mul(max_lines))
        && let Some(last) = lines.last_mut()
        && last.len() < width
    {
        last.push('…');
    }
    lines
}

#[derive(Clone, Default, Props)]
pub struct ToolApprovalPromptProps {
    pub screen_width: u16,
    pub tool_name: String,
    pub args_summary: String,
}

#[component]
pub fn ToolApprovalPrompt(props: &ToolApprovalPromptProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let inner_width = props.screen_width.saturating_sub(4) as usize;
    let args_lines = format_args_preview(&props.args_summary, inner_width, 4);
    let accent = rgb_color((250, 180, 131));

    let arg_elements: Vec<_> = args_lines
        .into_iter()
        .map(|line| {
            element! {
                Text(color: Color::Grey, wrap: TextWrap::NoWrap, content: line)
            }
        })
        .collect();

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
            Text(
                color: accent,
                weight: Weight::Bold,
                wrap: TextWrap::NoWrap,
                content: format!(" Allow tool: {} ", props.tool_name),
            )
            View(flex_direction: FlexDirection::Column, gap: 0u16) {
                #(arg_elements)
            }
            Text(
                color: Color::DarkGrey,
                wrap: TextWrap::NoWrap,
                content: "y/Enter allow once · a allow session · n/Esc deny".to_string(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_mapping_matches_docs() {
        assert_eq!(
            choice_from_key(KeyModifiers::NONE, KeyCode::Char('y')),
            Some(ToolApprovalChoice::Approve)
        );
        assert_eq!(
            choice_from_key(KeyModifiers::NONE, KeyCode::Char('a')),
            Some(ToolApprovalChoice::AllowSession)
        );
        assert_eq!(
            choice_from_key(KeyModifiers::NONE, KeyCode::Char('n')),
            Some(ToolApprovalChoice::Reject)
        );
        assert_eq!(
            choice_from_key(KeyModifiers::NONE, KeyCode::Esc),
            Some(ToolApprovalChoice::Reject)
        );
    }

    #[test]
    fn ignores_modified_keys() {
        assert_eq!(choice_from_key(KeyModifiers::CONTROL, KeyCode::Char('y')), None);
    }

    #[test]
    fn wraps_long_args() {
        let lines = format_args_preview("aaaa bbbb cccc dddd eeee ffff gggg", 10, 3);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.chars().count() <= 20));
    }
}
