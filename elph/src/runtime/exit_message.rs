use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const GOODBYES: &[&str] = &[
    "Goodbye — come back anytime.",
    "See you next time.",
    "Until next time. Happy coding!",
    "Take care!",
    "Bye for now.",
    "Catch you later.",
    "All done here. See you soon.",
    "Signing off. Have a great day!",
    "Later! The codebase will be here when you return.",
    "Peace out.",
];

static PENDING: Mutex<Option<ExitSnapshot>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitSnapshot {
    pub session_id: String,
    pub has_history: bool,
}

/// Creates a TSID session identifier for resume hints.
pub fn new_session_id() -> String {
    tsid::create_tsid().to_string()
}

pub fn record(snapshot: ExitSnapshot) {
    if let Ok(mut pending) = PENDING.lock() {
        *pending = Some(snapshot);
    }
}

pub fn print_and_clear() {
    let snapshot = PENDING.lock().ok().and_then(|mut pending| pending.take());
    let Some(snapshot) = snapshot else {
        return;
    };

    if snapshot.has_history {
        println!("To resume this session: elph --resume {}", snapshot.session_id);
    } else {
        println!("{}", random_goodbye());
    }
}

pub fn random_goodbye() -> &'static str {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    goodbye_message(seed)
}

pub fn goodbye_message(seed: u64) -> &'static str {
    GOODBYES[seed as usize % GOODBYES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_is_tsid() {
        let id = new_session_id();
        assert_eq!(id.len(), 13);
        assert!(tsid::TSID::try_from(id.as_str()).is_ok());
    }

    #[test]
    fn goodbye_rotates_with_seed() {
        assert_ne!(goodbye_message(0), goodbye_message(1));
        assert_eq!(goodbye_message(42), goodbye_message(42 + GOODBYES.len() as u64));
    }

    #[test]
    fn prints_resume_hint_when_history_exists() {
        record(ExitSnapshot {
            session_id: "abc123".to_string(),
            has_history: true,
        });
        print_and_clear();
        // Printed to stdout; ensure buffer clears without panic on second call.
        print_and_clear();
    }
}
