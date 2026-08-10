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

/// Braille spinner for a paint counter (each step ≈ one frame). Prefer [`braille_spinner_glyph_now`].
#[cfg_attr(not(test), allow(dead_code))]
pub fn braille_spinner_glyph(tick: u32) -> &'static str {
    SpinnerLoader::glyph_for_elapsed_ms(u64::from(tick).saturating_mul(80))
}

/// Live braille frame from wall clock — skips frames under load (no slow-mo / fake freeze).
#[cfg_attr(not(test), allow(dead_code))]
pub fn braille_spinner_glyph_now() -> &'static str {
    SpinnerLoader::glyph_now()
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
    if lower.starts_with("error")
        || lower.starts_with("authentication failed")
        || lower.starts_with("permission denied")
        || lower.starts_with("rate limited")
        || lower.starts_with("request conflict")
        || lower.starts_with("api request failed")
        || lower.starts_with("provider server error")
    {
        return truncate_status(line, 48);
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
        AgentUiEvent::ToolStart { name, .. } => {
            let base = name.rsplit("__").next().unwrap_or(name.as_str());
            if base == "wait_agent" {
                // Match process-row wording (status + agent id in transcript).
                Some("Waiting for subagent".to_string())
            } else {
                let verb = crate::tui::tool_params::tool_display_verb(name);
                Some(format!("Running {verb}"))
            }
        }
        AgentUiEvent::ToolEnd { .. } => Some("Thinking".to_string()),
        AgentUiEvent::SubagentStatus {
            agent_id,
            agent_path,
            task_name,
            message,
            ..
        } => Some(crate::tui::subagent_display::format_subagent_activity_label(
            task_name, agent_path, agent_id, message,
        )),
        AgentUiEvent::PlanConfirmationRequired(_) => Some("Awaiting plan approval".to_string()),
        AgentUiEvent::ToolApprovalRequired(_) => Some("Awaiting tool approval".to_string()),
        AgentUiEvent::UserQuestionRequired(_) => Some("Awaiting your answer".to_string()),
        AgentUiEvent::GoalUpdated { .. } => Some("Updating goal".to_string()),
        AgentUiEvent::Retrying { attempt } => {
            if *attempt > 1 {
                Some(format!("Retrying… attempt {attempt}"))
            } else {
                Some("Retrying…".to_string())
            }
        }
        AgentUiEvent::RunCompleted { .. }
        | AgentUiEvent::ToolUpdate { .. }
        | AgentUiEvent::ThinkingDelta(_)
        | AgentUiEvent::QueueUpdate { .. }
        | AgentUiEvent::MemoryResult(_)
        | AgentUiEvent::AsideStarted { .. }
        | AgentUiEvent::AsideFinished { .. }
        | AgentUiEvent::AsideFailed { .. }
        | AgentUiEvent::WorkerInboundStarted { .. }
        | AgentUiEvent::WorkerInboundAnswered { .. }
        | AgentUiEvent::WorkerInboundFailed { .. }
        | AgentUiEvent::UserPromptCommitted { .. }
        | AgentUiEvent::TranscriptNotice(_)
        | AgentUiEvent::SubagentOutput { .. }
        | AgentUiEvent::RetryablePrompt(_)
        | AgentUiEvent::TodoUpdated { .. } => None,
        AgentUiEvent::ModeChangeRequired(req) => Some(format!("Switch to {} mode?", req.target_mode)),
    }
}

/// Compact, auto-scaled duration for transcript / log / activity chrome.
///
/// Shared implementation: [`elph_tui::format_duration_secs`] (ns → us → ms → s → m → h).
/// Smallest unit is nanoseconds so sub-millisecond work never prints `0ms` / `0s`.
pub fn format_duration_secs(elapsed_secs: f64) -> String {
    elph_tui::format_duration_secs(elapsed_secs)
}

