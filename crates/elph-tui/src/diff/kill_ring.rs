//! Emacs-style kill ring for yank / yank-pop.

/// Maximum number of kill entries retained (Emacs default is 60).
const MAX_KILL_RING: usize = 60;

/// Ring buffer for killed text.
#[derive(Debug, Default)]
pub struct KillRing {
    ring: Vec<String>,
}

impl KillRing {
    pub fn push(&mut self, text: &str, prepend: bool, accumulate: bool) {
        if text.is_empty() {
            return;
        }
        if accumulate && let Some(last) = self.ring.pop() {
            let merged = if prepend {
                format!("{text}{last}")
            } else {
                format!("{last}{text}")
            };
            self.ring.push(merged);
        } else {
            self.ring.push(text.to_string());
        }
        if self.ring.len() > MAX_KILL_RING {
            let excess = self.ring.len() - MAX_KILL_RING;
            self.ring.drain(..excess);
        }
    }

    pub fn peek(&self) -> Option<&str> {
        self.ring.last().map(String::as_str)
    }

    pub fn rotate(&mut self) {
        if self.ring.len() > 1 {
            let last = self.ring.pop().expect("len > 1");
            self.ring.insert(0, last);
        }
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_consecutive_kills() {
        let mut ring = KillRing::default();
        ring.push("ab", false, false);
        ring.push("cd", false, true);
        assert_eq!(ring.peek(), Some("abcd"));
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn drops_oldest_entries_when_full() {
        let mut ring = KillRing::default();
        for i in 0..70 {
            ring.push(&format!("kill-{i}"), false, false);
        }
        assert_eq!(ring.len(), 60);
        assert_eq!(ring.peek(), Some("kill-69"));
    }
}
