use std::io::{IsTerminal, stdout};

use elph_agent::session::types::SessionTreeEntry;
use elph_agent::types::AgentMessage;
use elph_ai::{Message, StopReason, Usage};
use parking_lot::Mutex;

const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

static PENDING: Mutex<Option<ExitSnapshot>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq)]
pub struct ExitSnapshot {
    pub session_id: String,
    pub cost_usd: f64,
    pub api_duration_secs: f64,
    pub wall_duration_secs: f64,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub usage: UsageTotals,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageTotals {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

pub fn record(snapshot: ExitSnapshot) {
    *PENDING.lock() = Some(snapshot);
}

pub fn print_and_clear() {
    let snapshot = PENDING.lock().take();
    let Some(snapshot) = snapshot else {
        return;
    };
    print_exit_summary(&snapshot);
}

pub fn print_exit_summary(snapshot: &ExitSnapshot) {
    println!("Resume this session: elph --resume {}", snapshot.session_id);
    println_dim(format!("Estimated cost:        ${:.2}", snapshot.cost_usd));
    println_dim(format!(
        "Total duration (API):  {}",
        format_duration_secs(snapshot.api_duration_secs)
    ));
    println_dim(format!(
        "Total duration (wall): {}",
        format_duration_secs(snapshot.wall_duration_secs)
    ));
    println_dim(format!(
        "Total code changes:    {} lines added, {} lines removed",
        snapshot.lines_added, snapshot.lines_removed
    ));
    println_dim(format!(
        "Usage stats:           {} input, {} output, {} cache read, {} cache write",
        snapshot.usage.input, snapshot.usage.output, snapshot.usage.cache_read, snapshot.usage.cache_write
    ));
}

fn println_dim(line: impl AsRef<str>) {
    println!("{}", dim(line.as_ref()));
}

fn dim(text: &str) -> String {
    if supports_ansi_color() {
        format!("{DIM}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn supports_ansi_color() -> bool {
    std::env::var("NO_COLOR").as_deref() != Ok("true") && stdout().is_terminal()
}

pub fn aggregate_usage_from_entries(entries: &[SessionTreeEntry]) -> (UsageTotals, f64) {
    let mut totals = UsageTotals::default();
    let mut cost_usd = 0.0;
    for entry in entries {
        let SessionTreeEntry::Message { message, .. } = entry else {
            continue;
        };
        let Some(usage) = assistant_usage(message) else {
            continue;
        };
        totals.input += usage.input;
        totals.output += usage.output;
        totals.cache_read += usage.cache_read;
        totals.cache_write += usage.cache_write;
        cost_usd += usage.cost.total;
    }
    (totals, cost_usd)
}

fn assistant_usage(message: &AgentMessage) -> Option<&Usage> {
    let AgentMessage::Llm(llm) = message else {
        return None;
    };
    let Message::Assistant(assistant) = llm.as_ref() else {
        return None;
    };
    if matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error) {
        return None;
    }
    if assistant.usage.input == 0
        && assistant.usage.output == 0
        && assistant.usage.cache_read == 0
        && assistant.usage.cache_write == 0
        && assistant.usage.cost.total == 0.0
    {
        return None;
    }
    Some(&assistant.usage)
}

pub fn format_duration_secs(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.5 {
        "0s".to_string()
    } else {
        format!("{}s", secs.round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_rounds_to_seconds() {
        assert_eq!(format_duration_secs(0.0), "0s");
        assert_eq!(format_duration_secs(0.4), "0s");
        assert_eq!(format_duration_secs(51.2), "51s");
    }

    #[test]
    fn prints_codex_style_exit_block() {
        let snapshot = ExitSnapshot {
            session_id: "abc123xyz4567".to_string(),
            cost_usd: 0.0,
            api_duration_secs: 0.0,
            wall_duration_secs: 51.0,
            lines_added: 0,
            lines_removed: 0,
            usage: UsageTotals::default(),
        };
        print_exit_summary(&snapshot);
    }
}