/// Dimmed suffix for completed process rows in the transcript (` · 1.2s` / ` · 45ms`).
pub fn format_duration_label_suffix(elapsed_secs: f64) -> String {
    // U+00B7 MIDDLE DOT — same separator as process status meta chips.
    format!(" \u{00B7} {}", format_duration_secs(elapsed_secs))
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

/// Thinking threshold in seconds before the status row starts joking.
pub const THINKING_OVERDUE_THRESHOLD_SECS: f64 = 10.0;

/// Pool of witty labels for a thinking phase that drags on past the threshold.
/// Order is shuffled per phase start (see [`thinking_overdue_label`]) so messages
/// do not always escalate in the same sequence.
const OVERDUE_THINKING_MESSAGES: &[&str] = &[
    "Hold on, still over thinking...",
    "Still over thinking... going deeper",
    "Thinking so hard it might overheat",
    "Did it forget the question?",
    "Is it thinking, or napping?",
    "Thinking at the speed of molasses...",
    "I won't give up, sit tight...",
    "Still cooking… almost done?",
    "Deep in the thought mines…",
    "Consulting the silicon oracle…",
];

/// Escalating witty label for a thinking phase that exceeds 10s.
///
/// Returns `None` below the threshold; afterwards picks a message from a
/// **per-phase shuffled** permutation of [`OVERDUE_THINKING_MESSAGES`] so
/// long waits stay fresh and non-repetitive across turns.
pub fn thinking_overdue_label(elapsed_secs: f64) -> Option<&'static str> {
    if !elapsed_secs.is_finite() || elapsed_secs < THINKING_OVERDUE_THRESHOLD_SECS {
        return None;
    }
    let bucket = (elapsed_secs / THINKING_OVERDUE_THRESHOLD_SECS) as usize;
    let order = overdue_message_order();
    let idx = bucket.saturating_sub(1).min(order.len().saturating_sub(1));
    Some(OVERDUE_THINKING_MESSAGES[order[idx]])
}

/// Fisher–Yates shuffle of message indices, re-rolled when the thinking phase resets
/// (elapsed just crossed the first overdue bucket after being below threshold is handled
/// by callers re-querying each tick — we reseed when bucket==1 and cache via thread-local).
fn overdue_message_order() -> Vec<usize> {
    use std::cell::RefCell;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    thread_local! {
        static ORDER: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
        static SEED_SLOT: RefCell<u64> = const { RefCell::new(0) };
    }

    // New seed roughly every thinking phase: use coarse clock + reseed when order empty.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 30) // re-shuffle window ~30s
        .unwrap_or(0);

    ORDER.with(|order| {
        SEED_SLOT.with(|seed| {
            let mut order = order.borrow_mut();
            let mut seed = seed.borrow_mut();
            if order.is_empty() || *seed != now {
                *seed = now;
                let n = OVERDUE_THINKING_MESSAGES.len();
                let mut idx: Vec<usize> = (0..n).collect();
                // Deterministic shuffle from seed
                let mut h = DefaultHasher::new();
                now.hash(&mut h);
                let mut state = h.finish();
                for i in (1..n).rev() {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let j = (state as usize) % (i + 1);
                    idx.swap(i, j);
                }
                *order = idx;
            }
            order.clone()
        })
    })
}

/// Busy left segment with long-thinking escalation (see [`thinking_overdue_label`]).
pub fn format_activity_busy_line_dynamic(label: &str, phase_elapsed_secs: f64) -> String {
    if label == "Thinking"
        && let Some(witty) = thinking_overdue_label(phase_elapsed_secs)
    {
        return format!("{witty} · {}", format_duration_secs(phase_elapsed_secs));
    }
    format_activity_busy_line(label, phase_elapsed_secs)
}

/// Idle status notice shown briefly after a turn completes.
pub fn format_turn_complete_notice(elapsed_secs: f64) -> String {
    format!("Turn complete · {}", format_duration_secs(elapsed_secs))
}

/// Per-turn completion stats rendered as a dimmed Meta card below the last assistant reply.
///
/// Populated from the harness `session_turns` record at `RunCompleted`; all fields optional
/// so a missing store / legacy record degrades to a duration-only line.
#[derive(Debug, Clone, Default)]
pub struct TurnCompleteStats {
    pub elapsed_secs: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
}

