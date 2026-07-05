use super::prompt_paste::{PASTE_BURST_GAP, TAB_PASTE_GAP};
use std::time::Instant;

/// Rapid single-char inserts in a row that likely indicate a char-by-char paste.
const PASTE_ACTIVE_MIN_RAPID_INSERTS: u32 = 2;

/// Tracks clipboard paste so Enter pressed as part of a paste does not submit the prompt.
#[derive(Debug, Clone, Copy, Default)]
pub struct PasteGuard {
    block_next_submit: bool,
    last_change_at: Option<Instant>,
    rapid_insert_count: u32,
    paste_active: bool,
}

impl PasteGuard {
    /// Call when the prompt value changes; pass previous and next lengths.
    pub fn record_change(&mut self, prev_len: usize, next_len: usize, at: Instant) {
        let delta = next_len.saturating_sub(prev_len);

        if delta > 1 {
            self.block_next_submit = true;
            self.paste_active = true;
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

            if self.rapid_insert_count >= PASTE_ACTIVE_MIN_RAPID_INSERTS {
                self.paste_active = true;
            }

            self.last_change_at = Some(at);
        } else if next_len < prev_len {
            self.last_change_at = None;
            self.rapid_insert_count = 0;
            self.paste_active = false;
        }
    }

    pub fn clear(&mut self) {
        self.block_next_submit = false;
        self.last_change_at = None;
        self.rapid_insert_count = 0;
        self.paste_active = false;
    }

    /// Clears paste tracking without suppressing the next Enter.
    pub fn release_paste_active(&mut self) {
        self.paste_active = false;
        self.rapid_insert_count = 0;
        self.last_change_at = None;
    }

    /// Breaks a rapid-insert chain (e.g. typed space between collapsed paste chips).
    pub fn reset_burst_chain(&mut self) {
        self.rapid_insert_count = 0;
        self.last_change_at = None;
    }

    pub fn rapid_insert_count(&self) -> u32 {
        self.rapid_insert_count
    }

    /// Byte offset where the current rapid-insert run began (includes the first keystroke).
    pub fn burst_run_start(&self, cursor_before: usize) -> usize {
        if self.rapid_insert_count <= 1 {
            cursor_before
        } else {
            cursor_before.saturating_sub(self.rapid_insert_count as usize - 1)
        }
    }

    /// Returns `true` while keystrokes are still arriving as one paste burst.
    pub fn is_in_burst(&self, at: Instant) -> bool {
        self.last_change_at
            .is_some_and(|last| at.duration_since(last) < PASTE_BURST_GAP)
    }

    /// Returns `true` when recent input looked like a clipboard paste.
    pub fn is_paste_active(&self, at: Instant) -> bool {
        self.paste_active
            && self
                .last_change_at
                .is_some_and(|last| at.duration_since(last) < TAB_PASTE_GAP)
    }

    /// Returns `true` when submit should be suppressed for this Enter press.
    ///
    /// Burst-ending Enter after char-by-char paste is handled by [`super::prompt_paste::enter_should_finalize_paste`].
    pub fn should_block_submit(&mut self) -> bool {
        if self.block_next_submit {
            self.block_next_submit = false;
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
        let (t0, _, _) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1, t0);
        assert!(!guard.should_block_submit());
    }

    #[test]
    fn bulk_insert_blocks_next_submit_once() {
        let (t0, _, _) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 12, t0);
        assert!(guard.should_block_submit());
        assert!(!guard.should_block_submit());
    }

    #[test]
    fn rapid_single_keystrokes_do_not_block_submit() {
        let (t0, t_mid, _) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1, t0);
        guard.record_change(1, 2, t0 + Duration::from_millis(80));
        guard.record_change(2, 3, t_mid);
        assert!(!guard.should_block_submit());
    }

    #[test]
    fn bulk_insert_blocks_immediate_enter() {
        let (t0, _, _) = timeline();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 5, t0);
        assert!(guard.should_block_submit());
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
        let (t0, _, _) = timeline();
        let mut guard = PasteGuard::default();
        for len in 1..=5 {
            guard.record_change(len - 1, len, t0 + Duration::from_millis(len as u64));
        }
        assert!(!guard.should_block_submit());
    }

    #[test]
    fn rapid_inserts_mark_paste_active() {
        let t0 = Instant::now();
        let mut guard = PasteGuard::default();
        for len in 1..=3 {
            guard.record_change(len - 1, len, t0 + Duration::from_millis(len as u64));
        }
        assert!(guard.is_paste_active(t0 + Duration::from_millis(200)));
        assert!(!guard.is_paste_active(t0 + Duration::from_secs(2)));
    }

    #[test]
    fn typed_space_breaks_burst_chain() {
        let t0 = Instant::now();
        let mut guard = PasteGuard::default();
        guard.record_change(20, 21, t0);
        guard.reset_burst_chain();
        guard.record_change(21, 22, t0 + Duration::from_millis(1));
        guard.record_change(22, 23, t0 + Duration::from_millis(2));
        assert_eq!(guard.burst_run_start(23), 22);
    }

    #[test]
    fn burst_run_start_includes_first_keystroke() {
        let t0 = Instant::now();
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1, t0);
        guard.record_change(1, 2, t0 + Duration::from_millis(5));
        assert_eq!(guard.burst_run_start(1), 0);
    }

    #[test]
    fn rapid_paste_stays_active_after_short_pause() {
        let t0 = Instant::now();
        let mut guard = PasteGuard::default();
        for len in 1..=20 {
            guard.record_change(len - 1, len, t0 + Duration::from_millis(1));
        }
        let enter_at = t0 + Duration::from_millis(200);
        assert!(guard.is_paste_active(enter_at));
        assert!(!guard.should_block_submit());
    }
}
