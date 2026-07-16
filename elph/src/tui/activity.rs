//! Dynamic activity labels and braille spinner for the status row.

use chrono::{DateTime, Local, Utc};

use crate::agent::AgentUiEvent;
use elph_tui::loader::SpinnerLoader;

/// Dimmed submit-time label for user-input transcript cards.
pub fn format_submitted_timestamp(at: DateTime<Utc>) -> String {
    let local = at.with_timezone(&Local);
    let now = Local::now();
    if local.date_naive() == now.date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b %d, %H:%M").to_string()
    }
}

/// Inline dimmed timestamp for layout/render (`14:32`).
pub fn format_submitted_timestamp_suffix(at: DateTime<Utc>) -> String {
    format_submitted_timestamp(at)
}

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

/// Compact elapsed time: `12.4s` under a minute, then `1m50s`, `1h2m5s`.
pub fn format_duration_secs(elapsed_secs: f64) -> String {
    let secs = elapsed_secs.max(0.0);
    if secs < 60.0 {
        let rounded_tenth = (secs * 10.0).round() / 10.0;
        let whole = rounded_tenth.floor();
        if (rounded_tenth - whole).abs() < 0.05 {
            return format!("{}s", whole as u64);
        }
        return format!("{rounded_tenth:.1}s");
    }

    let total = secs.round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        if seconds > 0 {
            format!("{hours}h{minutes}m{seconds}s")
        } else if minutes > 0 {
            format!("{hours}h{minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if seconds > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{minutes}m")
    }
}

/// Dimmed suffix for completed process rows in the transcript (` · 1.2s`).
pub fn format_duration_label_suffix(elapsed_secs: f64) -> String {
    format!(" · {}", format_duration_secs(elapsed_secs))
}

/// Busy left segment: activity label and current phase timer only.
pub fn format_activity_busy_line(label: &str, phase_elapsed_secs: f64) -> String {
    let phase = format_duration_secs(phase_elapsed_secs);
    if label.is_empty() {
        phase
    } else {
        format!("{label} · {phase}")
    }
}

/// Idle status notice shown briefly after a turn completes.
pub fn format_turn_complete_notice(elapsed_secs: f64) -> String {
    format!("Turn complete · {}", format_duration_secs(elapsed_secs))
}

