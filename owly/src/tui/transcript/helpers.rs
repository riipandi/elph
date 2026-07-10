use crate::ui_events::AgentUiEvent;

use super::TranscriptApplier;
use crate::tui::entries::OwlyEntry;

/// Convert plain startup hint lines into transcript entries (skips blanks).
pub fn lines_to_entries(lines: &[String]) -> Vec<OwlyEntry> {
    lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| OwlyEntry::hint(line.clone()))
        .collect()
}

/// Append static shell output lines after a command finishes.
pub fn append_shell_lines(entries: &mut Vec<OwlyEntry>, lines: &[String]) {
    let mut live_tools = Vec::new();
    let mut applier = TranscriptApplier::new(entries, &mut live_tools, false);
    for line in lines {
        applier.apply(AgentUiEvent::Status(line.clone()));
    }
}
