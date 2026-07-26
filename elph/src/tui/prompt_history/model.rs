//! Prompt history data + render snapshot.

use crate::tui::slash_palette::list_viewport_cap;
use crate::tui::transcript::{TranscriptMessage, TranscriptStyle};

/// Cap stored entries per session (newest retained when full).
pub const MAX_PROMPT_HISTORY: usize = 100;

/// One selectable history row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptHistoryEntry {
    /// Full text inserted into the prompt on Tab/Enter.
    pub text: String,
}

/// Render-ready snapshot for the floating history palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptHistorySnapshot {
    pub visible: bool,
    /// Newest first.
    pub entries: Vec<PromptHistoryEntry>,
    pub list_height: u16,
    /// Total entries in the session history store (not just the viewport).
    pub total_count: usize,
}

impl Default for PromptHistorySnapshot {
    fn default() -> Self {
        Self::hidden()
    }
}

impl PromptHistorySnapshot {
    pub fn hidden() -> Self {
        Self {
            visible: false,
            entries: Vec::new(),
            list_height: 0,
            total_count: 0,
        }
    }

    pub fn should_render(&self) -> bool {
        self.visible && !self.entries.is_empty()
    }
}

/// Title chip: count + label (file-picker / slash-palette style).
pub fn history_title(total_count: usize) -> String {
    if total_count == 1 {
        "01 History · 1 prompt".to_string()
    } else {
        format!("{:02} History · {total_count} prompts", total_count.min(99))
    }
}

/// Single-line preview for a history row (truncate long pastes).
pub fn entry_preview(text: &str, max_cols: usize) -> String {
    let flat: String = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ↵ ");
    let flat = if flat.is_empty() {
        text.replace('\n', " ↵ ").trim().to_string()
    } else {
        flat
    };
    if max_cols == 0 {
        return String::new();
    }
    // Approximate display width by Unicode scalar count (history previews are mostly ASCII).
    let chars: Vec<char> = flat.chars().collect();
    if chars.len() <= max_cols {
        return if flat.is_empty() { " ".to_string() } else { flat };
    }
    let take = max_cols.saturating_sub(1);
    let mut out: String = chars.into_iter().take(take).collect();
    out.push('…');
    out
}

/// Normalize text for the history store / apply-to-prompt path.
///
/// - Free-form prompts: unchanged (no forced `/`)
/// - Skills: always `/skill:…` (never bare `skill:…`)
/// - Other slash commands: keep a leading `/` when already present (echo sites store `/cmd`)
pub fn normalize_prompt_history_entry(text: &str, style: TranscriptStyle) -> String {
    let t = text.trim();
    if t.is_empty() {
        return String::new();
    }

    // Skills — always `/skill:name[ args]`
    if style == TranscriptStyle::SkillPrompt
        || t.starts_with("skill:")
        || t.starts_with("skill ")
        || t.starts_with("/skill:")
        || t.starts_with("/skill ")
    {
        return normalize_skill_slash(t);
    }

    // Already a slash command / template (`/compact`, `/my-template args`)
    if t.starts_with('/') {
        return t.to_string();
    }

    // Free-form user prose (or legacy stripped slash without markers) — leave as-is.
    // New slash echoes store the leading `/` on the transcript card.
    t.to_string()
}

/// Canonical `/skill:name[ args]` form.
fn normalize_skill_slash(text: &str) -> String {
    let t = text.trim();
    let rest = t.strip_prefix('/').unwrap_or(t);
    let rest = rest
        .strip_prefix("skill:")
        .or_else(|| rest.strip_prefix("skill "))
        .unwrap_or(rest);
    let rest = rest.trim_start_matches(':').trim_start();
    if rest.is_empty() {
        "/skill:".to_string()
    } else {
        format!("/skill:{rest}")
    }
}

/// Push a submitted prompt onto the history store (newest first).
///
/// Skips empty/whitespace-only lines and consecutive duplicates.
/// Prefer [`push_history_entry_styled`] when the transcript style is known.
#[cfg_attr(not(test), allow(dead_code))]
pub fn push_history_entry(history: &mut Vec<String>, text: &str) {
    push_history_entry_styled(history, text, TranscriptStyle::User);
}