/// Idle status notice shown briefly after the user cancels an active turn.
pub fn format_turn_canceled_notice(elapsed_secs: f64) -> String {
    format!("Turn canceled · {}", format_duration_secs(elapsed_secs))
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

/// Completed turns plus the in-flight turn (wall-clock total for this session).
pub fn total_session_elapsed_secs(session_elapsed_secs: f64, turn_elapsed_secs: f64) -> f64 {
    session_elapsed_secs + turn_elapsed_secs.max(0.0)
}

/// Right status segment while busy: cumulative session time + optional quit confirm.
pub fn format_session_busy_right_line(
    session_elapsed_secs: f64,
    turn_elapsed_secs: f64,
    quit_confirm_pending: bool,
) -> String {
    let total = format!(
        "{} total",
        format_duration_secs(total_session_elapsed_secs(session_elapsed_secs, turn_elapsed_secs,))
    );
    if quit_confirm_pending {
        format!("{total} · {QUIT_CONFIRM_BUSY_HINT} · Ctrl+C cancel")
    } else {
        format!("{total} · Ctrl+C cancel")
    }
}

/// Idle right segment: action hint only.
pub fn format_session_idle_right_line(idle_action_hint: &str) -> String {
    idle_action_hint.to_string()
}

/// Add a completed turn duration to the session accumulator.
pub fn accumulate_session_elapsed(session_elapsed_secs: f64, turn_elapsed_secs: f64) -> f64 {
    total_session_elapsed_secs(session_elapsed_secs, turn_elapsed_secs)
}

/// Append quit-confirm keys to an arbitrary busy status-row right segment.
#[cfg(test)]
pub fn format_busy_right_with_quit_confirm(base: &str) -> String {
    if base.trim().is_empty() {
        QUIT_CONFIRM_BUSY_HINT.to_string()
    } else {
        format!("{base} · {QUIT_CONFIRM_BUSY_HINT}")
    }
}

/// Conservative token estimate for one streaming delta (matches compaction heuristic).
pub fn estimate_delta_tokens(delta: &str) -> u64 {
    delta.chars().count().div_ceil(4) as u64
}

/// Compact stream delta for the status row (header already shows full context usage).
#[cfg(test)]
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
#[cfg(test)]
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

    #[cfg(test)]
    pub fn active_tokens(&self) -> u64 {
        self.baseline_tokens.saturating_add(self.stream_tokens)
    }

    #[cfg(test)]
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
    fn format_submitted_timestamp_suffix_has_no_separator_prefix() {
        let at = DateTime::parse_from_rfc3339("2026-07-17T14:32:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let suffix = format_submitted_timestamp_suffix(at);
        assert!(!suffix.starts_with('·'));
        assert!(suffix.contains(':'));
    }

    #[test]
    fn format_submitted_timestamp_includes_clock_time() {
        let at = DateTime::parse_from_rfc3339("2026-07-17T14:32:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let label = format_submitted_timestamp(at);
        assert!(label.contains(':'));
        assert!(!label.is_empty());
    }

    #[test]
    fn format_duration_secs_uses_seconds_under_one_minute() {
        assert_eq!(format_duration_secs(1.24), "1.2s");
        assert_eq!(format_duration_secs(12.0), "12s");
        assert_eq!(format_duration_secs(45.0), "45s");
        assert_eq!(format_duration_secs(59.9), "59.9s");
    }

    #[test]
    fn format_duration_secs_uses_minutes_from_sixty_seconds() {
        assert_eq!(format_duration_secs(60.0), "1m");
        assert_eq!(format_duration_secs(90.0), "1m30s");
        assert_eq!(format_duration_secs(110.0), "1m50s");
    }

    #[test]
    fn format_duration_secs_uses_hours_for_long_sessions() {
        assert_eq!(format_duration_secs(3600.0), "1h");
        assert_eq!(format_duration_secs(3661.0), "1h1m1s");
    }

    #[test]
    fn format_duration_label_suffix_matches_duration_format() {
        assert_eq!(format_duration_label_suffix(1.2), " · 1.2s");
        assert_eq!(format_duration_label_suffix(110.0), " · 1m50s");
    }

    #[test]
    fn format_activity_busy_line_includes_elapsed() {
        assert_eq!(format_activity_busy_line("Thinking", 1.2), "Thinking · 1.2s");
    }

    #[test]
    fn format_turn_complete_notice_includes_elapsed() {
        assert_eq!(format_turn_complete_notice(110.0), "Turn complete · 1m50s");
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
    fn format_activity_busy_line_shows_label_and_phase_only() {
        assert_eq!(format_activity_busy_line("Running grep", 0.8), "Running grep · 0.8s");
        assert_eq!(format_activity_busy_line("Thinking", 1.2), "Thinking · 1.2s");
        assert_eq!(format_activity_busy_line("", 2.5), "2.5s");
    }

    #[test]
    fn format_session_busy_right_line_shows_total_and_cancel() {
        assert_eq!(format_session_busy_right_line(40.0, 18.1, false), "58.1s total · Ctrl+C cancel");
        assert_eq!(format_session_busy_right_line(0.0, 110.0, false), "1m50s total · Ctrl+C cancel");
        assert_eq!(
            format_session_busy_right_line(10.0, 3.0, true),
            "13s total · y quit · n stay · Ctrl+C cancel"
        );
    }

    #[test]
    fn total_session_elapsed_includes_in_flight_turn() {
        assert!((total_session_elapsed_secs(12.0, 3.5) - 15.5).abs() < f64::EPSILON);
        assert!((total_session_elapsed_secs(0.0, 4.2) - 4.2).abs() < f64::EPSILON);
    }

    #[test]
    fn format_session_idle_right_line_omits_total() {
        assert_eq!(format_session_idle_right_line("Enter to send"), "Enter to send");
    }

    #[test]
    fn accumulate_session_elapsed_adds_turn_duration() {
        assert!((accumulate_session_elapsed(10.0, 3.5) - 13.5).abs() < f64::EPSILON);
    }

    #[test]
    fn format_busy_right_with_quit_confirm_appends_hint() {
        assert_eq!(format_busy_right_with_quit_confirm("1.2s"), "1.2s · y quit · n stay");
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
