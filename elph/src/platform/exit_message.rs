use std::io::IsTerminal;
use std::io::stdout;

use elph_agent::session::types::SessionTreeEntry;
use elph_agent::types::AgentMessage;
use elph_ai::{Message, StopReason, Usage};
use parking_lot::Mutex;

const DIM: &str = "\x1b[2m";
const ORANGE: &str = "\x1b[38;2;249;115;22m";
const WHITE: &str = "\x1b[37m";
const RESET: &str = "\x1b[0m";

static PENDING: Mutex<Option<ExitSnapshot>> = Mutex::new(None);

const GOODBYE_MESSAGES: &[&str] = &[
    "Until next time — the codebase will miss you.",
    "Session closed. Your tabs are still open though.",
    "Done for now. Coffee recommended.",
    "Signing off — may your builds be green.",
    "That's a wrap. See you in the next commit.",
    "Ciao for now.",
    "Logging off. Resume anytime.",
    "Peace out — the elves will keep watch.",
    "Catch you later. Happy shipping.",
    "All quiet on the terminal front. Bye for now.",
];

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

/// True when the user actually participated: submitted a prompt this session or the
/// persisted branch already contains user turns (e.g. after `--resume`).
pub fn session_had_user_activity(submitted_prompt_count: u32, persisted_turn_count: u32) -> bool {
    submitted_prompt_count > 0 || persisted_turn_count > 0
}

pub fn record(snapshot: ExitSnapshot) {
    *PENDING.lock() = Some(snapshot);
}

/// Record the exit summary only when the session had real user participation.
pub fn record_if_active(snapshot: ExitSnapshot, submitted_prompt_count: u32, persisted_turn_count: u32) {
    if !session_had_user_activity(submitted_prompt_count, persisted_turn_count) {
        return;
    }
    record(snapshot);
}

pub fn print_and_clear() {
    let snapshot = PENDING.lock().take();
    let Some(snapshot) = snapshot else {
        return;
    };
    print_exit_summary(&snapshot);
}

pub fn print_exit_summary(snapshot: &ExitSnapshot) {
    println_orange(format!("\n{}", pick_goodbye_message(&snapshot.session_id)));
    println_white(format!("\nResume this session: elph --resume {}", snapshot.session_id));
    println_dim(format!("\nTotal cost            : ${:.4}", snapshot.cost_usd));
    println_dim(format!(
        "Total duration (API)  : {}",
        format_duration_secs(snapshot.api_duration_secs)
    ));
    println_dim(format!(
        "Total duration (wall) : {}",
        format_duration_secs(snapshot.wall_duration_secs)
    ));
    println_dim(format!(
        "Total code changes    : {} lines added, {} lines removed\n",
        snapshot.lines_added, snapshot.lines_removed
    ));
}

pub fn pick_goodbye_message(session_id: &str) -> &'static str {
    let mut hash = 0u64;
    for byte in session_id.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(u64::from(byte));
    }
    GOODBYE_MESSAGES[hash as usize % GOODBYE_MESSAGES.len()]
}

fn println_dim(line: impl AsRef<str>) {
    println!("{}", dim(line.as_ref()));
}

fn println_orange(line: impl AsRef<str>) {
    if supports_ansi_color() {
        println!("{ORANGE}{}{RESET}", line.as_ref());
    } else {
        println!("{}", line.as_ref());
    }
}

fn println_white(line: impl AsRef<str>) {
    if supports_ansi_color() {
        println!("{WHITE}{}{RESET}", line.as_ref());
    } else {
        println!("{}", line.as_ref());
    }
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
    fn session_had_user_activity_requires_input_or_turns() {
        assert!(!session_had_user_activity(0, 0));
        assert!(session_had_user_activity(1, 0));
        assert!(session_had_user_activity(0, 2));
    }

    #[test]
    fn pick_goodbye_is_stable_for_session_id() {
        let first = pick_goodbye_message("00000012abc01w01");
        let second = pick_goodbye_message("00000012abc01w01");
        assert_eq!(first, second);
        assert!(GOODBYE_MESSAGES.contains(&first));
    }

    #[test]
    fn prints_codex_style_exit_block() {
        let snapshot = ExitSnapshot {
            session_id: "00000012abc01w01".to_string(),
            cost_usd: 0.1548,
            api_duration_secs: 10.0,
            wall_duration_secs: 17.0,
            lines_added: 0,
            lines_removed: 0,
            usage: UsageTotals::default(),
        };
        print_exit_summary(&snapshot);
    }
}
