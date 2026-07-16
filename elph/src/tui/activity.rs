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

/// Idle status notice shown briefly after the user cancels an active turn.
pub fn format_turn_canceled_notice(elapsed_secs: f64) -> String {
    format!("Turn canceled · {elapsed_secs:.1}s")
}

/// Transcript notice when quit is requested while a turn is still running.
pub fn format_quit_while_busy_transcript() -> String {
    "Agent is still responding. Press y to quit (cancels the turn), n to keep waiting, or repeat /exit, :q, or Ctrl+D to confirm."
        .to_string()
}

/// Status-row suffix while quit confirmation is pending during an active turn.
pub const QUIT_CONFIRM_BUSY_HINT: &str = "y quit · n stay";

/// Brief status notice after the user dismisses a pending quit.
pub fn format_quit_canceled_notice() -> String {
    "Quit canceled".to_string()
}

/// Append quit-confirm keys to the busy status-row right segment when needed.
pub fn format_busy_right_with_quit_confirm(base: &str) -> String {
    if base.trim().is_empty() {
        QUIT_CONFIRM_BUSY_HINT.to_string()
    } else {
        format!("{base} | {QUIT_CONFIRM_BUSY_HINT}")
    }
}

/// Conservative token estimate for one streaming delta (matches compaction heuristic).
pub fn estimate_delta_tokens(delta: &str) -> u64 {
    delta.chars().count().div_ceil(4) as u64
}

/// Compact stream delta for the status row (header already shows full context usage).
pub fn format_stream_token_delta(stream_tokens: u64) -> String {
    if stream_tokens == 0 {
        return String::new();
    }
    if stream_tokens >= 1000 {
        format!("+{}k", stream_tokens / 1000)
    } else {
        format!("+{stream_tokens}")
    }
}

/// Live turn throughput on the status row — stream delta + TPS only, not full context stats.
pub fn format_busy_token_info(stream_tokens: u64, tokens_per_sec: f64) -> String {
    let tps = format!("{tokens_per_sec:.0} t/s");
    let delta = format_stream_token_delta(stream_tokens);
    if delta.is_empty() {
        tps
    } else {
        format!("{delta} · {tps}")
    }
}

/// Tracks in-flight stream tokens on top of a turn-start baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnTokenTracker {
    pub baseline_tokens: u64,
    pub stream_tokens: u64,
}

impl TurnTokenTracker {
    pub fn new(baseline_tokens: u64) -> Self {
        Self {
            baseline_tokens,
            stream_tokens: 0,
        }
    }

    pub fn record_delta(&mut self, delta: &str) {
        self.stream_tokens = self.stream_tokens.saturating_add(estimate_delta_tokens(delta));
    }

    pub fn sync_baseline(&mut self, tokens_used: u64) {
        if tokens_used > self.baseline_tokens {
            self.baseline_tokens = tokens_used;
            self.stream_tokens = 0;
        }
    }

    pub fn active_tokens(&self) -> u64 {
        self.baseline_tokens.saturating_add(self.stream_tokens)
    }

    pub fn tokens_per_sec(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs <= f64::EPSILON {
            return 0.0;
        }
        self.stream_tokens as f64 / elapsed_secs
    }
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

    #[test]
    fn format_turn_canceled_notice_includes_elapsed() {
        assert_eq!(format_turn_canceled_notice(2.1), "Turn canceled · 2.1s");
    }

    #[test]
    fn estimate_delta_tokens_uses_char_heuristic() {
        assert_eq!(estimate_delta_tokens("12345678"), 2);
    }

    #[test]
    fn format_stream_token_delta_prefixes_increment() {
        assert_eq!(format_stream_token_delta(0), "");
        assert_eq!(format_stream_token_delta(240), "+240");
        assert_eq!(format_stream_token_delta(1_200), "+1k");
    }

    #[test]
    fn format_busy_right_with_quit_confirm_appends_hint() {
        assert_eq!(
            format_busy_right_with_quit_confirm("+240 · 12 t/s"),
            "+240 · 12 t/s | y quit · n stay"
        );
        assert_eq!(format_busy_right_with_quit_confirm(""), QUIT_CONFIRM_BUSY_HINT);
    }

    #[test]
    fn format_quit_while_busy_transcript_mentions_confirm_keys() {
        let notice = format_quit_while_busy_transcript();
        assert!(notice.contains("y"));
        assert!(notice.contains("/exit"));
        assert!(notice.contains(":q"));
        assert!(notice.contains("Ctrl+D"));
    }

    #[test]
    fn format_busy_token_info_is_compact() {
        assert_eq!(format_busy_token_info(0, 0.0), "0 t/s");
        assert_eq!(format_busy_token_info(240, 12.4), "+240 · 12 t/s");
        assert_eq!(format_busy_token_info(1_200, 45.0), "+1k · 45 t/s");
    }

    #[test]
    fn turn_token_tracker_accumulates_and_computes_tps() {
        let mut tracker = TurnTokenTracker::new(100);
        tracker.record_delta("hello world");
        assert_eq!(tracker.active_tokens(), 103);
        assert!((tracker.tokens_per_sec(2.0) - 1.5).abs() < f64::EPSILON);
        tracker.sync_baseline(150);
        assert_eq!(tracker.active_tokens(), 150);
    }
}
