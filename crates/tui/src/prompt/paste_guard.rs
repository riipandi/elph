use super::prompt_paste::{PASTE_BURST_GAP, TAB_PASTE_GAP};
use std::time::Instant;

/// Rapid single-char inserts in a row that likely indicate a char-by-char paste.
const PASTE_LIKELY_MIN_RAPID_INSERTS: u32 = 3;

/// Tracks clipboard paste so Enter pressed as part of a paste does not submit the prompt.
#[derive(Debug, Clone, Copy, Default)]
pub struct PasteGuard {
    block_next_submit: bool,
    last_change_at: Option<Instant>,
    rapid_insert_count: u32,
    paste_likely: bool,
}

impl PasteGuard {
    /// Call when the prompt value changes; pass previous and next lengths.
    pub fn record_change(&mut self, prev_len: usize, next_len: usize, at: Instant) {
        let delta = next_len.saturating_sub(prev_len);

        if delta > 1 {
            self.block_next_submit = true;
            self.paste_likely = true;
        }

        if delta > 0 {
            if let Some(last) = self.last_change_at {
                if at.duration_since(last) < PASTE_BURST_GAP {
                    self.rapid_insert_count = self.rapid_insert_count.saturating_add(1);
                } else {
                    self.rapid_insert_count = 1;
                }
            } else {
                self.rapid_insert_count = 1;
            }

            if self.rapid_insert_count >= PASTE_LIKELY_MIN_RAPID_INSERTS {
                self.paste_likely = true;
            }

            self.last_change_at = Some(at);
        } else if next_len < prev_len {
            self.last_change_at = None;
            self.rapid_insert_count = 0;
            self.paste_likely = false;
        }
    }

    pub fn clear(&mut self) {
        self.block_next_submit = false;
        self.last_change_at = None;
        self.rapid_insert_count = 0;
        self.paste_likely = false;
    }

    /// Returns `true` while keystrokes are still arriving as one paste burst.
    pub fn is_in_burst(&self, at: Instant) -> bool {
        self.last_change_at
            .is_some_and(|last| at.duration_since(last) < PASTE_BURST_GAP)
    }

    /// Returns `true` when recent input looked like a clipboard paste.
    pub fn is_paste_likely(&self, at: Instant) -> bool {
        self.paste_likely
            && self
                .last_change_at
                .is_some_and(|last| at.duration_since(last) < TAB_PASTE_GAP)
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
    use std::time::Duration;

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
    fn tracks_burst_window() {
        let (t0, t_mid, t_late) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1, t0);
        assert!(guard.is_in_burst(t_mid));
        assert!(!guard.is_in_burst(t_late));
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

    #[test]
    fn rapid_inserts_mark_paste_likely() {
        let t0 = Instant::now();
        let mut guard = PasteGuard::default();
        for len in 1..=4 {
            guard.record_change(len - 1, len, t0 + Duration::from_millis(len as u64));
        }
        assert!(guard.is_paste_likely(t0 + Duration::from_millis(200)));
        assert!(!guard.is_paste_likely(t0 + Duration::from_secs(2)));
    }
}
