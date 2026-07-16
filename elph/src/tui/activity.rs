//! Dynamic activity labels and braille spinner for the status row.

use crate::agent::AgentUiEvent;
use elph_tui::loader::SpinnerLoader;

/// Braille spinner glyph for the given animation tick (parent-driven, non-blocking).
pub fn braille_spinner_glyph(tick: u32) -> &'static str {
    let mut spinner = SpinnerLoader::new();
    let frames = 10usize;
    for _ in 0..(tick as usize % frames) {
        spinner.tick();
    }
    spinner.glyph()
}

/// Normalize free-form agent status strings into short UI labels.
pub fn normalize_agent_status(line: &str) -> String {
    let line = line.trim();
    if line.is_empty() {
        return String::new();
    }
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("thinking") {
        return "Thinking".to_string();
    }
    if lower.starts_with("responding") || lower.contains("streaming") {
        return "Responding".to_string();
    }
    if lower.starts_with("cancelling") || lower.starts_with("canceling") {
        return "Cancelling".to_string();
    }
    if lower.starts_with("steering") {
        return "Steering".to_string();
    }
    if lower.starts_with("error") {
        return truncate_status(line, 40);
    }
    if lower.starts_with("running ") {
        return truncate_status(line, 40);
    }
    truncate_status(line, 40)
}

/// Map a live agent event to a short activity label, when applicable.
pub fn activity_label_for_event(event: &AgentUiEvent, show_thinking: bool) -> Option<String> {
    match event {
        AgentUiEvent::Status(line) => {
            let normalized = normalize_agent_status(line);
            if normalized.is_empty() { None } else { Some(normalized) }
        }
        AgentUiEvent::TextDelta(_) => Some("Responding".to_string()),
        AgentUiEvent::ThinkingDelta(_) if show_thinking => Some("Thinking".to_string()),
        AgentUiEvent::ToolStart { name, .. } => Some(format!("Running {name}")),
        AgentUiEvent::ToolEnd { .. } => Some("Thinking".to_string()),
        AgentUiEvent::SubagentStatus { message, .. } => Some(format!("Subagent · {message}")),
        AgentUiEvent::PlanConfirmationRequired(_) => Some("Awaiting plan approval".to_string()),
        AgentUiEvent::ToolApprovalRequired(_) => Some("Awaiting tool approval".to_string()),
        AgentUiEvent::UserQuestionRequired(_) => Some("Awaiting your answer".to_string()),
        AgentUiEvent::GoalUpdated { .. } => Some("Updating goal".to_string()),
        AgentUiEvent::RunCompleted { .. } | AgentUiEvent::ToolUpdate { .. } | AgentUiEvent::ThinkingDelta(_) => None,
    }
}

/// Format the left status segment: `Thinking · 1.2s`.
pub fn format_activity_line(label: &str, elapsed_secs: f64) -> String {
    if label.is_empty() {
        format!("{elapsed_secs:.1}s")
    } else {
        format!("{label} · {elapsed_secs:.1}s")
    }
}

/// Idle status notice shown briefly after a turn completes.
pub fn format_turn_complete_notice(elapsed_secs: f64) -> String {
    format!("Turn complete · {elapsed_secs:.1}s")
}

fn truncate_status(line: &str, max_chars: usize) -> String {
    if line.chars().count() <= max_chars {
        return line.to_string();
    }
    let truncated: String = line.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_thinking_status() {
        assert_eq!(normalize_agent_status("Thinking…"), "Thinking");
        assert_eq!(normalize_agent_status("  thinking "), "Thinking");
    }

    #[test]
    fn maps_text_delta_to_responding() {
        assert_eq!(
            activity_label_for_event(&AgentUiEvent::TextDelta("hi".into()), false),
            Some("Responding".to_string())
        );
    }

    #[test]
    fn maps_tool_start_to_running_label() {
        assert_eq!(
            activity_label_for_event(
                &AgentUiEvent::ToolStart {
                    id: "1".into(),
                    name: "read_file".into(),
                    args_summary: "{}".into(),
                },
                false
            ),
            Some("Running read_file".to_string())
        );
    }

    #[test]
    fn braille_spinner_cycles() {
        assert_eq!(braille_spinner_glyph(0), "⠋");
        assert_eq!(braille_spinner_glyph(1), "⠙");
    }

    #[test]
    fn format_activity_line_includes_elapsed() {
        assert_eq!(format_activity_line("Thinking", 1.2), "Thinking · 1.2s");
    }

    #[test]
    fn format_turn_complete_notice_includes_elapsed() {
        assert_eq!(format_turn_complete_notice(3.45), "Turn complete · 3.5s");
    }
}
