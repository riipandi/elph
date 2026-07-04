use std::time::{Duration, Instant};

/// Minimum gap between pasted characters; terminal paste is far faster than human typing.
const PASTE_BURST_GAP: Duration = Duration::from_millis(40);

/// Tracks clipboard paste so Enter pressed as part of a paste does not submit the prompt.
#[derive(Debug, Clone, Copy, Default)]
pub struct PasteGuard {
    block_next_submit: bool,
    last_change_at: Option<Instant>,
}

impl PasteGuard {
    /// Call when the prompt value changes; pass previous and next lengths.
    pub fn record_change(&mut self, prev_len: usize, next_len: usize, at: Instant) {
        let delta = next_len.saturating_sub(prev_len);

        if delta > 1 {
            self.block_next_submit = true;
        }

        if delta > 0 {
            self.last_change_at = Some(at);
        } else if next_len < prev_len {
            self.last_change_at = None;
        }
    }

    /// Returns `true` when submit should be suppressed for this Enter press.
    pub fn should_block_submit(&mut self, at: Instant) -> bool {
        if self.block_next_submit {
            self.block_next_submit = false;
            return true;
        }

        if let Some(last) = self.last_change_at
            && at.duration_since(last) < PASTE_BURST_GAP
        {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline() -> (Instant, Instant, Instant) {
        let base = Instant::now();
        (base, base + Duration::from_millis(5), base + Duration::from_millis(100))
    }

    #[test]
    fn single_keystroke_does_not_block_submit() {
        let (t0, _, t_late) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1, t0);
        assert!(!guard.should_block_submit(t_late));
    }

    #[test]
    fn bulk_insert_blocks_next_submit_once() {
        let (t0, _, t_late) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 12, t0);
        assert!(guard.should_block_submit(t_late));
        assert!(!guard.should_block_submit(t_late));
    }

    #[test]
    fn rapid_single_keystrokes_do_not_block_submit() {
        let (t0, t_mid, t_late) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1, t0);
        guard.record_change(1, 2, t0 + Duration::from_millis(80));
        guard.record_change(2, 3, t_mid);
        assert!(!guard.should_block_submit(t_late));
    }

    #[test]
    fn enter_immediately_after_insert_is_treated_as_paste() {
        let (t0, t_enter, _) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 5, t0);
        assert!(guard.should_block_submit(t_enter));
    }

    #[test]
    fn enter_after_pause_allows_submit() {
        let (t0, _, t_late) = timeline();
        let mut guard = PasteGuard::default();
        for len in 1..=5 {
            guard.record_change(len - 1, len, t0 + Duration::from_millis(len as u64));
        }
        assert!(!guard.should_block_submit(t_late));
    }
}
