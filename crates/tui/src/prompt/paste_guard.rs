/// Tracks bulk text insertion (e.g. clipboard paste) to suppress one accidental submit.
#[derive(Debug, Clone, Copy, Default)]
pub struct PasteGuard {
    block_next_submit: bool,
}

impl PasteGuard {
    /// Call when the prompt value changes; pass previous and next lengths.
    pub fn record_change(&mut self, prev_len: usize, next_len: usize) {
        if next_len > prev_len.saturating_add(1) {
            self.block_next_submit = true;
        }
    }

    /// Returns `true` once after a bulk insert; clears the flag.
    pub fn consume_submit_block(&mut self) -> bool {
        let blocked = self.block_next_submit;
        self.block_next_submit = false;
        blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_keystroke_does_not_block_submit() {
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1);
        assert!(!guard.consume_submit_block());
    }

    #[test]
    fn bulk_insert_blocks_next_submit_once() {
        let mut guard = PasteGuard::default();
        guard.record_change(0, 12);
        assert!(guard.consume_submit_block());
        assert!(!guard.consume_submit_block());
    }

    #[test]
    fn rapid_single_keystrokes_do_not_block_submit() {
        let mut guard = PasteGuard::default();
        guard.record_change(0, 1);
        guard.record_change(1, 2);
        guard.record_change(2, 3);
        assert!(!guard.consume_submit_block());
    }
}