impl TurnCompleteStats {
    pub fn from_event(
        elapsed_secs: f64,
        usage: Option<&elph_agent::TurnUsage>,
        provider_id: Option<&str>,
        model_id: Option<&str>,
    ) -> Self {
        Self {
            elapsed_secs,
            input_tokens: usage.map(|u| u.input_tokens.max(0) as u64).unwrap_or(0),
            output_tokens: usage.map(|u| u.output_tokens.max(0) as u64).unwrap_or(0),
            cache_read_tokens: usage.map(|u| u.cache_read_tokens.max(0) as u64).unwrap_or(0),
            cache_write_tokens: usage.map(|u| u.cache_write_tokens.max(0) as u64).unwrap_or(0),
            provider_id: provider_id.map(str::to_string),
            model_id: model_id.map(str::to_string),
        }
    }

    /// `provider/model` label, omitting the provider when either side is missing.
    pub fn model_label(&self) -> Option<String> {
        match (&self.provider_id, &self.model_id) {
            (Some(provider), Some(model)) if !provider.is_empty() && !model.is_empty() => {
                Some(format!("{provider}/{model}"))
            }
            (_, Some(model)) if !model.is_empty() => Some(model.clone()),
            _ => None,
        }
    }

    /// Compact token usage line:
    /// `3K/2K ↓↑ · 1K/0 ↓↑ cached` (input/output pair and cache read/write pair;
    /// `↓` = sent to the API, `↑` = received from it).
    fn tokens_line(&self) -> String {
        let mut parts = Vec::new();
        if self.input_tokens > 0 || self.output_tokens > 0 {
            parts.push(format!(
                "{}/{} ↓↑",
                crate::tui::labels::format_token_count(self.input_tokens),
                crate::tui::labels::format_token_count(self.output_tokens),
            ));
        }
        if self.cache_read_tokens > 0 || self.cache_write_tokens > 0 {
            parts.push(format!(
                "{}/{} ↓↑ cached",
                crate::tui::labels::format_token_count(self.cache_read_tokens),
                crate::tui::labels::format_token_count(self.cache_write_tokens),
            ));
        }
        parts.join(" · ")
    }
}

/// Dimmed transcript line rendered under the last assistant message when a turn finishes.
///
/// Examples:
/// - `turn: 1m50s · 3.1K in · 2.4K out · 1.2K cached · anthropic/claude-sonnet-4`
/// - `turn: 12s` (no usage / model reported)
pub fn format_turn_complete_stats_line(stats: &TurnCompleteStats) -> String {
    let mut parts = vec![format!("turn: {}", format_duration_secs(stats.elapsed_secs))];
    let tokens = stats.tokens_line();
    if !tokens.is_empty() {
        parts.push(tokens);
    }
    if let Some(model) = stats.model_label() {
        parts.push(model);
    }
    parts.join(" · ")
}

/// Idle status notice shown briefly after the user cancels an active turn.
pub fn format_turn_canceled_notice(elapsed_secs: f64) -> String {
    format!("Turn canceled · {}", format_duration_secs(elapsed_secs))
}

/// Idle status notice shown briefly after the user cancels a `!` / `!!` shell command.
pub fn format_shell_canceled_notice(elapsed_secs: f64) -> String {
    format!("Command canceled · {}", format_duration_secs(elapsed_secs))
}

/// StatusRow label while a user shell command (`!` / `!!`) is running.
pub fn user_shell_activity_label(command: &str) -> String {
    format!("Running shell_exec({})", truncate_status(command.trim(), 28))
}

