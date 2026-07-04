//! Heuristic paste-burst detection for terminals without bracketed paste.

use std::time::{Duration, Instant};

const MIN_CHARS: u32 = 8;
const CHAR_INTERVAL: Duration = Duration::from_millis(8);
const ACTIVE_IDLE: Duration = Duration::from_millis(30);
const ENTER_SUPPRESS: Duration = Duration::from_millis(120);

/// Detects rapid single-character streams that likely represent a paste.
#[derive(Debug, Default)]
pub struct PasteBurst {
    last_plain_at: Option<Instant>,
    consecutive: u32,
    active_until: Option<Instant>,
    enter_suppress_until: Option<Instant>,
}

impl PasteBurst {
    pub fn on_plain_char(&mut self, now: Instant) {
        if self
            .last_plain_at
            .is_some_and(|last| now.duration_since(last) <= CHAR_INTERVAL)
        {
            self.consecutive += 1;
        } else {
            self.consecutive = 1;
        }
        self.last_plain_at = Some(now);

        if self.consecutive >= MIN_CHARS {
            self.extend_window(now);
        }
    }

    pub fn should_insert_newline_instead_of_submit(&self, now: Instant) -> bool {
        if self.active_until.is_some_and(|t| now <= t) {
            return true;
        }
        if self.enter_suppress_until.is_some_and(|t| now <= t) {
            return true;
        }
        self.last_plain_at
            .is_some_and(|last| self.consecutive >= MIN_CHARS && now.duration_since(last) <= CHAR_INTERVAL)
    }

    pub fn extend_window(&mut self, now: Instant) {
        self.active_until = Some(now + ACTIVE_IDLE);
        self.enter_suppress_until = Some(now + ENTER_SUPPRESS);
    }

    pub fn reset(&mut self) {
        self.last_plain_at = None;
        self.consecutive = 0;
        self.active_until = None;
        self.enter_suppress_until = None;
    }
}
