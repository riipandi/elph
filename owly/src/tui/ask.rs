//! TUI prompts for ask_* agent tools.

use elph_tui::{DEFAULT_TRANSCRIPT_CAP, SelectItem, Theme, push_capped};
use slt::{Border, Context, KeyCode, ListState};

use crate::tui::entries::OwlyEntry;
use crate::ui_events::{AskUserKind, AskUserResponse};

#[derive(Debug)]
pub struct PendingAsk {
    pub _tool_call_id: String,
    pub tool_name: String,
    pub question: String,
    pub kind: AskUserKind,
    pub response_tx: tokio::sync::oneshot::Sender<AskUserResponse>,
    pub selected: usize,
}

impl PendingAsk {
    pub fn is_text(&self) -> bool {
        matches!(self.kind, AskUserKind::Text { .. })
    }

    pub fn push_transcript_notice(&self, entries: &mut Vec<OwlyEntry>) {
        let line = format!("{} asks: {}", self.tool_name, self.question);
        push_capped(entries, OwlyEntry::status(&line), DEFAULT_TRANSCRIPT_CAP);
    }

    pub fn finish_with_answer(self, answer: String) {
        let _ = self.response_tx.send(AskUserResponse::Answered(answer));
    }

    pub fn finish_cancelled(self) {
        let _ = self.response_tx.send(AskUserResponse::Cancelled);
    }
}

pub enum AskModalAction {
    None,
    Answered(String),
}

pub fn handle_ask_modal_keys(ui: &mut Context, pending: &mut PendingAsk) -> AskModalAction {
    if pending.is_text() {
        return AskModalAction::None;
    }

    let item_count = match &pending.kind {
        AskUserKind::Select { options, .. } => options.len(),
        AskUserKind::Confirm { .. } => 2,
        AskUserKind::Text { .. } => return AskModalAction::None,
    };
    if item_count == 0 {
        return AskModalAction::None;
    }

    if ui.raw_key_code(KeyCode::Up) {
        pending.selected = pending.selected.saturating_sub(1);
    }
    if ui.raw_key_code(KeyCode::Down) {
        pending.selected = (pending.selected + 1).min(item_count.saturating_sub(1));
    }
    if ui.raw_key_code(KeyCode::Enter) {
        let answer = match &pending.kind {
            AskUserKind::Select { options, .. } => options.get(pending.selected).cloned().unwrap_or_default(),
            AskUserKind::Confirm { .. } => {
                if pending.selected == 0 {
                    "yes".to_string()
                } else {
                    "no".to_string()
                }
            }
            AskUserKind::Text { .. } => String::new(),
        };
        return AskModalAction::Answered(answer);
    }
    AskModalAction::None
}

pub fn render_ask_modal(ui: &mut Context, pending: &PendingAsk, theme: Theme) -> usize {
    if pending.is_text() {
        return pending.selected;
    }

    let (items, default_index) = match &pending.kind {
        AskUserKind::Select { options, default_index } => {
            let items: Vec<SelectItem> = options
                .iter()
                .map(|opt| SelectItem::new(opt.as_str(), opt.as_str()))
                .collect();
            (items, *default_index)
        }
        AskUserKind::Confirm { default } => {
            let items = vec![SelectItem::new("yes", "Yes"), SelectItem::new("no", "No")];
            let default_index = if *default { 0 } else { 1 };
            (items, default_index)
        }
        AskUserKind::Text { .. } => return pending.selected,
    };

    if items.is_empty() {
        return pending.selected;
    }

    let labels: Vec<String> = items
        .iter()
        .map(|item| match &item.description {
            Some(desc) => format!("{} — {}", item.label, desc),
            None => item.label.clone(),
        })
        .collect();
    let mut list = ListState::new(labels);
    list.selected = pending.selected.min(items.len().saturating_sub(1));

    let modal_width = (ui.width().saturating_mul(70) / 100).clamp(24, ui.width());
    let title = format!("{} — {}", pending.tool_name, pending.question);

    let _ = ui.modal(|ui| {
        let _ = ui
            .bordered(Border::Rounded)
            .border_fg(theme.mode_border_color(elph_tui::AgentMode::Ask))
            .p(1)
            .w(modal_width)
            .col(|ui| {
                let _ = ui.text(&title).bold();
                let _ = ui.list(&mut list);
            });
    });

    let _ = default_index;
    list.selected
}

pub fn resolve_text_answer(text: String, kind: &AskUserKind) -> String {
    if let AskUserKind::Text { default } = kind
        && text.trim().is_empty()
        && let Some(d) = default
        && !d.is_empty()
    {
        return d.clone();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_text_answer_uses_default_when_empty() {
        let kind = AskUserKind::Text {
            default: Some("fallback".into()),
        };
        assert_eq!(resolve_text_answer("  ".into(), &kind), "fallback");
    }

    #[test]
    fn resolve_text_answer_keeps_non_empty_input() {
        let kind = AskUserKind::Text {
            default: Some("fallback".into()),
        };
        assert_eq!(resolve_text_answer("custom".into(), &kind), "custom");
    }
}