/// Push history with style-aware slash/skill normalization.
pub fn push_history_entry_styled(history: &mut Vec<String>, text: &str, style: TranscriptStyle) {
    let normalized = normalize_prompt_history_entry(text, style);
    if normalized.is_empty() {
        return;
    }
    if history.first().is_some_and(|prev| prev == &normalized) {
        return;
    }
    history.insert(0, normalized);
    if history.len() > MAX_PROMPT_HISTORY {
        history.truncate(MAX_PROMPT_HISTORY);
    }
}

/// Seed history from resumed transcript user / skill prompts.
///
/// Transcript order is oldest-first; `push_history_entry` inserts at front so the
/// store ends newest-first.
pub fn seed_history_from_transcript(history: &mut Vec<String>, messages: &[TranscriptMessage]) {
    for message in messages {
        if !matches!(message.style, TranscriptStyle::User | TranscriptStyle::SkillPrompt) {
            continue;
        }
        push_history_entry_styled(history, &message.content, message.style);
    }
}

/// Build a visible snapshot when the palette is open.
pub fn build_snapshot(open: bool, history: &[String], screen_height: u16) -> PromptHistorySnapshot {
    if !open || history.is_empty() {
        return PromptHistorySnapshot {
            visible: false,
            entries: Vec::new(),
            list_height: 0,
            total_count: history.len(),
        };
    }
    let entries: Vec<PromptHistoryEntry> = history
        .iter()
        .map(|text| PromptHistoryEntry { text: text.clone() })
        .collect();
    let list_height = entries.len().min(list_viewport_cap(screen_height)).max(1) as u16;
    PromptHistorySnapshot {
        visible: true,
        total_count: entries.len(),
        list_height,
        entries,
    }
}

/// Whether Arrow Up may open the history palette for this editor state.
pub fn can_open_history(
    prompt_focused: bool,
    draft: &str,
    slash_open: bool,
    file_picker_open: bool,
    history_len: usize,
) -> bool {
    prompt_focused && history_len > 0 && !slash_open && !file_picker_open && draft.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_history_newest_first_dedupes_consecutive() {
        let mut h = Vec::new();
        push_history_entry(&mut h, "one");
        push_history_entry(&mut h, "two");
        push_history_entry(&mut h, "two");
        push_history_entry(&mut h, "one");
        assert_eq!(h, vec!["one".to_string(), "two".to_string(), "one".to_string()]);
    }

    #[test]
    fn seed_from_transcript_orders_newest_first() {
        let messages = vec![
            TranscriptMessage::text("first", TranscriptStyle::User),
            TranscriptMessage::text("thinking", TranscriptStyle::Thinking),
            TranscriptMessage::text("second", TranscriptStyle::User),
            TranscriptMessage::text("skill:x", TranscriptStyle::SkillPrompt),
        ];
        let mut h = Vec::new();
        seed_history_from_transcript(&mut h, &messages);
        assert_eq!(h, vec!["/skill:x".to_string(), "second".to_string(), "first".to_string(),]);
    }

    #[test]
    fn normalize_skill_always_slash_skill_prefix() {
        assert_eq!(
            normalize_prompt_history_entry("skill:tui-design layout", TranscriptStyle::SkillPrompt),
            "/skill:tui-design layout"
        );
        assert_eq!(
            normalize_prompt_history_entry("/skill:tui-design", TranscriptStyle::SkillPrompt),
            "/skill:tui-design"
        );
        assert_eq!(normalize_prompt_history_entry("skill:foo", TranscriptStyle::User), "/skill:foo");
    }

    #[test]
    fn normalize_slash_command_and_freeform() {
        assert_eq!(normalize_prompt_history_entry("/compact", TranscriptStyle::User), "/compact");
        assert_eq!(
            normalize_prompt_history_entry("/review-pr 42", TranscriptStyle::User),
            "/review-pr 42"
        );
        // Free-form must not gain a forced slash.
        assert_eq!(
            normalize_prompt_history_entry("fix this bug please", TranscriptStyle::User),
            "fix this bug please"
        );
        assert_eq!(normalize_prompt_history_entry("one", TranscriptStyle::User), "one");
    }

    #[test]
    fn can_open_requires_empty_draft_and_focus() {
        assert!(can_open_history(true, "", false, false, 1));
        assert!(!can_open_history(false, "", false, false, 1));
        assert!(!can_open_history(true, "x", false, false, 1));
        assert!(!can_open_history(true, "", true, false, 1));
        assert!(!can_open_history(true, "", false, false, 0));
    }

    #[test]
    fn title_includes_count() {
        assert!(history_title(3).contains("3 prompts"));
        assert!(history_title(1).contains("1 prompt"));
    }
}