/// Banner text when quit is requested while a turn is still running (fixed above StatusRow).
pub fn format_quit_while_busy_transcript() -> String {
    "Agent is still responding. Press y to quit, n to keep waiting, or Ctrl+D to force quit.".to_string()
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
                    user_shell: false,
                },
                false
            ),
            Some("Running Read".to_string())
        );
        assert_eq!(
            activity_label_for_event(
                &AgentUiEvent::ToolStart {
                    id: "2".into(),
                    name: "wait_agent".into(),
                    args_summary: "{}".into(),
                    user_shell: false,
                },
                false
            ),
            Some("Waiting for subagent".to_string())
        );
    }

    #[test]
    fn braille_spinner_cycles() {
        assert_eq!(braille_spinner_glyph(0), "⠋");
        assert_eq!(braille_spinner_glyph(1), "⠙");
        // Wall-clock helper always returns a known braille frame.
        let now = braille_spinner_glyph_now();
        assert!(["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"].contains(&now));
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
    fn format_duration_secs_uses_ns_us_ms_under_one_second() {
        assert_eq!(format_duration_secs(0.0), "0ns");
        assert_eq!(format_duration_secs(0.000_000_42), "420ns");
        assert_eq!(format_duration_secs(0.000_4), "400us");
        assert_eq!(format_duration_secs(0.045), "45ms");
        assert_eq!(format_duration_secs(0.5), "500ms");
        assert_eq!(format_duration_secs(0.999), "999ms");
        // Never collapse tiny intervals to `0ms` / `0s`.
        assert_ne!(format_duration_secs(0.000_4), "0ms");
        assert_ne!(format_duration_secs(0.0), "0ms");
        assert_ne!(format_duration_secs(0.0), "0s");
    }

    #[test]
    fn format_duration_secs_uses_tenths_under_ten_seconds() {
        assert_eq!(format_duration_secs(1.0), "1s");
        assert_eq!(format_duration_secs(1.24), "1.2s");
        assert_eq!(format_duration_secs(9.95), "10s"); // rounds into whole-second band
    }

    #[test]
    fn format_duration_secs_uses_whole_seconds_under_one_minute() {
        assert_eq!(format_duration_secs(10.0), "10s");
        assert_eq!(format_duration_secs(12.4), "12s");
        assert_eq!(format_duration_secs(45.0), "45s");
        assert_eq!(format_duration_secs(59.4), "59s");
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
        assert_eq!(format_duration_label_suffix(0.045), " · 45ms");
        assert_eq!(format_duration_label_suffix(1.2), " · 1.2s");
        assert_eq!(format_duration_label_suffix(110.0), " · 1m50s");
    }

    #[test]
    fn format_activity_busy_line_includes_elapsed() {
        assert_eq!(format_activity_busy_line("Thinking", 1.2), "Thinking · 1.2s");
        assert_eq!(format_activity_busy_line("Wait Agent", 0.08), "Wait Agent · 80ms");
    }

    #[test]
    fn thinking_overdue_label_none_below_threshold() {
        assert_eq!(thinking_overdue_label(0.0), None);
        assert_eq!(thinking_overdue_label(9.9), None);
        assert_eq!(thinking_overdue_label(f64::NAN), None);
    }

    #[test]
    fn thinking_overdue_label_escalates_by_ten_second_buckets() {
        // Same 10s bucket maps to the same (shuffled) message, so the label
        // stays stable within a phase.
        assert_eq!(thinking_overdue_label(10.0), thinking_overdue_label(19.9));
        // Adjacent buckets escalate to different messages from the pool.
        let b1 = thinking_overdue_label(10.0).expect("overdue");
        let b2 = thinking_overdue_label(20.0).expect("overdue");
        let b3 = thinking_overdue_label(30.0).expect("overdue");
        assert_ne!(b1, b2);
        assert_ne!(b2, b3);
        // Very long phases clamp to the last message of the shuffled order.
        assert_eq!(thinking_overdue_label(3600.0), thinking_overdue_label(4200.0));
        // Every label is drawn from the pool.
        for label in [b1, b2, b3, thinking_overdue_label(3600.0).expect("overdue")] {
            assert!(OVERDUE_THINKING_MESSAGES.contains(&label), "{label:?} not in pool");
        }
    }

    #[test]
    fn format_activity_busy_line_dynamic_escalates_only_thinking() {
        // Below threshold: unchanged.
        assert_eq!(format_activity_busy_line_dynamic("Thinking", 5.0), "Thinking · 5s");
        // Past threshold: a pool message replaces the activity name, timer kept.
        let busy = format_activity_busy_line_dynamic("Thinking", 11.0);
        let (witty, duration) = busy.split_once(" · ").expect("timer suffix");
        assert_eq!(duration, "11s");
        assert!(OVERDUE_THINKING_MESSAGES.contains(&witty), "{witty:?} not in pool");
        // Non-thinking labels are untouched.
        assert_eq!(format_activity_busy_line_dynamic("Running grep", 31.0), "Running grep · 31s");
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
    fn format_shell_canceled_notice_includes_elapsed() {
        assert_eq!(format_shell_canceled_notice(1.5), "Command canceled · 1.5s");
    }

    #[test]
    fn user_shell_activity_label_describes_running_command() {
        assert_eq!(user_shell_activity_label("cargo test"), "Running shell_exec(cargo test)");
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
        // Sub-second uses ms; 1–10s uses tenths (see `format_duration_secs`).
        assert_eq!(format_activity_busy_line("Running grep", 0.8), "Running grep · 800ms");
        assert_eq!(format_activity_busy_line("Thinking", 1.2), "Thinking · 1.2s");
        assert_eq!(format_activity_busy_line("", 2.5), "2.5s");
    }

    #[test]
    fn format_session_busy_right_line_shows_total_and_cancel() {
        // 40 + 18.1 = 58.1 → whole seconds in the 10s–59s band.
        assert_eq!(format_session_busy_right_line(40.0, 18.1, false), "58s total · Ctrl+C cancel");
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
        assert!(notice.contains("n"));
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

    #[test]
    fn turn_complete_stats_from_event_maps_usage() {
        let stats = TurnCompleteStats::from_event(
            110.0,
            Some(&elph_agent::TurnUsage {
                input_tokens: 3100,
                output_tokens: 2400,
                cache_read_tokens: 1200,
                cache_write_tokens: 0,
                total_tokens: 6700,
                cost: 0.0123,
            }),
            Some("anthropic"),
            Some("claude-sonnet-4"),
        );
        assert_eq!(stats.input_tokens, 3100);
        assert_eq!(stats.output_tokens, 2400);
        assert_eq!(stats.cache_read_tokens, 1200);
        assert!(stats.input_tokens > 0 && stats.cache_read_tokens > 0);
        assert_eq!(stats.model_label().as_deref(), Some("anthropic/claude-sonnet-4"));
        assert_eq!(
            format_turn_complete_stats_line(&stats),
            "turn: 1m50s · 3K/2K ↓↑ · 1K/0 ↓↑ cached · anthropic/claude-sonnet-4"
        );
    }

    #[test]
    fn turn_complete_stats_cache_write_shows_read_write_pair() {
        let stats = TurnCompleteStats::from_event(5.0, None, None, None);
        let mut stats = stats;
        stats.input_tokens = 500;
        stats.output_tokens = 700;
        stats.cache_write_tokens = 2_500;
        assert_eq!(
            format_turn_complete_stats_line(&stats),
            "turn: 5s · 500/700 ↓↑ · 0/2K ↓↑ cached"
        );
    }

    #[test]
    fn turn_complete_stats_without_usage_is_duration_only() {
        let stats = TurnCompleteStats::from_event(12.0, None, None, None);
        assert_eq!((stats.input_tokens, stats.output_tokens, stats.cache_read_tokens), (0, 0, 0));
        assert_eq!(stats.model_label(), None);
        assert_eq!(format_turn_complete_stats_line(&stats), "turn: 12s");
    }

    #[test]
    fn turn_complete_stats_model_label_handles_partial_ids() {
        let stats = TurnCompleteStats::from_event(1.0, None, None, Some("gpt-5.6-luna"));
        assert_eq!(stats.model_label().as_deref(), Some("gpt-5.6-luna"));
        let empty = TurnCompleteStats::from_event(1.0, None, Some("openai"), None);
        assert_eq!(empty.model_label(), None);
    }
}
